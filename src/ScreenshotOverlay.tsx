import { useEffect, useRef, useState } from 'react'
import { commands, type OcrBlockDto, type OcrErrorDto, type ScreenshotSessionDto } from './bindings'
import { getTranslation, Locale } from './i18n'
import { parseLocalePreference, parseThemePreference, LOCALE_STORAGE_KEY } from './utils/prefs'

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

  const [session, setSession] = useState<ScreenshotSessionDto | null>(null)
  const [error, setError] = useState<OcrErrorDto | string | null>(null)
  const [anchor, setAnchor] = useState<Point | null>(null)
  const [pointer, setPointer] = useState<Point | null>(null)
  const [blocks, setBlocks] = useState<OcrBlockDto[]>([])
  const [busy, setBusy] = useState(false)
  const [showTranslation, setShowTranslation] = useState(true)
  const [closed, setClosed] = useState(false)
  const dragging = useRef(false)

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
        if (res.status === 'ok') setSession(res.data)
        else setError(res.error)
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

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      setClosed(true)
      commands.cancelScreenshotTranslation().catch(() => {})
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  /** 指针位置 → 归一化坐标。钳制在信任边界上做一次，Rust 侧还会再钳一次。 */
  const toNormalized = (e: React.PointerEvent): Point => ({
    x: clamp01(e.clientX / window.innerWidth),
    y: clamp01(e.clientY / window.innerHeight),
  })

  const onPointerDown = (e: React.PointerEvent) => {
    if (busy || e.button !== 0) return
    const point = toNormalized(e)
    dragging.current = true
    // 新的框选开始即清掉上一轮的错误与译文块：否则旧译文会浮在屏幕上，
    // 旧错误横幅也会在后续成功识别后依然挂在那里。
    setError(null)
    setBlocks([])
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
    try {
      const res = await commands.recognizeScreenshotSelection(rect, session.generation)
      if (res.status === 'error') setError(res.error)
      else setBlocks(res.data)
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  const selection: Selection | null =
    anchor && pointer
      ? {
          x: Math.min(anchor.x, pointer.x),
          y: Math.min(anchor.y, pointer.y),
          w: Math.abs(pointer.x - anchor.x),
          h: Math.abs(pointer.y - anchor.y),
        }
      : null

  if (closed) return null

  const physicalWidth = selection ? Math.round(selection.w * (session?.image_width ?? 0)) : 0
  const physicalHeight = selection ? Math.round(selection.h * (session?.image_height ?? 0)) : 0
  // 从屏幕最顶端开始框选时，提示条不能挂到视口外——外层 `overflow-hidden` 会把它整条裁掉。
  const sizeHintBelowTop = selection != null && selection.y * window.innerHeight < 32

  return (
    <div
      data-testid="shot-surface"
      className="fixed inset-0 cursor-crosshair touch-none select-none overflow-hidden bg-slate-950"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        dragging.current = false
      }}
    >
      {session && (
        <img
          data-testid="shot-frame"
          src={session.image_url}
          alt=""
          draggable={false}
          className="absolute inset-0 h-full w-full object-fill"
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
        blocks.map((block) => (
          <div
            key={block.index}
            data-testid="shot-block"
            className="absolute overflow-hidden rounded bg-white/90 px-0.5 text-left text-[11px] leading-tight text-slate-900 dark:bg-slate-800/90 dark:text-slate-100"
            style={{
              left: pct(block.x ?? 0),
              top: pct(block.y ?? 0),
              width: block.w == null ? undefined : pct(block.w),
              height: block.h == null ? undefined : pct(block.h),
            }}
          >
            {block.text}
          </div>
        ))}

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

      <div
        className="absolute left-1/2 top-3 flex -translate-x-1/2 items-center gap-2 rounded-lg bg-white/95 px-3 py-1.5 text-xs text-slate-800 shadow-lg dark:bg-slate-900/95 dark:text-slate-100"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <span className="font-medium">{t.screenshot.title}</span>
        {busy && (
          <span className="text-indigo-600 dark:text-indigo-400">{t.screenshot.recognizing}</span>
        )}
        {blocks.length > 0 && (
          <button
            type="button"
            className="rounded bg-indigo-50 px-2 py-0.5 font-medium text-indigo-700 hover:bg-indigo-100 dark:bg-indigo-950 dark:text-indigo-300"
            onClick={() => setShowTranslation((v) => !v)}
          >
            {showTranslation ? t.screenshot.showOriginal : t.screenshot.showTranslation}
          </button>
        )}
        <button
          type="button"
          className="rounded bg-slate-100 px-2 py-0.5 text-slate-600 hover:bg-slate-200 dark:bg-slate-800 dark:text-slate-300"
          onClick={cancel}
        >
          {t.screenshot.cancel}
        </button>
      </div>

      {error && (
        <div
          data-testid="shot-error"
          className="absolute bottom-4 left-1/2 -translate-x-1/2 rounded bg-rose-600/95 px-3 py-1.5 text-xs text-white shadow-lg"
        >
          {formatOcrError(error)}
        </div>
      )}
    </div>
  )
}
