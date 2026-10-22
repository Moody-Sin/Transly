import { useEffect, useRef, useState } from 'react'
import {
  commands,
  type OcrBlockDto,
  type OcrErrorDto,
  type ScreenshotBlockResultDto,
  type ScreenshotSessionDto,
} from './bindings'
import { getTranslation, Locale } from './i18n'
import { parseLocalePreference, parseThemePreference, LOCALE_STORAGE_KEY } from './utils/prefs'
import {
  computeFloatingToolbarPosition,
  fitFontSize,
  toPercentStyle,
} from './utils/screenshotLayout'

/**
 * 轻点未拖拽的物理像素阈值，与 Rust 侧 `overlay::MIN_SELECTION_PX` 保持一致。
 * 低于此值的框选视为误触，既不提交也不取消，继续保持等待。
 */
const MIN_SELECTION_PX = 8

/** 归一化分量 → CSS 百分比。先取整到 4 位小数再转数字，避免 `0.1 * 100` 得到 `10.000000000000002`。 */
const pct = (value: number) => `${Number((value * 100).toFixed(4))}%`

const clamp01 = (value: number) => Math.min(1, Math.max(0, value))

type Point = { x: number; y: number }

/**
 * 本地选区。字段用非空 `number`：`NormRect` 的 f32 分量经 specta 生成的是
 * `number | null`（f32 无法表达 NaN/Infinity），但本地计算出的选区永远有值。
 * 结构上可直接作为 `NormRect` 参数传给命令。
 */
type Selection = { x: number; y: number; w: number; h: number }

/**
 * 截图翻译覆盖窗（`#shot`）。
 *
 * 独立窗口，故语言与主题必须自行读 `localStorage` 并监听 `storage` 事件
 * （与 `FloatWindow` 同理，次级窗口不得硬编码用户偏好）。
 *
 * 坐标契约是**归一化矩形** `[0,1]²`（原点左上）：截图的物理像素与窗口的 CSS
 * 逻辑像素在任何 DPI 下都不相等，归一化后前端只需 `left: x*100%`，全链路零缩放系数。
 */
export default function ScreenshotOverlay() {
  const [locale, setLocale] = useState<Locale>(() =>
    parseLocalePreference(localStorage.getItem(LOCALE_STORAGE_KEY)),
  )
  const t = getTranslation(locale)

  /**
   * 错误文案在**渲染时**才按当前语言包取：`OcrErrorDto::message` 是硬编码中文，
   * 直接透传会让英文界面弹中文错误（与 `App.tsx` 的 `t.ocrError[kind]` 约定一致）。
   * 把映射放在渲染期而非 setError 处，还使错误文案能随用户切换语言即时更新。
   */
  const formatOcrError = (err: OcrErrorDto | string) =>
    typeof err === 'string' ? err : t.ocrError[err.kind] || err.message

  /** 渲染期按来源选文案表（放在渲染期可随用户切语言即时更新）。 */
  const renderError = (err: OverlayError | string): string => {
    if (typeof err === 'string') return err
    if (err.source === 'ocr') return formatOcrError(err.dto)
    return err.message
  }

  const [session, setSession] = useState<ScreenshotSessionDto | null>(null)
  // 两类错误来源：OCR（OcrErrorDto）与翻译（TranslateErrorDto），
  // 它们的 `kind` 是不同枚举，故用联合类型 + 各自的文案表。
  type OverlayError = { source: 'ocr'; dto: OcrErrorDto } | { source: 'translate'; message: string }
  const [error, setError] = useState<OverlayError | string | null>(null)
  const [anchor, setAnchor] = useState<Point | null>(null)
  const [pointer, setPointer] = useState<Point | null>(null)
  const [blocks, setBlocks] = useState<OcrBlockDto[]>([])
  const [busy, setBusy] = useState(false)
  const [showTranslation, setShowTranslation] = useState(true)
  const [closed, setClosed] = useState(false)
  const dragging = useRef(false)

  // 译文与引擎/语言状态
  const [targetLang, setTargetLang] = useState('zh-CN')
  const [engine, setEngine] = useState('google')
  const [results, setResults] = useState<Map<number, ScreenshotBlockResultDto>>(new Map())
  const [translating, setTranslating] = useState(false)
  const [copyState, setCopyState] = useState<'idle' | 'ok' | 'fail'>('idle')
  const [copiedBlockIndex, setCopiedBlockIndex] = useState<number | null>(null)
  // 翻译代际：与 Rust 侧的**会话** generation 是两回事。
  // 会话代际防「陈旧窗口」，本代际防「旧译文覆盖新译文」（切语言/切引擎时）。
  const translateGen = useRef(0)

  const copyBlock = (content: string, index: number) => {
    const clean = content.trim()
    if (!clean) return
    navigator.clipboard
      .writeText(clean)
      .then(() => {
        setCopiedBlockIndex(index)
        setTimeout(() => setCopiedBlockIndex(null), 1500)
      })
      .catch(() => {})
  }

  // 跟随主窗口切换界面语言
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key !== LOCALE_STORAGE_KEY) return
      setLocale(parseLocalePreference(e.newValue))
    }
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [])

  // 同步暗黑模式主题类名
  useEffect(() => {
    const syncTheme = () => {
      const theme = parseThemePreference(localStorage.getItem('transly_theme'))
      const isDark =
        theme === 'dark' ||
        (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
      document.documentElement.classList.toggle('dark', isDark)
    }
    syncTheme()
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    mediaQuery.addEventListener('change', syncTheme)
    window.addEventListener('storage', syncTheme)
    return () => {
      mediaQuery.removeEventListener('change', syncTheme)
      window.removeEventListener('storage', syncTheme)
    }
  }, [])

  // 拉取会话。后端是幂等读，`React.StrictMode` 的二次挂载是安全的。
  useEffect(() => {
    let alive = true
    commands
      .getScreenshotSession()
      .then((res) => {
        if (!alive) return
        if (res.status === 'ok') {
          setSession(res.data)
          setTargetLang(res.data.target_lang)
          setEngine(res.data.engine)
        } else {
          setError({ source: 'ocr', dto: res.error })
        }
      })
      .catch((e: unknown) => {
        if (alive) setError(e instanceof Error ? e.message : String(e))
      })
    return () => {
      alive = false
    }
  }, [])

  const cancel = () => {
    // 先卸载自身再通知后端：真实运行时后端会销毁窗口，但在无 Tauri 运行时的
    // 前端 E2E 里只有本地状态能证明「覆盖层已消失」。
    setClosed(true)
    commands.cancelScreenshotTranslation().catch(() => {})
  }

  const reselect = () => {
    setAnchor(null)
    setPointer(null)
    setBlocks([])
    setResults(new Map())
    setError(null)
  }

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        setClosed(true)
        commands.cancelScreenshotTranslation().catch(() => {})
      } else if (e.key === ' ' || e.code === 'Space') {
        // 空格键极速切换原译文（对标有道翻译单手秒切对比）
        const tag = (e.target as HTMLElement | null)?.tagName
        if (tag !== 'SELECT' && tag !== 'INPUT' && blocks.length > 0) {
          e.preventDefault()
          setShowTranslation((v) => !v)
        }
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [blocks.length])

  /** 指针位置 → 归一化坐标。钳制在信任边界上做一次，Rust 侧还会再钳一次。 */
  const toNormalized = (e: React.PointerEvent): Point => ({
    x: clamp01(e.clientX / window.innerWidth),
    y: clamp01(e.clientY / window.innerHeight),
  })

  const onPointerDown = (e: React.PointerEvent) => {
    // 鼠标右键一键快速取消退出截图覆盖窗（贴合主流截图工具桌面操作习惯）
    if (e.button === 2) {
      cancel()
      return
    }
    if (busy || e.button !== 0) return
    const point = toNormalized(e)
    dragging.current = true
    setError(null)
    setAnchor(point)
    setPointer(point)
    e.currentTarget.setPointerCapture(e.pointerId)
  }

  const onPointerMove = (e: React.PointerEvent) => {
    if (!dragging.current) return
    setPointer(toNormalized(e))
  }

  const onPointerUp = (e: React.PointerEvent) => {
    if (!dragging.current) return
    dragging.current = false
    const from = anchor
    const to = toNormalized(e)
    if (!from) return

    // 取 min 以支持四向拖拽：先定左上角，再算宽高。
    const rect: Selection = {
      x: Math.min(from.x, to.x),
      y: Math.min(from.y, to.y),
      w: Math.abs(to.x - from.x),
      h: Math.abs(to.y - from.y),
    }
    setPointer(to)

    // 轻点未拖拽不算错误：清掉选区，保持等待用户重新框选。
    const physicalWidth = rect.w * (session?.image_width ?? 0)
    const physicalHeight = rect.h * (session?.image_height ?? 0)
    if (physicalWidth < MIN_SELECTION_PX || physicalHeight < MIN_SELECTION_PX) {
      setAnchor(null)
      setPointer(null)
      return
    }

    void submit(rect)
  }

  const submit = async (rect: Selection) => {
    if (!session) return
    setBusy(true)
    setError(null)
    setBlocks([])
    setResults(new Map())
    try {
      const res = await commands.recognizeScreenshotSelection(rect, session.generation, targetLang)
      if (res.status === 'error') {
        setError({ source: 'ocr', dto: res.error })
        return
      }
      setBlocks(res.data)
      if (res.data.length > 0) {
        // 识别完成即自动翻译（用户不必再点一次）
        await translate(res.data, targetLang, engine)
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  /**
   * 批量翻译给定块。
   *
   * 切语言/切引擎时**只重发翻译，不重跑 OCR**（块文本已在内存，OCR 又贵又不会变）。
   */
  const translate = async (target: OcrBlockDto[], lang: string, eng: string, only?: number[]) => {
    if (!session || target.length === 0) return
    const mine = ++translateGen.current
    setTranslating(true)
    setError(null)
    try {
      const subset = only ? target.filter((b) => only.includes(b.index)) : target
      const res = await commands.translateScreenshotBlocks(
        subset.map((b) => b.text),
        lang,
        eng,
        session.generation,
      )
      // 代际闸门：期间用户又切了语言/引擎或重新框选，则丢弃本次结果。
      if (mine !== translateGen.current) return
      if (res.status === 'error') {
        // 翻译错误的 kind 是 ErrorKind（非 OcrErrorKind），用 t.error 表映射。
        const errMap = t.error as Record<string, string>
        setError({ source: 'translate', message: errMap[res.error.kind] || res.error.message })
        return
      }
      // ⚠️ 按 index 回填，不能按数组顺序硬贴：本次只翻了 subset，
      // 且失败项占位后顺序可能与块顺序不一致。
      setResults((prev) => {
        const next = new Map(prev)
        subset.forEach((b, i) => {
          const r = res.data[i]
          if (r) next.set(b.index, { ...r, index: b.index })
        })
        return next
      })
    } catch (e: unknown) {
      if (mine === translateGen.current) {
        setError(e instanceof Error ? e.message : String(e))
      }
    } finally {
      if (mine === translateGen.current) setTranslating(false)
    }
  }

  /** 提取当前展示文本（支持还原原文状态下复制原文、译文模式下优先译文，失败或为空时回退原文）。 */
  const getSelectedText = () =>
    blocks
      .map((b) => {
        if (!showTranslation) return b.text
        const r = results.get(b.index)
        return r && !r.failed && r.text ? r.text : b.text
      })
      .filter((text) => text.trim().length > 0)
      .join('\n')

  const selection: Selection | null =
    anchor && pointer
      ? {
          x: Math.min(anchor.x, pointer.x),
          y: Math.min(anchor.y, pointer.y),
          w: Math.abs(pointer.x - anchor.x),
          h: Math.abs(pointer.y - anchor.y),
        }
      : null

  const toolbarPos =
    selection && selection.w > 0 && selection.h > 0
      ? computeFloatingToolbarPosition(
          {
            x: selection.x * window.innerWidth,
            y: selection.y * window.innerHeight,
            w: selection.w * window.innerWidth,
            h: selection.h * window.innerHeight,
          },
          { width: window.innerWidth, height: window.innerHeight },
          { width: 520, height: 42 },
          8,
          8,
        )
      : null

  if (closed) return null

  const physicalWidth = selection ? Math.round(selection.w * (session?.image_width ?? 0)) : 0
  const physicalHeight = selection ? Math.round(selection.h * (session?.image_height ?? 0)) : 0
  // 从屏幕最顶端开始框选时，提示条不能挂到视口外——外层 `overflow-hidden` 会把它整条裁掉。
  const sizeHintBelowTop = selection != null && selection.y * window.innerHeight < 32

  return (
    <div
      data-testid="shot-surface"
      role="application"
      aria-label={t.screenshot.title}
      className="fixed inset-0 cursor-crosshair touch-none select-none overflow-hidden bg-slate-950"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        dragging.current = false
      }}
      onContextMenu={(e) => {
        e.preventDefault()
        cancel()
      }}
    >
      {session && (
        <img
          data-testid="shot-frame"
          src={session.image_url}
          alt=""
          draggable={false}
          className="pointer-events-none absolute inset-0 h-full w-full object-fill select-none"
        />
      )}

      {selection && (
        <div
          data-testid="shot-selection"
          className="pointer-events-none absolute border border-indigo-400 shadow-[0_0_0_100vmax_rgba(15,23,42,0.45)]"
          style={{
            left: pct(selection.x),
            top: pct(selection.y),
            width: pct(selection.w),
            height: pct(selection.h),
          }}
        >
          {/* 四角 6px 装饰方块：纯视觉，不做拖拽改尺寸（重选比拖手柄快） */}
          {[
            '-left-[3px] -top-[3px]',
            '-right-[3px] -top-[3px]',
            '-bottom-[3px] -left-[3px]',
            '-bottom-[3px] -right-[3px]',
          ].map((position) => (
            <span key={position} className={`absolute h-1.5 w-1.5 bg-indigo-400 ${position}`} />
          ))}
        </div>
      )}

      {showTranslation &&
        blocks.map((block) => {
          const r = results.get(block.index)
          const failed = r?.failed === true
          const text = r && !r.failed ? r.text : block.text
          // 字号按「原文框在视口中的 CSS 逻辑像素」自适应，避免高分屏 (Retina / Windows 150%)
          // 因混用物理像素导致字号严重虚大并被 overflow-hidden 裁切。
          const boxPx = {
            w: (block.w ?? 0) * window.innerWidth,
            h: (block.h ?? 0) * window.innerHeight,
          }
          const fitted = fitFontSize(text, boxPx, { maxPx: 22, minPx: 9 })
          const fontSize = fitted ?? 9
          return (
            <div
              key={block.index}
              data-testid="shot-block"
              data-failed={failed ? 'true' : 'false'}
              className={`group absolute break-words select-text cursor-text overflow-hidden rounded px-1 py-0.5 text-left leading-tight transition-[box-shadow,background-color] hover:overflow-visible hover:z-30 hover:!h-auto hover:shadow-lg hover:ring-1 hover:ring-indigo-400/50 ${
                failed
                  ? 'border border-rose-400 bg-rose-50/95 text-rose-700 dark:bg-rose-950/90 dark:text-rose-300'
                  : 'border border-slate-200/90 bg-white/95 text-slate-900 shadow-sm dark:border-slate-700/90 dark:bg-slate-800/95 dark:text-slate-100'
              }`}
              style={{
                ...toPercentStyle({
                  x: block.x ?? 0,
                  y: block.y ?? 0,
                  w: block.w ?? 0,
                  h: block.h ?? 0,
                }),
                minHeight: pct(block.h ?? 0),
                fontSize: `${fontSize}px`,
              }}
              onPointerDown={(e) => {
                if (e.button === 2) {
                  cancel()
                  return
                }
                e.stopPropagation()
              }}
              onDoubleClick={(e) => {
                e.stopPropagation()
                copyBlock(text, block.index)
              }}
              title={failed ? (r?.error_message ?? t.screenshot.translateFailed) : text}
            >
              {text}
              {copiedBlockIndex === block.index && (
                <span className="pointer-events-none absolute right-0.5 top-0.5 rounded bg-emerald-600 px-1 py-0.5 text-[9px] font-medium text-white shadow">
                  {t.translation.copied}
                </span>
              )}
              {!failed && (
                <button
                  type="button"
                  aria-label={t.translation.copy}
                  className="pointer-events-auto ml-1 inline-flex items-center opacity-0 group-hover:opacity-100 transition-opacity rounded bg-slate-200/80 hover:bg-slate-300 px-1 py-0.5 text-[9px] text-slate-700 dark:bg-slate-700 dark:text-slate-200 dark:hover:bg-slate-600"
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={(e) => {
                    e.stopPropagation()
                    copyBlock(text, block.index)
                  }}
                  title={t.translation.copy}
                >
                  {t.translation.copy}
                </button>
              )}
              {failed && (
                <button
                  type="button"
                  data-testid="shot-block-retry"
                  className="pointer-events-auto ml-1 rounded bg-rose-200 dark:bg-rose-900/60 px-1 text-[10px] font-medium text-rose-800 dark:text-rose-200 hover:bg-rose-300 dark:hover:bg-rose-800/60"
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={(e) => {
                    e.stopPropagation()
                    void translate(blocks, targetLang, engine, [block.index])
                  }}
                >
                  {t.task.btnRetry}
                </button>
              )}
            </div>
          )
        })}

      {selection && physicalWidth > 0 && (
        <div
          data-testid="shot-size"
          className="absolute rounded bg-slate-900/80 px-1.5 py-0.5 text-[11px] text-slate-100"
          style={{
            left: pct(selection.x),
            top: sizeHintBelowTop
              ? `calc(${pct(selection.y)} + 0.375rem)`
              : `calc(${pct(selection.y)} - 1.6rem)`,
          }}
        >
          {t.screenshot.sizeHint
            .replace('{w}', String(physicalWidth))
            .replace('{h}', String(physicalHeight))}
        </div>
      )}

      {/* 初始待选提示栏：尚未框选时居中停靠顶部 */}
      {blocks.length === 0 && !busy && !translating && (
        <div
          data-testid="shot-initial-bar"
          className="absolute left-1/2 top-3 flex -translate-x-1/2 items-center gap-3 rounded-lg border border-slate-200/90 bg-white/95 px-3.5 py-1.5 text-xs text-slate-800 shadow-xl backdrop-blur-sm dark:border-slate-700/90 dark:bg-slate-900/95 dark:text-slate-100"
          onPointerDown={(e) => e.stopPropagation()}
        >
          <span className="font-medium text-slate-700 dark:text-slate-200">
            {t.screenshot.title}
          </span>
          <button
            type="button"
            className="rounded bg-slate-100 px-2 py-0.5 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300"
            onClick={cancel}
          >
            {t.screenshot.cancel}
          </button>
        </div>
      )}

      {/* 选区智能跟随悬浮工具栏：对标有道翻译，紧贴框选区域右下/上方，消除高分大屏视线与光标长距离位移 */}
      {(blocks.length > 0 || busy || translating) && (
        <div
          data-testid="shot-toolbar"
          className="absolute z-40 flex items-center gap-2 rounded-lg border border-slate-200/90 bg-white/95 px-3 py-1.5 text-xs text-slate-800 shadow-xl backdrop-blur-sm transition-all duration-150 max-w-[calc(100vw-16px)] overflow-x-auto whitespace-nowrap dark:border-slate-700/90 dark:bg-slate-900/95 dark:text-slate-100"
          style={
            toolbarPos
              ? { left: `${toolbarPos.left}px`, top: `${toolbarPos.top}px` }
              : { left: '50%', top: '0.75rem', transform: 'translateX(-50%)' }
          }
          onPointerDown={(e) => e.stopPropagation()}
        >
          {busy || translating ? (
            <span
              role="status"
              aria-live="polite"
              className="flex items-center gap-1.5 font-medium text-indigo-600 dark:text-indigo-400"
            >
              <span className="inline-block h-2.5 w-2.5 animate-spin rounded-full border-2 border-indigo-600 border-t-transparent dark:border-indigo-400" />
              {busy ? t.screenshot.recognizing : t.screenshot.translating}
            </span>
          ) : (
            <>
              <select
                data-testid="shot-engine"
                aria-label={t.translation.engineSelect}
                value={engine}
                onChange={(e) => {
                  const next = e.target.value
                  setEngine(next)
                  // 只重发翻译：块文本已在内存，OCR 不必重跑。
                  void translate(blocks, targetLang, next)
                }}
                className="rounded border border-slate-200 bg-slate-50 px-1 py-0.5 text-[11px] text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200"
              >
                <option value="google">{t.translation.engineGoogle}</option>
                <option value="offline">{t.translation.engineOffline}</option>
                <option value="deepl">{t.translation.engineDeepl}</option>
                <option value="deepseek">{t.translation.engineDeepseek}</option>
                <option value="openai">{t.translation.engineOpenai}</option>
              </select>
              <select
                data-testid="shot-lang"
                aria-label={t.app.switchLanguage}
                value={targetLang}
                onChange={(e) => {
                  const next = e.target.value
                  setTargetLang(next)
                  void translate(blocks, next, engine)
                }}
                className="rounded border border-slate-200 bg-slate-50 px-1 py-0.5 text-[11px] text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200"
              >
                <option value="zh-CN">{t.languages['zh-CN']}</option>
                <option value="en">{t.languages.en}</option>
                <option value="ja">{t.languages.ja}</option>
                <option value="ko">{t.languages.ko}</option>
                <option value="fr">{t.languages.fr}</option>
                <option value="de">{t.languages.de}</option>
              </select>
              <button
                type="button"
                data-testid="shot-reselect"
                className="rounded bg-slate-100 px-2 py-0.5 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300"
                onClick={reselect}
                title={t.screenshot.reselect}
              >
                {t.screenshot.reselect}
              </button>
              <button
                type="button"
                data-testid="shot-copy"
                className="rounded bg-slate-100 px-2 py-0.5 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300"
                onClick={() => {
                  const text = getSelectedText()
                  navigator.clipboard
                    .writeText(text)
                    .then(() => {
                      setCopyState('ok')
                      setTimeout(() => setCopyState('idle'), 1500)
                    })
                    .catch(() => {
                      setCopyState('fail')
                      setTimeout(() => setCopyState('idle'), 1500)
                    })
                }}
              >
                {copyState === 'ok' ? t.translation.copied : t.translation.copy}
              </button>
              <button
                type="button"
                data-testid="shot-to-main"
                className="rounded bg-slate-100 px-2 py-0.5 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300"
                onClick={() => {
                  const text = getSelectedText()
                  commands.pushScreenshotTextToMain(text).catch(() => {})
                }}
              >
                {t.screenshot.sendToMain}
              </button>
              <button
                type="button"
                aria-label={
                  showTranslation ? t.screenshot.showOriginal : t.screenshot.showTranslation
                }
                className="flex items-center gap-1 rounded bg-indigo-50 px-2 py-0.5 font-medium text-indigo-700 hover:bg-indigo-100 dark:bg-indigo-950 dark:text-indigo-300"
                onClick={() => setShowTranslation((v) => !v)}
                title={t.screenshot.spaceTip}
              >
                <span>
                  {showTranslation ? t.screenshot.showOriginal : t.screenshot.showTranslation}
                </span>
                <kbd className="rounded bg-indigo-200/60 px-1 font-mono text-[9px] text-indigo-800 dark:bg-indigo-900/60 dark:text-indigo-200">
                  Space
                </kbd>
              </button>
            </>
          )}
          <button
            type="button"
            className="rounded bg-slate-100 px-2 py-0.5 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300"
            onClick={cancel}
          >
            {t.screenshot.cancel}
          </button>
        </div>
      )}

      {error && (
        <div
          data-testid="shot-error"
          role="alert"
          className="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center gap-2 rounded bg-rose-600/95 px-3 py-1.5 text-xs text-white shadow-lg"
          onPointerDown={(e) => e.stopPropagation()}
        >
          <span>{renderError(error)}</span>
          <button
            type="button"
            className="ml-1 text-white/80 hover:text-white"
            onClick={() => setError(null)}
          >
            ✕
          </button>
        </div>
      )}
    </div>
  )
}
