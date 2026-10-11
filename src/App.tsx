import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import {
  Languages,
  ArrowRight,
  Copy,
  Check,
  Key,
  Settings,
  X,
  Save,
  Globe,
  Image as ImageIcon,
  Volume2,
  Pin,
  PinOff,
} from 'lucide-react'
import { commands } from './bindings'
import type { EngineCapabilitiesDto, ProxyMode, ServerStatus } from './bindings'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getTranslation, Locale } from './i18n'
import type { I18nSchema } from './i18n'
import { useTaskRunner } from './task/useTaskRunner'
import { canRetry } from './task/taskState'
import HistoryPanel from './HistoryPanel'
import ShortcutRecorder from './ShortcutRecorder'
import { speakText } from './utils/speech'
import { downloadBlob } from './utils/download'
import { formatGlossaryCsv, parseGlossaryCsv } from './utils/csv'
import { useDialogFocus } from './utils/useDialogFocus'
import {
  parseLlmStylePreference,
  parseThemePreference,
  THEME_VALUES,
  type LlmStylePreference,
  type ThemePreference,
} from './utils/prefs'

/**
 * 配对会话有效期（秒）。
 * 权威值在后端 `server::pairing::PAIRING_SESSION_TTL_SECS`；当前 specta 绑定没有暴露
 * 「查询配对 TTL」命令，前端拿不到运行时的权威值，只能在这里同步维护一份。
 * 后端调整该常量时必须同步修改本值，否则倒计时会与服务端实际失效时间脱节。
 */
const PAIRING_TTL_SECONDS = 60

const SETTINGS_DIALOG_TITLE_ID = 'transly-settings-dialog-title'
const PAIRING_DIALOG_TITLE_ID = 'transly-pairing-dialog-title'
const OCR_DIALOG_TITLE_ID = 'transly-ocr-dialog-title'
const OCR_DIALOG_DESC_ID = 'transly-ocr-dialog-desc'

type CompareItem = { engine: string; text: string; loading: boolean; error: string | null }

/** 引擎清单：单引擎下拉与多引擎对比共用同一数据源，杜绝两处硬编码漂移。 */
const ENGINE_OPTIONS = [
  { id: 'google', labelKey: 'engineGoogle' },
  { id: 'deepl', labelKey: 'engineDeepl' },
  { id: 'deepseek', labelKey: 'engineDeepseek' },
  { id: 'openai', labelKey: 'engineOpenai' },
] as const satisfies ReadonlyArray<{ id: string; labelKey: keyof I18nSchema['translation'] }>

export default function App() {
  const [locale, setLocale] = useState<Locale>('zh-CN')
  const t = getTranslation(locale)

  const [sourceText, setSourceText] = useState('')
  const [targetLang, setTargetLang] = useState('zh-CN')
  const [engine, setEngine] = useState('google')
  const [copied, setCopied] = useState(false)
  const [serverToken, setServerToken] = useState<string>('')
  const [tokenCopied, setTokenCopied] = useState(false)
  const [rotatingToken, setRotatingToken] = useState(false)

  // 设置面板状态
  const [showSettings, setShowSettings] = useState(false)
  const [activeTab, setActiveTab] = useState<'translate' | 'history'>('translate')
  const [deeplConfigured, setDeeplConfigured] = useState(false)
  const [deepseekConfigured, setDeepseekConfigured] = useState(false)
  const [openaiConfigured, setOpenaiConfigured] = useState(false)
  const [deeplKey, setDeeplKey] = useState('')
  const [deepseekKey, setDeepseekKey] = useState('')
  const [openaiKey, setOpenaiKey] = useState('')
  const [deepseekBaseUrl, setDeepseekBaseUrl] = useState('')
  const [deepseekModel, setDeepseekModel] = useState('')
  const [openaiBaseUrl, setOpenaiBaseUrl] = useState('')
  const [openaiModel, setOpenaiModel] = useState('')
  const [clipboardMonitor, setClipboardMonitor] = useState(false)
  const [cacheEnabled, setCacheEnabled] = useState(true)
  const [glossarySource, setGlossarySource] = useState('')
  const [glossaryTarget, setGlossaryTarget] = useState('')
  const [glossaryMatchCase, setGlossaryMatchCase] = useState(true)
  const [glossaryList, setGlossaryList] = useState<
    Array<{ source: string; target: string; match_case: boolean }>
  >([])
  const [saveStatus, setSaveStatus] = useState<string | null>(null)
  const [capabilities, setCapabilities] = useState<EngineCapabilitiesDto | null>(null)
  const [ocrLoading, setOcrLoading] = useState(false)
  const [ocrError, setOcrError] = useState<string | null>(null)
  const [ocrConfirmOpen, setOcrConfirmOpen] = useState(false)
  const [serverStatus, setServerStatus] = useState<ServerStatus | null>(null)
  const [customShortcut, setCustomShortcut] = useState('Alt+Shift+D')
  // 快捷键注册失败原因（如被其他软件占用）。Alt+Shift+D/R 极易冲突，
  // 静默失败会让用户看到「划词莫名其妙不工作」且毫无线索。独立维护两者的冲突提示。
  const [shortcutConflict, setShortcutConflict] = useState<string>('')
  const [customReplaceShortcut, setCustomReplaceShortcut] = useState('Alt+Shift+R')
  const [replaceShortcutConflict, setReplaceShortcutConflict] = useState<string>('')
  const [preferredReplaceEngine, setPreferredReplaceEngine] = useState('google')
  const [proxyMode, setProxyMode] = useState<ProxyMode>('auto')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxySaving, setProxySaving] = useState(false)
  const [isFloatPinned, setIsFloatPinned] = useState(false)
  const [isCompareMode, setIsCompareMode] = useState(false)
  const [compareResults, setCompareResults] = useState<CompareItem[]>([])
  const compareGenRef = useRef(0)
  const compareDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // 瞬时错误提示：saveStatus 只在设置面板内渲染，主窗口的复制失败无处可显示
  const [toastError, setToastError] = useState<string | null>(null)
  const toastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const showToastError = (message: string) => {
    setToastError(message)
    if (toastTimerRef.current) clearTimeout(toastTimerRef.current)
    toastTimerRef.current = setTimeout(() => {
      toastTimerRef.current = null
      setToastError(null)
    }, 3000)
  }

  const triggerCompareTranslate = useCallback(
    (textToTranslate: string) => {
      const trimmed = textToTranslate.trim()
      if (!trimmed) {
        setCompareResults([])
        return
      }
      const activeGen = ++compareGenRef.current
      setCompareResults(
        ENGINE_OPTIONS.map(({ id }) => ({ engine: id, text: '', loading: true, error: null })),
      )
      ENGINE_OPTIONS.forEach(({ id: engName }) => {
        commands
          .translate(trimmed, 'auto', targetLang, engName)
          .then((res) => {
            if (compareGenRef.current !== activeGen) return
            if (res.status === 'ok') {
              setCompareResults((prev) =>
                prev.map((item) =>
                  item.engine === engName ? { ...item, text: res.data.text, loading: false } : item,
                ),
              )
            } else {
              setCompareResults((prev) =>
                prev.map((item) =>
                  item.engine === engName
                    ? {
                        ...item,
                        error: t.error[res.error.kind] || res.error.message,
                        loading: false,
                      }
                    : item,
                ),
              )
            }
          })
          .catch((err) => {
            if (compareGenRef.current !== activeGen) return
            setCompareResults((prev) =>
              prev.map((item) =>
                item.engine === engName ? { ...item, error: String(err), loading: false } : item,
              ),
            )
          })
      })
    },
    [targetLang, t.error],
  )

  /**
   * 多引擎对比的统一取消入口：作废在途代际 + 清掉待触发的防抖 + 复位 loading。
   * 任一入口漏掉其中一步，都可能让对比面板永久停在「翻译中」并让按钮被永久禁用。
   */
  const cancelCompare = useCallback(() => {
    if (compareDebounceRef.current) {
      clearTimeout(compareDebounceRef.current)
      compareDebounceRef.current = null
    }
    compareGenRef.current += 1
    setCompareResults([])
  }, [])

  const compareByEngine = useMemo(() => {
    const map: Record<string, CompareItem | undefined> = {}
    for (const item of compareResults) map[item.engine] = item
    return map
  }, [compareResults])

  // localStorage 属不可信输入：历史遗留或被改写的值会让主题/风格下拉与说明文案整块空白
  const [theme, setTheme] = useState<ThemePreference>(() =>
    parseThemePreference(localStorage.getItem('transly_theme')),
  )
  const [llmStyle, setLlmStyle] = useState<LlmStylePreference>(() =>
    parseLlmStylePreference(localStorage.getItem('transly_llm_style')),
  )

  // 同步 theme 类名到 document.documentElement
  useEffect(() => {
    const applyTheme = () => {
      const isDark =
        theme === 'dark' ||
        (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
      if (isDark) {
        document.documentElement.classList.add('dark')
      } else {
        document.documentElement.classList.remove('dark')
      }
    }

    applyTheme()
    localStorage.setItem('transly_theme', theme)

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handleChange = () => {
      if (theme === 'system') applyTheme()
    }
    mediaQuery.addEventListener('change', handleChange)
    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [theme])

  // 浏览器扩展一键配对请求浮层
  const [pairingRequest, setPairingRequest] = useState<{
    sessionId: string
    browserHint?: string
    countdown: number
  } | null>(null)
  const [pairingError, setPairingError] = useState<string | null>(null)
  const [pairingBusy, setPairingBusy] = useState(false)

  // 模态语义与焦点管理：打开聚焦容器、Tab 循环、Escape 关闭、关闭后焦点归位
  const closeSettings = useCallback(() => setShowSettings(false), [])
  const closePairing = useCallback(() => setPairingRequest(null), [])
  const closeOcrConfirm = useCallback(() => setOcrConfirmOpen(false), [])
  const settingsDialogRef = useDialogFocus(showSettings, closeSettings)
  const pairingDialogRef = useDialogFocus(pairingRequest !== null, closePairing)
  const ocrDialogRef = useDialogFocus(ocrConfirmOpen, closeOcrConfirm)

  const isFloatWindow = window.location.hash === '#float'

  // 四类入口（输入框 / 划词 / 剪贴板 / 浮窗）共用同一任务状态机
  const runner = useTaskRunner(engine, targetLang, { isPinned: isFloatPinned })
  const { state, text: translatedText, error, engine: usedEngine } = runner.snapshot
  const isComparing = isCompareMode && compareResults.some((r) => r.loading)
  const isTranslating = isCompareMode ? isComparing : state === 'running'

  // 默认 Google 引擎在部分网络（如中国大陆）不可达：
  // 命中网络类错误时必须给出可执行的替代方案，而不是只报「检查网络」
  const showNetworkEngineHint =
    error?.kind === 'network' && (engine === 'google' || engine === 'google_web')

  const loadGlossaryList = () => {
    commands
      .getGlossaryTerms()
      .then((res) => {
        if (res.status === 'ok') {
          setGlossaryList(res.data)
        } else {
          console.warn('[Transly] 读取术语库失败：', res.error)
        }
      })
      .catch((err) => {
        console.warn('[Transly] 读取术语库失败：', err)
      })
  }

  const checkConfiguredKeys = () => {
    commands
      .isApiKeyConfigured('deepl')
      .then(setDeeplConfigured)
      .catch((err) => console.warn('[Transly] 读取 DeepL 密钥状态失败：', err))
    commands
      .isApiKeyConfigured('deepseek')
      .then(setDeepseekConfigured)
      .catch((err) => console.warn('[Transly] 读取 DeepSeek 密钥状态失败：', err))
    commands
      .isApiKeyConfigured('openai')
      .then(setOpenaiConfigured)
      .catch((err) => console.warn('[Transly] 读取 OpenAI 密钥状态失败：', err))

    commands
      .getLlmConfig('deepseek')
      .then((res) => {
        if (res.status === 'ok') {
          setDeepseekBaseUrl(res.data.base_url || '')
          setDeepseekModel(res.data.model || '')
        }
      })
      .catch((err) => console.warn('[Transly] 读取 DeepSeek 配置失败：', err))

    commands
      .getLlmConfig('openai')
      .then((res) => {
        if (res.status === 'ok') {
          setOpenaiBaseUrl(res.data.base_url || '')
          setOpenaiModel(res.data.model || '')
        }
      })
      .catch((err) => console.warn('[Transly] 读取 OpenAI 配置失败：', err))
  }

  useEffect(() => {
    // 浮窗只渲染翻译卡片：下面约 12 次 IPC 拉回的状态它一个都不读，直接短路
    if (window.location.hash === '#float') return

    // 获取本地服务器 Token 供插件端授权配对
    commands
      .getServerToken()
      .then((res) => {
        if (res.status === 'ok') {
          setServerToken(res.data)
        }
      })
      .catch((err) => {
        console.warn('[Transly] 读取配对 Token 失败：', err)
      })

    checkConfiguredKeys()

    commands
      .getClipboardMonitor()
      .then((enabled) => {
        setClipboardMonitor(enabled)
      })
      .catch((err) => {
        console.warn('[Transly] 读取剪贴板监听状态失败：', err)
      })

    commands
      .getCacheEnabled()
      .then((enabled) => {
        setCacheEnabled(enabled)
      })
      .catch((err) => {
        console.warn('[Transly] 读取缓存开关失败：', err)
      })

    commands
      .getShortcut()
      .then((res) => {
        if (res.status === 'ok') {
          setCustomShortcut(res.data)
        }
      })
      .catch((err) => {
        console.warn('[Transly] 读取快捷键失败：', err)
      })

    commands
      .getReplaceShortcut()
      .then((res) => {
        if (res.status === 'ok') {
          setCustomReplaceShortcut(res.data)
        }
      })
      .catch((err) => {
        console.warn('[Transly] 读取划词替换快捷键失败：', err)
      })

    commands
      .getPreferredReplaceEngine()
      .then((res) => {
        if (res.status === 'ok') {
          setPreferredReplaceEngine(res.data)
        }
      })
      .catch((err) => {
        console.warn('[Transly] 读取划词替换首选引擎失败：', err)
      })

    commands
      .getProxyConfig()
      .then((res) => {
        if (res.status === 'ok') {
          setProxyMode(res.data.mode)
          setProxyUrl(res.data.url || '')
        }
      })
      .catch((err) => {
        console.warn('[Transly] 读取代理配置失败：', err)
      })

    // 启动时也查一次快捷键注册结果。
    //
    // Alt+Shift+D / Alt+Shift+R 极易被其它软件占用，而注册发生在应用启动阶段。
    // 若只在「保存快捷键」后查询，启动即失败的用户打开设置页看不到任何提示，
    // 只会觉得「划词莫名其妙不工作」——那正是要修的问题本身。
    commands
      .getShortcutStatus()
      .then((status) => {
        if (status.ok || !status.reason) {
          setShortcutConflict('')
          setReplaceShortcutConflict('')
          return
        }
        const parts = status.reason.split('; ')
        const normalConflict = parts.find((p) => p.includes('Alt+Shift+D'))
        const replaceConflict = parts.find((p) => p.includes('Alt+Shift+R'))
        setShortcutConflict(normalConflict || (!replaceConflict ? status.reason : ''))
        setReplaceShortcutConflict(replaceConflict || '')
      })
      .catch((err) => {
        console.warn('[Transly] 读取快捷键注册状态失败：', err)
      })

    commands
      .getLlmStyle()
      .then((res) => {
        if (res.status === 'ok') {
          // 后端返回的是未校验的裸字符串：非法值会让风格说明整块空白
          const val = parseLlmStylePreference(res.data)
          setLlmStyle(val)
          localStorage.setItem('transly_llm_style', val)
        }
      })
      .catch((err) => {
        console.warn('[Transly] 读取 LLM 风格失败：', err)
      })

    loadGlossaryList()
  }, [])

  useEffect(() => {
    // 监听本地浏览器插件配对请求（仅主窗口响应并展示，浮窗静默忽略）
    let unlisten: (() => void) | undefined
    let disposed = false
    listen<{ session_id: string; browser_hint?: string }>(
      'transly://pairing-requested',
      async (event) => {
        if (isFloatWindow) return
        setPairingError(null)
        setPairingBusy(false)
        setPairingRequest({
          sessionId: event.payload.session_id,
          browserHint: event.payload.browser_hint,
          countdown: PAIRING_TTL_SECONDS,
        })
        try {
          const win = getCurrentWindow()
          await win.show()
          await win.setFocus()
        } catch {
          // 在无窗口或测试环境下静默回落
        }
      },
    )
      .then((fn) => {
        // cleanup 先于 promise 兑现时 unlisten 仍是 undefined，监听器会永久泄漏；
        // 此处先判断是否已卸载，已卸载则立即注销。
        if (disposed) fn()
        else unlisten = fn
      })
      .catch((err) => {
        console.warn('[Transly] 配对请求事件订阅失败：', err)
      })

    return () => {
      disposed = true
      if (unlisten) unlisten()
    }
  }, [isFloatWindow])

  // 监听划词原地替换失败事件并弹出 Toast 与桌面原生通知提醒（仅限主窗口处理，防止双窗口重复弹窗）
  useEffect(() => {
    if (isFloatWindow) return
    let unlisten: (() => void) | undefined
    let disposed = false
    listen<{ engine: string; message: string }>('transly://replacement-failed', (event) => {
      showToastError(
        `${t.settings.replacementFailedPrefix} [${event.payload.engine}]: ${event.payload.message}`,
      )
      if (
        typeof window !== 'undefined' &&
        'Notification' in window &&
        Notification.permission === 'granted'
      ) {
        try {
          new Notification(t.settings.replacementFailedPrefix, {
            body: `[${event.payload.engine}]: ${event.payload.message}`,
          })
        } catch {
          // 降级使用内联 Toast 提示
        }
      }
    })
      .then((fn) => {
        if (disposed) fn()
        else unlisten = fn
      })
      .catch((err) => {
        console.warn('[Transly] 原地替换错误事件订阅失败：', err)
      })

    return () => {
      disposed = true
      if (unlisten) unlisten()
    }
  }, [t, isFloatWindow])

  // 启动阶段按需预申请系统通知权限（仅限主窗口，供外部窗口原地替换失败时原生弹出通知）
  useEffect(() => {
    if (isFloatWindow) return
    if (
      typeof window !== 'undefined' &&
      'Notification' in window &&
      Notification.permission === 'default'
    ) {
      Notification.requestPermission().catch(() => {})
    }
  }, [isFloatWindow])

  // 迷你悬浮窗 Escape 键极速关闭交互
  useEffect(() => {
    if (!isFloatWindow) return
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        getCurrentWindow()
          .hide()
          .catch((err) => console.warn('[Transly] 隐藏浮窗失败：', err))
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [isFloatWindow])

  // 依赖必须是稳定的 sessionId：依赖整个 pairingRequest 对象时，
  // 倒计时每秒重建一次对象，也会把 setInterval 每秒 clear 一次再重建。
  const pairingSessionId = pairingRequest?.sessionId ?? null
  const pairingCountdownRef = useRef(PAIRING_TTL_SECONDS)

  useEffect(() => {
    if (!pairingSessionId) return
    pairingCountdownRef.current = PAIRING_TTL_SECONDS
    const timer = setInterval(() => {
      const next = pairingCountdownRef.current - 1
      if (next <= 0) {
        pairingCountdownRef.current = 0
        setPairingRequest(null)
        return
      }
      pairingCountdownRef.current = next
      setPairingRequest((prev) => (prev ? { ...prev, countdown: next } : null))
    }, 1000)

    return () => clearInterval(timer)
  }, [pairingSessionId])

  const handleApprovePairing = async (sessionId: string) => {
    setPairingBusy(true)
    setPairingError(null)
    try {
      const res = await commands.approvePairing(sessionId)
      // approve_pairing 失败返回 {status:'error'} 而不是抛错：
      // 不判断就会在用户实际拒绝授权时照样弹窗消失，且零反馈。
      if (res.status === 'error') {
        setPairingError(res.error)
        return
      }
      setPairingRequest(null)
    } catch (err) {
      setPairingError(
        err instanceof Error && err.message ? err.message : t.app.pairingApproveFailed,
      )
    } finally {
      setPairingBusy(false)
    }
  }

  const handleRejectPairing = async (sessionId: string) => {
    setPairingBusy(true)
    setPairingError(null)
    try {
      const res = await commands.rejectPairing(sessionId)
      if (res.status === 'error') {
        setPairingError(res.error)
        return
      }
      setPairingRequest(null)
    } catch (err) {
      setPairingError(err instanceof Error && err.message ? err.message : t.app.pairingRejectFailed)
    } finally {
      setPairingBusy(false)
    }
  }

  useEffect(() => {
    // 浮窗不渲染服务状态，这段轮询对它纯属白跑
    if (window.location.hash === '#float') return

    // 本地服务在后台异步启动，失败只进 stderr 时用户完全无感
    // （扩展端会显示“桌面端未运行”，桌面端却毫无提示）。
    // 启动需要时间，故轮询数次直到脱离 starting 态。
    let cancelled = false
    let attempts = 0
    const poll = () => {
      commands
        .getServerStatus()
        .then((status) => {
          if (cancelled) return
          setServerStatus(status)
          attempts += 1
          if (status.state === 'starting' && attempts < 10) {
            setTimeout(poll, 500)
          }
        })
        .catch((err) => {
          console.warn('[Transly] 读取本地服务状态失败：', err)
        })
    }
    poll()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    // 引擎能力与配置检查：展示批量上限、降级标记与密钥配置状态
    let cancelled = false
    commands
      .getEngineCapabilities(engine)
      .then((res) => {
        if (!cancelled && res.status === 'ok') setCapabilities(res.data)
      })
      .catch((err) => {
        console.warn('[Transly] 读取引擎能力失败：', err)
      })
    return () => {
      cancelled = true
    }
  }, [engine])

  const handleToggleCacheEnabled = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.checked
    setCacheEnabled(val)
    try {
      const res = await commands.setCacheEnabled(val)
      if (res.status === 'error') {
        setCacheEnabled(!val)
        setSaveStatus(res.error)
        setTimeout(() => setSaveStatus(null), 2500)
      }
    } catch (err) {
      // typedError 遇到 Error 实例会直接 rethrow：缺 try/catch 时下面的回滚永不执行
      setCacheEnabled(!val)
      setSaveStatus(err instanceof Error && err.message ? err.message : t.settings.statusSaveFailed)
      setTimeout(() => setSaveStatus(null), 2500)
    }
  }

  const handleToggleClipboardMonitor = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.checked
    setClipboardMonitor(val)
    try {
      const res = await commands.setClipboardMonitor(val)
      if (res.status === 'error') {
        setClipboardMonitor(!val)
        setSaveStatus(res.error)
        setTimeout(() => setSaveStatus(null), 2500)
      }
    } catch (err) {
      setClipboardMonitor(!val)
      setSaveStatus(err instanceof Error && err.message ? err.message : t.settings.statusSaveFailed)
      setTimeout(() => setSaveStatus(null), 2500)
    }
  }

  const handleSaveShortcut = async () => {
    try {
      const res = await commands.setShortcut(customShortcut.trim())
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusShortcutSaved)
        setTimeout(() => setSaveStatus(null), 2000)
        // 保存成功不等于注册成功：组合键可能被其它软件占用。
        // 向后端确认真实注册结果，失败时把原因摆到界面上。
        const status = await commands.getShortcutStatus()
        if (status.ok || !status.reason) {
          setShortcutConflict('')
        } else {
          const parts = status.reason.split('; ')
          const target = parts.find((p) => p.includes(customShortcut.trim()))
          setShortcutConflict(target || status.reason)
        }
      } else {
        setSaveStatus(res.error)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusShortcutFailed,
      )
    }
  }

  const handleSaveReplaceShortcut = async () => {
    try {
      const res = await commands.setReplaceShortcut(customReplaceShortcut.trim())
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusShortcutSaved)
        setTimeout(() => setSaveStatus(null), 2000)
        const status = await commands.getShortcutStatus()
        if (status.ok || !status.reason) {
          setReplaceShortcutConflict('')
        } else {
          const parts = status.reason.split('; ')
          const target = parts.find((p) => p.includes(customReplaceShortcut.trim()))
          setReplaceShortcutConflict(target || status.reason)
        }
      } else {
        setSaveStatus(res.error)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusShortcutFailed,
      )
    }
  }

  const handleSavePreferredReplaceEngine = async (val: string) => {
    setPreferredReplaceEngine(val)
    try {
      const res = await commands.savePreferredReplaceEngine(val)
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusReplaceEngineSaved)
        setTimeout(() => setSaveStatus(null), 2000)
      } else {
        setSaveStatus(res.error)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusReplaceEngineFailed,
      )
    }
  }

  const handleSaveProxyConfig = async (newMode: ProxyMode, newUrl: string) => {
    setProxySaving(true)
    try {
      const res = await commands.saveProxyConfig({
        mode: newMode,
        url: newMode === 'manual' ? newUrl.trim() : null,
      })
      if (res.status === 'ok') {
        setProxyMode(newMode)
        setProxyUrl(newUrl)
        setSaveStatus(t.settings.statusProxySaved)
        setTimeout(() => setSaveStatus(null), 2500)
      } else {
        setSaveStatus(`${t.settings.statusProxySaveFailed}: ${res.error}`)
        setTimeout(() => setSaveStatus(null), 4000)
      }
    } catch (err) {
      setSaveStatus(
        `${t.settings.statusProxySaveFailed}: ${err instanceof Error ? err.message : String(err)}`,
      )
      setTimeout(() => setSaveStatus(null), 4000)
    } finally {
      setProxySaving(false)
    }
  }

  const handleRotateToken = async () => {
    // 无 in-flight 守卫时可连点：每次都真去动系统凭据库，后一次会把前一次的 Token 作废
    if (rotatingToken) return
    setRotatingToken(true)
    try {
      const res = await commands.rotateServerToken()
      if (res.status === 'ok') {
        setServerToken(res.data)
        setSaveStatus(t.settings.statusTokenRotated)
        setTimeout(() => setSaveStatus(null), 2500)
      } else {
        setSaveStatus(res.error)
      }
    } catch (err) {
      // 保留后端具体原因（如凭据管理器不可用），仅在其缺失时回落通用文案
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusTokenRotateFailed,
      )
    } finally {
      setRotatingToken(false)
    }
  }

  const handleCopyToken = () => {
    if (!serverToken) return
    navigator.clipboard
      .writeText(serverToken)
      .then(() => {
        setTokenCopied(true)
        setTimeout(() => setTokenCopied(false), 2000)
      })
      .catch((e: unknown) => {
        // 写入失败时绝不显示「已复制」
        showToastError(e instanceof Error && e.message ? e.message : t.translation.copyFailed)
      })
  }

  const handleSaveKeys = async () => {
    setSaveStatus(t.settings.statusSaving)
    try {
      if (deeplKey.trim()) {
        const res = await commands.saveApiKey('deepl', deeplKey.trim())
        if (res.status === 'error') throw new Error(res.error)
        setDeeplKey('')
      }
      if (deepseekKey.trim()) {
        const res = await commands.saveApiKey('deepseek', deepseekKey.trim())
        if (res.status === 'error') throw new Error(res.error)
        setDeepseekKey('')
      }
      if (openaiKey.trim()) {
        const res = await commands.saveApiKey('openai', openaiKey.trim())
        if (res.status === 'error') throw new Error(res.error)
        setOpenaiKey('')
      }
      // 保存 LLM 自定义端点与模型名称。
      // save_llm_config 失败返回 {status:'error'} 而不是抛错：
      // 不逐个判断就会把失败静默吞掉，最后还谎报「密钥已保存」。
      const deepseekRes = await commands.saveLlmConfig(
        'deepseek',
        deepseekBaseUrl.trim() || null,
        deepseekModel.trim() || null,
      )
      if (deepseekRes.status === 'error') throw new Error(deepseekRes.error)
      const openaiRes = await commands.saveLlmConfig(
        'openai',
        openaiBaseUrl.trim() || null,
        openaiModel.trim() || null,
      )
      if (openaiRes.status === 'error') throw new Error(openaiRes.error)
      checkConfiguredKeys()
      setSaveStatus(t.settings.statusKeysSaved)
      setTimeout(() => setSaveStatus(null), 2500)
    } catch (err) {
      // 透传后端校验原因（如「API 密钥长度超出限制」），避免退化为无法自查的通用文案
      setSaveStatus(err instanceof Error && err.message ? err.message : t.settings.statusSaveFailed)
    }
  }

  const handleDeleteKey = async (eng: string) => {
    try {
      const res = await commands.deleteApiKey(eng)
      if (res.status === 'error') throw new Error(res.error)
      checkConfiguredKeys()
      setSaveStatus(t.settings.statusKeyDeleted)
      setTimeout(() => setSaveStatus(null), 2000)
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusKeyDeleteFailed,
      )
    }
  }

  const handleAddGlossary = async () => {
    if (!glossarySource.trim() || !glossaryTarget.trim()) return
    try {
      const res = await commands.addGlossaryTerm(
        glossarySource.trim(),
        glossaryTarget.trim(),
        glossaryMatchCase,
      )
      if (res.status === 'error') throw new Error(res.error)
      setGlossarySource('')
      setGlossaryTarget('')
      setGlossaryMatchCase(true)
      loadGlossaryList()
      setSaveStatus(t.settings.statusGlossaryAdded)
      setTimeout(() => setSaveStatus(null), 2000)
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusGlossaryFailed,
      )
    }
  }

  const handleDeleteGlossary = async (source: string) => {
    try {
      const res = await commands.deleteGlossaryTerm(source)
      if (res.status === 'error') throw new Error(res.error)
      loadGlossaryList()
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusGlossaryFailed,
      )
    }
  }

  const handleClearSource = () => {
    setSourceText('')
    runner.reset()
    cancelCompare()
  }

  const handleCopy = () => {
    if (!translatedText) return
    navigator.clipboard
      .writeText(translatedText)
      .then(() => {
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
      })
      .catch((e: unknown) => {
        // 写入失败时绝不显示「已复制」
        showToastError(e instanceof Error && e.message ? e.message : t.translation.copyFailed)
      })
  }

  const handleSpeak = (text: string, lang: string) => {
    speakText(text, lang, runner.snapshot.detectedLang)
  }

  const runOcrClipboard = async () => {
    setOcrLoading(true)
    setOcrError(null)
    try {
      const res = await commands.ocrClipboardImage(targetLang)
      if (res.status === 'ok') {
        const text = res.data.trim()
        setSourceText(text)
        // OCR 文本来自剪贴板**图片**，并非剪贴板文本监听链路，
        // source 必须是 input，否则两条链路在任务状态里被混为一谈。
        // debounceMs: 0 立即投递统一任务状态机，秒级出译文
        runner.run(text, { source: 'input', debounceMs: 0 })
      } else {
        const errMap = t.ocrError
        setOcrError(errMap[res.error.kind] || res.error.message)
      }
    } catch (e) {
      setOcrError(String(e))
    } finally {
      setOcrLoading(false)
    }
  }

  const handleOcrClipboard = () => {
    // 已输入的文本没有撤销路径：写了半篇长文再误点一次就永久丢失，必须先确认
    if (sourceText.trim()) {
      setOcrConfirmOpen(true)
      return
    }
    void runOcrClipboard()
  }

  // 迷你悬浮小窗模式 (无边框、紧凑置顶卡片)
  if (isFloatWindow) {
    return (
      <div className="flex h-screen w-screen flex-col overflow-hidden rounded-xl border border-indigo-100 dark:border-slate-800 bg-white/95 dark:bg-slate-900/95 p-3.5 shadow-2xl backdrop-blur-md select-none text-slate-800 dark:text-slate-100">
        <div
          data-tauri-drag-region
          className="flex cursor-move items-center justify-between border-b border-slate-100 dark:border-slate-800 pb-2"
        >
          <div data-tauri-drag-region className="flex items-center gap-1.5">
            <Languages className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
            <span
              data-tauri-drag-region
              className="text-xs font-semibold text-slate-800 dark:text-slate-200"
            >
              {t.float.title}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                const next = !isFloatPinned
                setIsFloatPinned(next)
                commands.setFloatSticky(next).catch(() => {})
              }}
              className={`rounded p-1 transition-colors ${
                isFloatPinned
                  ? 'bg-indigo-100 dark:bg-indigo-950 text-indigo-700 dark:text-indigo-300 hover:bg-indigo-200 dark:hover:bg-indigo-900'
                  : 'text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-slate-600 dark:hover:text-slate-300'
              }`}
              title={isFloatPinned ? t.float.unpin : t.float.pin}
            >
              {isFloatPinned ? <PinOff className="h-3.5 w-3.5" /> : <Pin className="h-3.5 w-3.5" />}
            </button>
            {isTranslating && (
              <button
                onClick={runner.cancel}
                className="rounded bg-slate-100 dark:bg-slate-800 px-1.5 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700"
              >
                {t.task.btnCancel}
              </button>
            )}
            <select
              value={targetLang}
              onChange={(e) => setTargetLang(e.target.value)}
              className="rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-1.5 py-0.5 text-[11px] text-slate-700 dark:text-slate-200 outline-none"
            >
              <option value="zh-CN">{t.translation.languageZhCN}</option>
              <option value="en">{t.languages.en}</option>
              <option value="ja">{t.translation.languageJa}</option>
            </select>
          </div>
        </div>

        <div className="my-2 flex-1 overflow-auto rounded-lg bg-slate-50/70 dark:bg-slate-800/70 p-2 text-xs leading-relaxed text-slate-800 dark:text-slate-200 border border-slate-100 dark:border-slate-800">
          {isTranslating ? (
            <span className="text-slate-400 animate-pulse">{t.float.translating}</span>
          ) : error ? (
            <div className="flex flex-col gap-1">
              <span className="text-rose-500">{t.error[error.kind]}</span>
              {canRetry(state, error) && (
                <button
                  onClick={runner.retry}
                  className="self-start rounded bg-rose-50 px-2 py-0.5 text-[11px] font-medium text-rose-600 hover:bg-rose-100"
                >
                  {t.task.btnRetry}
                </button>
              )}
              {showNetworkEngineHint && (
                <span className="text-[10px] leading-snug text-amber-600">
                  {t.error.networkHintGoogle}
                </span>
              )}
            </div>
          ) : (
            translatedText || <span className="text-slate-400">{t.float.placeholder}</span>
          )}
        </div>

        <div className="flex items-center justify-between pt-1">
          <span className="text-[10px] text-slate-400 truncate max-w-[200px]">
            {state === 'cancelled'
              ? t.task.cancelled
              : runner.snapshot.sourceText
                ? `“${runner.snapshot.sourceText.slice(0, 30)}...”`
                : t.float.autoHideNotice}
          </span>
          {translatedText && (
            <div className="flex items-center gap-1.5">
              <button
                onClick={() => handleSpeak(translatedText, targetLang)}
                className="flex items-center gap-1 rounded bg-slate-100 px-2 py-0.5 text-[11px] font-medium text-slate-600 hover:bg-slate-200"
                title={t.translation.speak}
              >
                <Volume2 className="h-3 w-3" />
                <span>{t.translation.speak}</span>
              </button>
              <button
                onClick={handleCopy}
                className="flex items-center gap-1 rounded bg-indigo-50 px-2 py-0.5 text-[11px] font-medium text-indigo-600 hover:bg-indigo-100"
              >
                {copied ? (
                  <Check className="h-3 w-3 text-emerald-600" />
                ) : (
                  <Copy className="h-3 w-3" />
                )}
                <span>{copied ? t.translation.copied : t.translation.copy}</span>
              </button>
            </div>
          )}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-screen flex-col bg-slate-50 dark:bg-slate-950 font-sans text-slate-800 dark:text-slate-100">
      {/* 顶部标题栏 */}
      <header className="flex h-12 items-center justify-between border-b border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 px-4">
        <div className="flex items-center gap-2">
          <Languages className="h-5 w-5 text-indigo-600 dark:text-indigo-400" />
          <span className="font-semibold text-slate-800 dark:text-slate-100">Transly</span>
        </div>
        <div className="flex items-center gap-3 text-xs text-slate-500 dark:text-slate-400">
          <button
            onClick={() => setLocale(locale === 'zh-CN' ? 'en' : 'zh-CN')}
            className="flex items-center gap-1 rounded bg-slate-100 dark:bg-slate-800 px-2 py-1 text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700"
            title={t.app.switchLanguage}
          >
            <Globe className="h-3.5 w-3.5 text-indigo-500 dark:text-indigo-400" />
            <span>{locale === 'zh-CN' ? 'EN' : t.translation.languageZhCN}</span>
          </button>
          {serverToken && (
            <button
              onClick={handleCopyToken}
              title={t.app.copyToken}
              className="flex items-center gap-1 rounded bg-slate-100 dark:bg-slate-800 px-2 py-1 text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700"
            >
              <Key className="h-3.5 w-3.5 text-indigo-500 dark:text-indigo-400" />
              <span>{tokenCopied ? t.app.tokenCopied : t.app.copyToken}</span>
            </button>
          )}
          <button
            onClick={() => setShowSettings(true)}
            className="flex items-center gap-1 rounded bg-slate-100 dark:bg-slate-800 px-2 py-1 text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700"
            title={t.app.settings}
          >
            <Settings className="h-3.5 w-3.5 text-slate-500 dark:text-slate-400" />
            <span>{t.app.settings}</span>
          </button>
        </div>
      </header>

      {/* 翻译工作区 */}
      <main className="flex flex-1 flex-col p-4 gap-4">
        {/* 顶层 Tab：翻译工作区 / 翻译历史 */}
        <div className="flex items-center gap-1">
          <button
            onClick={() => setActiveTab('translate')}
            className={`rounded px-3 py-1 text-xs font-medium ${
              activeTab === 'translate'
                ? 'bg-indigo-600 text-white'
                : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
            }`}
          >
            {t.history.tabTranslate}
          </button>
          <button
            onClick={() => setActiveTab('history')}
            className={`rounded px-3 py-1 text-xs font-medium ${
              activeTab === 'history'
                ? 'bg-indigo-600 text-white'
                : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
            }`}
          >
            {t.history.tabHistory}
          </button>
        </div>

        {activeTab === 'translate' && (
          <>
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <span className="text-xs font-medium text-slate-500">
                  {runner.snapshot.detectedLang
                    ? `${t.translation.detectedLang}: ${runner.snapshot.detectedLang}`
                    : t.translation.autoDetect}
                </span>
                <ArrowRight className="h-4 w-4 text-slate-400" />
                <div className="flex items-center gap-1.5">
                  <select
                    value={targetLang}
                    onChange={(e) => setTargetLang(e.target.value)}
                    className="rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-xs text-slate-700 dark:text-slate-200 shadow-sm focus:border-indigo-500 focus:outline-none cursor-pointer"
                  >
                    <option value="zh-CN">{t.languages['zh-CN']}</option>
                    <option value="en">{t.languages.en}</option>
                    <option value="ja">{t.languages.ja}</option>
                    <option value="ko">{t.languages.ko}</option>
                    <option value="fr">{t.languages.fr}</option>
                    <option value="de">{t.languages.de}</option>
                  </select>
                  {runner.snapshot.effectiveTargetLang &&
                    runner.snapshot.effectiveTargetLang !== targetLang && (
                      <span className="rounded bg-indigo-50 dark:bg-indigo-950/60 px-1.5 py-0.5 text-[10px] font-medium text-indigo-600 dark:text-indigo-400 border border-indigo-200 dark:border-indigo-800">
                        {t.translation.smartRoutingTo}:{' '}
                        {t.languages[
                          runner.snapshot.effectiveTargetLang as keyof typeof t.languages
                        ] || runner.snapshot.effectiveTargetLang}
                      </span>
                    )}
                </div>
              </div>

              <div className="flex items-center gap-2">
                <div className="flex items-center rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 p-0.5 text-xs">
                  <button
                    onClick={() => {
                      setIsCompareMode(false)
                      // 必须一并作废对比代际：清空文本后切回单引擎、再切回对比，
                      // 会看到上一次遗留的 loading:true，翻译按钮被永久禁用
                      cancelCompare()
                      if (sourceText.trim()) {
                        runner.run(sourceText, { source: 'input', debounceMs: 0 })
                      }
                    }}
                    className={`rounded px-2 py-0.5 ${!isCompareMode ? 'bg-indigo-600 text-white font-medium' : 'text-slate-600 dark:text-slate-300'}`}
                  >
                    {t.translation.modeSingle}
                  </button>
                  <button
                    onClick={() => {
                      setIsCompareMode(true)
                      runner.cancel()
                      if (sourceText.trim()) {
                        triggerCompareTranslate(sourceText)
                      } else {
                        cancelCompare()
                      }
                    }}
                    className={`rounded px-2 py-0.5 ${isCompareMode ? 'bg-indigo-600 text-white font-medium' : 'text-slate-600 dark:text-slate-300'}`}
                  >
                    {t.translation.modeCompare}
                  </button>
                </div>
                {!isCompareMode && (
                  <>
                    <span className="text-xs text-slate-500 dark:text-slate-400">
                      {t.translation.engineSelect}:
                    </span>
                    <select
                      value={engine}
                      onChange={(e) => setEngine(e.target.value)}
                      className="rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-xs text-slate-700 dark:text-slate-200 shadow-sm focus:border-indigo-500 focus:outline-none cursor-pointer"
                    >
                      {ENGINE_OPTIONS.map(({ id, labelKey }) => (
                        <option key={id} value={id}>
                          {t.translation[labelKey]}
                        </option>
                      ))}
                    </select>
                  </>
                )}
              </div>

              {capabilities && (
                <div className="text-[10px] text-slate-400">
                  {capabilities.name}
                  {capabilities.supports_batch &&
                    ` · ${t.translation.maxBatch}${capabilities.max_batch_size}`}
                  {capabilities.requires_api_key && ` · ${t.translation.apiKeyRequired}`}
                </div>
              )}
            </div>

            <div className="grid flex-1 grid-cols-2 gap-4">
              <div className="flex flex-col rounded-lg border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-3 shadow-sm focus-within:border-indigo-500 dark:focus-within:border-indigo-400">
                <textarea
                  value={sourceText}
                  onChange={(e) => {
                    const v = e.target.value
                    setSourceText(v)
                    if (isCompareMode) {
                      // 键盘清空必须与「清空内容」按钮走同一条路径，
                      // 否则残留的 loading:true 会让翻译按钮永久禁用
                      if (!v.trim()) {
                        cancelCompare()
                        return
                      }
                      if (compareDebounceRef.current) clearTimeout(compareDebounceRef.current)
                      compareDebounceRef.current = setTimeout(() => {
                        triggerCompareTranslate(v)
                      }, 500)
                    } else {
                      // 即输即译：500ms 防抖，新输入立即作废上一代在途结果
                      runner.run(v, { source: 'input', debounceMs: 500 })
                    }
                  }}
                  placeholder={t.translation.inputPlaceholder}
                  className="flex-1 resize-none bg-transparent text-sm text-slate-800 dark:text-slate-100 outline-none placeholder:text-slate-400 dark:placeholder:text-slate-500"
                />
                {error && (
                  <div className="mt-1 flex flex-col gap-1 text-xs">
                    <div className="flex items-start gap-2">
                      <span className="text-rose-500">{t.error[error.kind]}</span>
                      {canRetry(state, error) && (
                        <button
                          onClick={runner.retry}
                          className="rounded border border-rose-200 bg-rose-50 px-1.5 py-0.5 font-medium text-rose-600 hover:bg-rose-100"
                        >
                          {t.task.btnRetry}
                        </button>
                      )}
                    </div>
                    {showNetworkEngineHint && (
                      <span className="text-amber-600">{t.error.networkHintGoogle}</span>
                    )}
                  </div>
                )}
                {ocrError && (
                  <div className="mt-1 flex items-center justify-between rounded bg-rose-50 px-2 py-1 text-xs text-rose-600 border border-rose-200">
                    <span>{ocrError}</span>
                    <button
                      onClick={() => setOcrError(null)}
                      className="text-slate-400 hover:text-slate-600 ml-2"
                      title={t.translation.btnClose}
                    >
                      ✕
                    </button>
                  </div>
                )}
                <div className="mt-2 flex items-center justify-between gap-2">
                  <div className="flex items-center gap-1.5">
                    <button
                      type="button"
                      onClick={handleOcrClipboard}
                      disabled={ocrLoading || isTranslating}
                      className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2.5 py-1.5 text-xs text-slate-600 shadow-sm hover:bg-slate-100 disabled:opacity-50"
                      title={t.translation.btnOcrPaste}
                    >
                      <ImageIcon className="h-3.5 w-3.5 text-indigo-500" />
                      <span>
                        {ocrLoading ? t.translation.ocrRunning : t.translation.btnOcrPaste}
                      </span>
                    </button>
                    {sourceText.trim() && (
                      <button
                        type="button"
                        onClick={handleClearSource}
                        className="rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-1.5 text-xs text-slate-500 dark:text-slate-400 shadow-sm hover:bg-slate-100 dark:hover:bg-slate-700"
                        title={t.translation.clearText}
                      >
                        <X className="h-3.5 w-3.5" />
                      </button>
                    )}
                    {sourceText.length > 0 && (
                      <span className="text-[10px] text-slate-400">
                        {t.translation.charCount.replace('{n}', String(sourceText.length))}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-2">
                    {isTranslating && (
                      <button
                        onClick={() => {
                          if (isCompareMode) {
                            compareGenRef.current += 1
                            setCompareResults((prev) =>
                              prev.map((item) =>
                                item.loading ? { ...item, loading: false } : item,
                              ),
                            )
                          } else {
                            runner.cancel()
                          }
                        }}
                        className="rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-1 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700"
                      >
                        {t.task.btnCancel}
                      </button>
                    )}
                    <button
                      onClick={() => {
                        if (isCompareMode) {
                          triggerCompareTranslate(sourceText)
                        } else {
                          runner.run(sourceText, { source: 'input' })
                        }
                      }}
                      disabled={isTranslating || !sourceText.trim()}
                      className="rounded bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 disabled:opacity-50"
                    >
                      {isTranslating ? t.translation.translating : t.translation.btnTranslate}
                    </button>
                  </div>
                </div>
              </div>

              {!isCompareMode ? (
                <div className="flex flex-col rounded-lg border border-slate-200 bg-white dark:bg-slate-900 p-3 shadow-sm">
                  <div className="flex-1 overflow-auto text-sm text-slate-800 dark:text-slate-100 whitespace-pre-wrap">
                    {translatedText ? (
                      <span>
                        {translatedText}
                        {isTranslating && (
                          <span className="inline-block w-1.5 h-3.5 ml-0.5 bg-indigo-500 animate-pulse align-middle" />
                        )}
                      </span>
                    ) : isTranslating ? (
                      <span className="text-slate-400 flex items-center gap-1.5">
                        <span className="inline-block w-1.5 h-3.5 bg-indigo-500 animate-pulse" />
                        {t.translation.translating}
                      </span>
                    ) : (
                      <span className="text-slate-400">{t.translation.outputPlaceholder}</span>
                    )}
                  </div>
                  <div className="mt-2 flex items-center justify-between">
                    {usedEngine && (
                      <span className="text-[10px] text-slate-400">engine: {usedEngine}</span>
                    )}
                    {translatedText && (
                      <div className="flex items-center gap-1.5 justify-end">
                        <button
                          onClick={() => handleSpeak(translatedText, targetLang)}
                          className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2 py-1 text-xs text-slate-600 hover:bg-slate-100 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                          title={t.translation.speak}
                        >
                          <Volume2 className="h-3.5 w-3.5" />
                          <span>{t.translation.speak}</span>
                        </button>
                        <button
                          onClick={handleCopy}
                          className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-100 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                        >
                          {copied ? (
                            <Check className="h-3.5 w-3.5 text-emerald-600" />
                          ) : (
                            <Copy className="h-3.5 w-3.5" />
                          )}
                          <span>{copied ? t.translation.copied : t.translation.copy}</span>
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              ) : (
                <div className="flex flex-col gap-2 overflow-y-auto">
                  {ENGINE_OPTIONS.map(({ id: engName }) => {
                    const item = compareByEngine[engName]
                    return (
                      <div
                        key={engName}
                        className="flex flex-col rounded-lg border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-2.5 shadow-sm"
                      >
                        <div className="flex items-center justify-between border-b border-slate-100 dark:border-slate-800 pb-1 mb-1.5">
                          <span className="text-[11px] font-semibold uppercase text-indigo-600 dark:text-indigo-400">
                            {engName}
                          </span>
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => {
                                if (item?.text) handleSpeak(item.text, targetLang)
                              }}
                              className="p-1 text-slate-400 hover:text-slate-600"
                              title={t.translation.speak}
                            >
                              <Volume2 className="h-3 w-3" />
                            </button>
                            <button
                              onClick={() => {
                                if (!item?.text) return
                                navigator.clipboard.writeText(item.text).catch((e: unknown) => {
                                  showToastError(
                                    e instanceof Error && e.message
                                      ? e.message
                                      : t.translation.copyFailed,
                                  )
                                })
                              }}
                              className="p-1 text-slate-400 hover:text-slate-600"
                              title={t.translation.copy}
                            >
                              <Copy className="h-3 w-3" />
                            </button>
                          </div>
                        </div>
                        <div className="text-xs text-slate-700 dark:text-slate-200 min-h-[40px]">
                          {item?.loading ? (
                            <span className="text-slate-400 animate-pulse">
                              {t.translation.translating}
                            </span>
                          ) : item?.error ? (
                            <span className="text-rose-500">{item.error}</span>
                          ) : (
                            item?.text || (
                              <span className="text-slate-400">
                                {t.translation.outputPlaceholder}
                              </span>
                            )
                          )}
                        </div>
                      </div>
                    )
                  })}
                </div>
              )}
            </div>
          </>
        )}

        {activeTab === 'history' && <HistoryPanel t={t} locale={locale} />}
      </main>
      {/* 设置与密钥配置模态框 */}
      {showSettings && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 backdrop-blur-sm p-4">
          <div
            ref={settingsDialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={SETTINGS_DIALOG_TITLE_ID}
            tabIndex={-1}
            className="flex max-h-[85vh] w-[480px] flex-col rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-5 shadow-xl"
          >
            <div className="flex items-center justify-between border-b border-slate-100 dark:border-slate-800 pb-3 shrink-0">
              <div className="flex items-center gap-2">
                <Settings className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
                <h3
                  id={SETTINGS_DIALOG_TITLE_ID}
                  className="font-semibold text-slate-800 dark:text-slate-100 text-sm"
                >
                  {t.settings.title}
                </h3>
              </div>
              <button
                onClick={() => setShowSettings(false)}
                aria-label={t.translation.btnClose}
                className="rounded p-1 text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-slate-600 dark:hover:text-slate-300"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="mt-4 flex flex-1 flex-col gap-4 overflow-y-auto pr-1 text-xs">
              {/* 外观主题设置 */}
              <div className="flex items-center justify-between rounded-lg bg-slate-50 dark:bg-slate-800/80 p-3 border border-slate-100 dark:border-slate-800">
                <span className="font-medium text-slate-700 dark:text-slate-200">
                  {t.settings.themeTitle}
                </span>
                <div className="flex items-center gap-1">
                  {THEME_VALUES.map((mode) => (
                    <button
                      key={mode}
                      onClick={() => setTheme(mode)}
                      className={`rounded px-2.5 py-1 text-xs font-medium transition-colors ${
                        theme === mode
                          ? 'bg-indigo-600 text-white'
                          : 'bg-white dark:bg-slate-700 text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-600 border border-slate-200 dark:border-slate-600'
                      }`}
                    >
                      {mode === 'light'
                        ? t.settings.themeLight
                        : mode === 'dark'
                          ? t.settings.themeDark
                          : t.settings.themeSystem}
                    </button>
                  ))}
                </div>
              </div>

              {/* API 密钥管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/80 p-3 border border-slate-100 dark:border-slate-800">
                <span className="font-medium text-slate-700">{t.settings.credentialsTitle}</span>
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <label className="text-slate-500">{t.settings.deeplKeyLabel}</label>
                    {deeplConfigured && (
                      <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
                        {t.settings.configuredBadge}
                        <button
                          onClick={() => handleDeleteKey('deepl')}
                          className="text-slate-400 hover:text-rose-600 ml-1 underline"
                        >
                          {t.settings.btnDeleteKey}
                        </button>
                      </span>
                    )}
                  </div>
                  <input
                    type="password"
                    value={deeplKey}
                    onChange={(e) => setDeeplKey(e.target.value)}
                    placeholder={
                      deeplConfigured
                        ? t.settings.keyPlaceholderOverwrite
                        : t.settings.keyPlaceholderDeepl
                    }
                    className="w-full rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                </div>
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <label className="text-slate-500">{t.settings.deepseekKeyLabel}</label>
                    {deepseekConfigured && (
                      <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
                        {t.settings.configuredBadge}
                        <button
                          onClick={() => handleDeleteKey('deepseek')}
                          className="text-slate-400 hover:text-rose-600 ml-1 underline"
                        >
                          {t.settings.btnDeleteKey}
                        </button>
                      </span>
                    )}
                  </div>
                  <input
                    type="password"
                    value={deepseekKey}
                    onChange={(e) => setDeepseekKey(e.target.value)}
                    placeholder={
                      deepseekConfigured
                        ? t.settings.keyPlaceholderOverwrite
                        : t.settings.keyPlaceholderDeepseek
                    }
                    className="w-full rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                  <div className="mt-1.5 text-[11px] text-slate-400">
                    {t.settings.deepseekCustomTitle}
                  </div>
                  <div className="mt-1 grid grid-cols-2 gap-2">
                    <input
                      type="text"
                      value={deepseekBaseUrl}
                      onChange={(e) => setDeepseekBaseUrl(e.target.value)}
                      placeholder={t.settings.deepseekBaseUrlPlaceholder}
                      className="rounded border border-slate-200 bg-white px-2 py-1 text-[11px] text-slate-700 outline-none focus:border-indigo-500"
                    />
                    <input
                      type="text"
                      value={deepseekModel}
                      onChange={(e) => setDeepseekModel(e.target.value)}
                      placeholder={t.settings.deepseekModelPlaceholder}
                      className="rounded border border-slate-200 bg-white px-2 py-1 text-[11px] text-slate-700 outline-none focus:border-indigo-500"
                    />
                  </div>
                </div>
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <label className="text-slate-500">{t.settings.openaiKeyLabel}</label>
                    {openaiConfigured && (
                      <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
                        {t.settings.configuredBadge}
                        <button
                          onClick={() => handleDeleteKey('openai')}
                          className="text-slate-400 hover:text-rose-600 ml-1 underline"
                        >
                          {t.settings.btnDeleteKey}
                        </button>
                      </span>
                    )}
                  </div>
                  <input
                    type="password"
                    value={openaiKey}
                    onChange={(e) => setOpenaiKey(e.target.value)}
                    placeholder={
                      openaiConfigured
                        ? t.settings.keyPlaceholderOverwrite
                        : t.settings.keyPlaceholderOpenai
                    }
                    className="w-full rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                  <div className="mt-1.5 text-[11px] text-slate-400">
                    {t.settings.openaiCustomTitle}
                  </div>
                  <div className="mt-1 grid grid-cols-2 gap-2">
                    <input
                      type="text"
                      value={openaiBaseUrl}
                      onChange={(e) => setOpenaiBaseUrl(e.target.value)}
                      placeholder={t.settings.openaiBaseUrlPlaceholder}
                      className="rounded border border-slate-200 bg-white px-2 py-1 text-[11px] text-slate-700 outline-none focus:border-indigo-500"
                    />
                    <input
                      type="text"
                      value={openaiModel}
                      onChange={(e) => setOpenaiModel(e.target.value)}
                      placeholder={t.settings.openaiModelPlaceholder}
                      className="rounded border border-slate-200 bg-white px-2 py-1 text-[11px] text-slate-700 outline-none focus:border-indigo-500"
                    />
                  </div>
                  <div className="mt-1 flex items-center justify-between">
                    <button
                      type="button"
                      onClick={() => {
                        setOpenaiBaseUrl('http://localhost:11434/v1')
                        setOpenaiModel('qwen2.5:7b')
                        if (!openaiKey) setOpenaiKey('ollama')
                      }}
                      className="text-[10px] text-indigo-600 dark:text-indigo-400 hover:underline"
                    >
                      💡 {t.settings.ollamaPresetBtn}
                    </button>
                  </div>
                </div>
                <div className="flex justify-end pt-1">
                  <button
                    onClick={handleSaveKeys}
                    className="flex items-center gap-1 rounded bg-indigo-600 px-3 py-1.5 font-medium text-white shadow-sm hover:bg-indigo-700"
                  >
                    <Save className="h-3.5 w-3.5" />
                    <span>{t.settings.btnSaveKeys}</span>
                  </button>
                </div>
              </div>

              {/* 全局划词快捷键设置 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div>
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.shortcutTitle}
                  </div>
                  <div className="text-[11px] text-slate-400">{t.settings.shortcutDesc}</div>
                </div>
                <ShortcutRecorder
                  value={customShortcut}
                  onChange={setCustomShortcut}
                  onSave={handleSaveShortcut}
                  t={t}
                />
                {shortcutConflict && (
                  <div role="alert" className="text-[11px] text-amber-700 dark:text-amber-400">
                    {t.settings.shortcutRegisterFailed}: {shortcutConflict}
                  </div>
                )}
              </div>

              {/* 划词原地替换快捷键设置 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div>
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.replaceShortcutTitle}
                  </div>
                  <div className="text-[11px] text-slate-400">{t.settings.replaceShortcutDesc}</div>
                </div>
                <ShortcutRecorder
                  value={customReplaceShortcut}
                  onChange={setCustomReplaceShortcut}
                  onSave={handleSaveReplaceShortcut}
                  t={t}
                />
                {replaceShortcutConflict && (
                  <div role="alert" className="text-[11px] text-amber-700 dark:text-amber-400">
                    {t.settings.shortcutRegisterFailed}: {replaceShortcutConflict}
                  </div>
                )}
              </div>

              {/* 划词原地替换首选引擎设置 */}
              <div className="flex items-center justify-between rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div>
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.preferredReplaceEngine}
                  </div>
                  <div className="text-[11px] text-slate-400">
                    {t.settings.preferredReplaceEngineDesc}
                  </div>
                </div>
                <select
                  value={preferredReplaceEngine}
                  onChange={(e) => handleSavePreferredReplaceEngine(e.target.value)}
                  className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-xs text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500 cursor-pointer"
                >
                  <option value="google">{t.translation.engineGoogle}</option>
                  <option value="deepl">{t.translation.engineDeepl}</option>
                  <option value="deepseek">{t.translation.engineDeepseek}</option>
                  <option value="openai">{t.translation.engineOpenai}</option>
                </select>
              </div>

              {/* LLM 风格偏好设置 */}
              <div className="flex items-center justify-between rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div>
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.llmStyleTitle}
                  </div>
                  <div className="text-[11px] text-slate-400">
                    {llmStyle === 'general' && t.settings.llmStyleGeneral}
                    {llmStyle === 'academic' && t.settings.llmStyleAcademic}
                    {llmStyle === 'literary' && t.settings.llmStyleLiterary}
                    {llmStyle === 'business' && t.settings.llmStyleBusiness}
                  </div>
                </div>
                <select
                  value={llmStyle}
                  onChange={(e) => {
                    const val = parseLlmStylePreference(e.target.value)
                    setLlmStyle(val)
                    localStorage.setItem('transly_llm_style', val)
                    commands.saveLlmStyle(val).catch((err) => {
                      console.warn('[Transly] 保存 LLM 风格到后端失败:', err)
                    })
                  }}
                  className="rounded border border-slate-200 bg-white dark:bg-slate-800 px-2.5 py-1 text-xs text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500 cursor-pointer"
                >
                  <option value="general">{t.settings.llmStyleGeneral}</option>
                  <option value="academic">{t.settings.llmStyleAcademic}</option>
                  <option value="literary">{t.settings.llmStyleLiterary}</option>
                  <option value="business">{t.settings.llmStyleBusiness}</option>
                </select>
              </div>

              {/* 网络代理设置 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-medium text-slate-700 dark:text-slate-200">
                      {t.settings.proxyTitle}
                    </div>
                    <div className="text-[11px] text-slate-400">{t.settings.proxyDesc}</div>
                  </div>
                  <select
                    value={proxyMode}
                    onChange={(e) => {
                      const newMode = e.target.value as ProxyMode
                      setProxyMode(newMode)
                      if (newMode !== 'manual') {
                        handleSaveProxyConfig(newMode, proxyUrl)
                      }
                    }}
                    className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-xs text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500 cursor-pointer"
                  >
                    <option value="auto">{t.settings.proxyModeAuto}</option>
                    <option value="system">{t.settings.proxyModeSystem}</option>
                    <option value="manual">{t.settings.proxyModeManual}</option>
                    <option value="none">{t.settings.proxyModeNone}</option>
                  </select>
                </div>
                {proxyMode === 'manual' && (
                  <div className="mt-1 flex items-center gap-2">
                    <input
                      type="text"
                      value={proxyUrl}
                      onChange={(e) => setProxyUrl(e.target.value)}
                      placeholder={t.settings.proxyUrlPlaceholder}
                      className="flex-1 rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1 text-xs text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500 font-mono"
                    />
                    <button
                      onClick={() => handleSaveProxyConfig('manual', proxyUrl)}
                      disabled={proxySaving || !proxyUrl.trim()}
                      className="rounded bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-700 disabled:opacity-50 transition-colors"
                    >
                      {proxySaving ? '...' : t.settings.btnSaveProxy}
                    </button>
                  </div>
                )}
              </div>

              {/* 剪贴板监听开关 */}
              <div className="flex items-center justify-between rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div>
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.clipboardMonitorLabel}
                  </div>
                  <div className="text-[11px] text-slate-400">
                    {t.settings.clipboardMonitorDesc}
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={clipboardMonitor}
                  onChange={handleToggleClipboardMonitor}
                  className="h-4 w-4 rounded text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                />
              </div>

              {/* 插件通信 Token 轮换管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-medium text-slate-700 dark:text-slate-200">
                      {t.settings.tokenRotateTitle}
                    </div>
                    <div className="text-[11px] text-slate-400">{t.settings.tokenRotateDesc}</div>
                  </div>
                  <button
                    onClick={handleRotateToken}
                    disabled={rotatingToken}
                    className="rounded border border-indigo-200 dark:border-indigo-800 bg-indigo-50 dark:bg-indigo-950/60 px-2.5 py-1 text-[11px] font-medium text-indigo-600 dark:text-indigo-400 hover:bg-indigo-100 dark:hover:bg-indigo-900/60 disabled:opacity-50"
                  >
                    {t.settings.btnRotateToken}
                  </button>
                </div>
                {/* 本地扩展服务状态：启动失败（如端口被占）此前只进 stderr，
                    用户只能看到扩展端“桌面端未运行”而无法自查 */}
                <div className="flex items-center justify-between border-t border-slate-200 dark:border-slate-700 pt-2">
                  <div className="text-[11px] text-slate-500 dark:text-slate-400">
                    {t.settings.serverStatusTitle}
                  </div>
                  <span
                    className={
                      serverStatus?.state === 'running'
                        ? 'text-[11px] font-medium text-emerald-600 dark:text-emerald-400'
                        : serverStatus?.state === 'failed'
                          ? 'text-[11px] font-medium text-rose-600 dark:text-rose-400'
                          : 'text-[11px] text-slate-400'
                    }
                  >
                    {serverStatus?.state === 'running'
                      ? t.settings.serverStatusRunning
                      : serverStatus?.state === 'failed'
                        ? t.settings.serverStatusFailed
                        : t.settings.serverStatusStarting}
                  </span>
                </div>
                {serverStatus?.state === 'failed' && (
                  <div className="rounded bg-rose-50 dark:bg-rose-950/50 border border-rose-200 dark:border-rose-900/60 px-2 py-1 text-[11px] leading-snug text-rose-600 dark:text-rose-400">
                    {serverStatus.reason}
                  </div>
                )}
              </div>

              {/* 缓存数据管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-medium text-slate-700 dark:text-slate-200">
                      {t.settings.cachePersistTitle}
                    </div>
                    <div className="text-[11px] text-slate-400">{t.settings.cachePersistDesc}</div>
                  </div>
                  <input
                    type="checkbox"
                    checked={cacheEnabled}
                    onChange={handleToggleCacheEnabled}
                    className="h-4 w-4 rounded text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                  />
                </div>
                <div className="flex items-center justify-between border-t border-slate-200 dark:border-slate-700 pt-2 mt-1">
                  <div>
                    <div className="font-medium text-slate-700 dark:text-slate-200">
                      {t.settings.cacheClearTitle}
                    </div>
                    <div className="text-[11px] text-slate-400">{t.settings.cacheClearDesc}</div>
                  </div>
                  <button
                    onClick={async () => {
                      // 与设置面板其他操作一致：失败必须可见，不得静默无反馈
                      try {
                        const res = await commands.clearCache()
                        if (res.status === 'ok') {
                          setSaveStatus(
                            t.settings.statusCacheCleared.replace('{n}', String(res.data)),
                          )
                        } else {
                          setSaveStatus(res.error)
                        }
                      } catch (err) {
                        setSaveStatus(
                          err instanceof Error && err.message
                            ? err.message
                            : t.settings.statusCacheClearFailed,
                        )
                      }
                      setTimeout(() => setSaveStatus(null), 2000)
                    }}
                    className="rounded border border-rose-200 dark:border-rose-900/60 bg-rose-50 dark:bg-rose-950/50 px-2.5 py-1 text-[11px] font-medium text-rose-600 dark:text-rose-400 hover:bg-rose-100 dark:hover:bg-rose-900/50"
                  >
                    {t.settings.btnClearCache}
                  </button>
                </div>
              </div>

              {/* 术语库管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div className="flex items-center justify-between">
                  <span className="font-medium text-slate-700 dark:text-slate-200">
                    {t.settings.glossaryTitle}
                  </span>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => {
                        if (glossaryList.length === 0) return
                        // 与导入共用同一套 RFC 4180 转义 / 解析，含逗号的术语才不会被拆坏
                        const csv = formatGlossaryCsv(
                          glossaryList.map((g) => ({
                            source: g.source,
                            target: g.target,
                            matchCase: g.match_case,
                          })),
                        )
                        const blob = new Blob([csv], { type: 'text/csv' })
                        downloadBlob(
                          blob,
                          `transly-glossary-${new Date().toISOString().slice(0, 10)}.csv`,
                        )
                      }}
                      className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700"
                      title={t.translation.exportGlossaryCsv}
                    >
                      {t.translation.exportGlossaryCsv}
                    </button>
                    <label className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 cursor-pointer">
                      <span>{t.translation.importGlossaryCsv}</span>
                      <input
                        type="file"
                        accept=".csv"
                        className="hidden"
                        onChange={(e) => {
                          const file = e.target.files?.[0]
                          // 清空 value，否则连续两次选择同一个文件不会触发 change
                          e.target.value = ''
                          if (!file) return
                          const reader = new FileReader()
                          reader.onload = async (evt) => {
                            const text =
                              typeof evt.target?.result === 'string' ? evt.target.result : ''
                            if (!text) return
                            let imported = 0
                            let failed = 0
                            for (const term of parseGlossaryCsv(text)) {
                              try {
                                const res = await commands.addGlossaryTerm(
                                  term.source,
                                  term.target,
                                  term.matchCase,
                                )
                                // add_glossary_term 失败不抛错：不判 status 就会静默吞掉
                                if (res.status === 'error') {
                                  failed += 1
                                  console.warn('[Transly] 导入术语失败:', res.error)
                                } else {
                                  imported += 1
                                }
                              } catch (err) {
                                failed += 1
                                console.warn('[Transly] 导入术语失败:', err)
                              }
                            }
                            loadGlossaryList()
                            setSaveStatus(
                              failed > 0
                                ? t.settings.glossaryImportPartial
                                    .replace('{n}', String(imported))
                                    .replace('{m}', String(failed))
                                : t.settings.glossaryImportDone.replace('{n}', String(imported)),
                            )
                            setTimeout(() => setSaveStatus(null), 3000)
                          }
                          reader.onerror = () => {
                            setSaveStatus(t.settings.statusGlossaryFailed)
                            setTimeout(() => setSaveStatus(null), 3000)
                          }
                          reader.readAsText(file)
                        }}
                      />
                    </label>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  <input
                    type="text"
                    value={glossarySource}
                    onChange={(e) => setGlossarySource(e.target.value)}
                    placeholder={t.settings.glossarySourcePlaceholder}
                    className="rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                  <input
                    type="text"
                    value={glossaryTarget}
                    onChange={(e) => setGlossaryTarget(e.target.value)}
                    placeholder={t.settings.glossaryTargetPlaceholder}
                    className="rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                </div>
                <div className="flex items-center justify-between pt-1">
                  <label className="flex items-center gap-1.5 text-xs text-slate-600 cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={glossaryMatchCase}
                      onChange={(e) => setGlossaryMatchCase(e.target.checked)}
                      className="h-3.5 w-3.5 rounded text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                    />
                    <span>{t.settings.glossaryMatchCaseLabel}</span>
                  </label>
                  <button
                    onClick={handleAddGlossary}
                    disabled={!glossarySource.trim() || !glossaryTarget.trim()}
                    className="rounded bg-slate-800 px-3 py-1.5 font-medium text-white shadow-sm hover:bg-slate-900 disabled:opacity-40"
                  >
                    {t.settings.btnAddGlossary}
                  </button>
                </div>

                {/* 已保存术语列表 */}
                {glossaryList.length > 0 && (
                  <div className="mt-2 max-h-32 overflow-y-auto border-t border-slate-200 pt-2 flex flex-col gap-1.5">
                    {glossaryList.map((item) => (
                      <div
                        key={item.source}
                        className="flex items-center justify-between rounded bg-white px-2 py-1 border border-slate-200"
                      >
                        <div className="flex items-center gap-1.5">
                          <span className="font-semibold text-slate-700">{item.source}</span>
                          <span className="text-slate-400">→</span>
                          <span className="text-slate-600">{item.target}</span>
                          {!item.match_case && (
                            <span className="rounded bg-amber-50 px-1 py-0.2 text-[10px] text-amber-600 border border-amber-200">
                              Aa
                            </span>
                          )}
                        </div>
                        <button
                          onClick={() => handleDeleteGlossary(item.source)}
                          className="rounded p-0.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600"
                          title={t.settings.btnDeleteGlossary}
                        >
                          <X className="h-3 w-3" />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {saveStatus && (
                <div className="rounded bg-emerald-50 border border-emerald-200 p-2 text-center text-emerald-700 font-medium">
                  {saveStatus}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* 浏览器扩展配对确认浮层 */}
      {pairingRequest && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 backdrop-blur-xs p-4 animate-in fade-in duration-200">
          <div
            ref={pairingDialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={PAIRING_DIALOG_TITLE_ID}
            tabIndex={-1}
            className="w-full max-w-sm rounded-xl bg-white p-5 shadow-2xl border border-indigo-100 flex flex-col gap-4"
          >
            <div className="flex items-start gap-3">
              <div className="rounded-full bg-indigo-50 p-2 text-indigo-600">
                <Key className="h-6 w-6" />
              </div>
              <div className="flex-1">
                <h3 id={PAIRING_DIALOG_TITLE_ID} className="font-semibold text-slate-800 text-sm">
                  {t.app.pairingModalTitle}
                </h3>
                <p className="text-xs text-slate-500 mt-1 leading-relaxed">
                  {t.app.pairingModalDesc}
                </p>
                {pairingRequest.browserHint && (
                  <p className="text-[11px] text-slate-400 mt-1">
                    {t.app.pairingBrowserLabel}
                    <span className="font-medium text-slate-600">{pairingRequest.browserHint}</span>
                  </p>
                )}
              </div>
            </div>

            {pairingError && (
              <div
                role="alert"
                className="rounded bg-rose-50 border border-rose-200 px-2 py-1 text-[11px] leading-snug text-rose-600"
              >
                {pairingError}
              </div>
            )}

            <div className="flex items-center justify-end gap-2 pt-2 border-t border-slate-100">
              <button
                onClick={() => handleRejectPairing(pairingRequest.sessionId)}
                disabled={pairingBusy}
                className="rounded-lg px-3 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-100 transition-colors disabled:opacity-50"
              >
                {t.app.pairingReject}
              </button>
              <button
                onClick={() => handleApprovePairing(pairingRequest.sessionId)}
                disabled={pairingBusy}
                className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 transition-colors flex items-center gap-1.5 disabled:opacity-50"
              >
                <span>{t.app.pairingAllow}</span>
                <span className="text-[10px] bg-indigo-700/60 rounded px-1">
                  {pairingRequest.countdown}
                  {t.app.pairingExpiresIn}
                </span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* OCR 覆盖确认：识别结果会不可撤销地替换已输入的文本 */}
      {ocrConfirmOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 backdrop-blur-sm p-4">
          <div
            ref={ocrDialogRef}
            role="alertdialog"
            aria-modal="true"
            aria-labelledby={OCR_DIALOG_TITLE_ID}
            aria-describedby={OCR_DIALOG_DESC_ID}
            tabIndex={-1}
            className="flex w-full max-w-sm flex-col gap-4 rounded-xl border border-slate-200 bg-white p-5 shadow-xl"
          >
            <h3 id={OCR_DIALOG_TITLE_ID} className="text-sm font-semibold text-slate-800">
              {t.translation.ocrOverwriteTitle}
            </h3>
            <p id={OCR_DIALOG_DESC_ID} className="text-xs leading-relaxed text-slate-500">
              {t.translation.ocrOverwriteDesc.replace('{n}', String(sourceText.length))}
            </p>
            <div className="flex items-center justify-end gap-2 border-t border-slate-100 pt-3">
              <button
                onClick={closeOcrConfirm}
                className="rounded-lg px-3 py-1.5 text-xs font-medium text-slate-600 transition-colors hover:bg-slate-100"
              >
                {t.task.btnCancel}
              </button>
              <button
                onClick={() => {
                  setOcrConfirmOpen(false)
                  void runOcrClipboard()
                }}
                className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white shadow-sm transition-colors hover:bg-indigo-700"
              >
                {t.translation.ocrOverwriteConfirm}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 瞬时错误提示（剪贴板写入失败等）：saveStatus 只在设置面板内可见 */}
      {toastError && (
        <div
          role="alert"
          className="fixed bottom-4 left-1/2 z-[60] -translate-x-1/2 rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-600 shadow-lg"
        >
          {toastError}
        </div>
      )}
    </div>
  )
}
