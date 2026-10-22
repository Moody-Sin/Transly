import { useState, useEffect, useRef, useCallback } from 'react'
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
  Camera,
  Volume2,
} from 'lucide-react'
import { commands } from './bindings'
import type { EngineCapabilitiesDto, ProxyMode, ServerStatus } from './bindings'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getTranslation, Locale } from './i18n'
import type { I18nSchema } from './i18n'
import { useTaskRunner } from './task/useTaskRunner'
import { useTransientMessage } from './utils/useTransientMessage'
import { canRetry } from './task/taskState'
import HistoryPanel from './HistoryPanel'
import ShortcutRecorder from './ShortcutRecorder'
import { speakText } from './utils/speech'
import { downloadBlob } from './utils/download'
import { formatGlossaryCsv, parseGlossaryCsv } from './utils/csv'
import { useDialogFocus } from './utils/useDialogFocus'
import {
  LOCALE_STORAGE_KEY,
  parseLlmStylePreference,
  parseLocalePreference,
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

/** 引擎清单 */
const ENGINE_OPTIONS = [
  { id: 'google', labelKey: 'engineGoogle' },
  { id: 'offline', labelKey: 'engineOffline' },
  { id: 'deepl', labelKey: 'engineDeepl' },
  { id: 'deepseek', labelKey: 'engineDeepseek' },
  { id: 'openai', labelKey: 'engineOpenai' },
] as const satisfies ReadonlyArray<{ id: string; labelKey: keyof I18nSchema['translation'] }>

export default function App() {
  // localStorage 属不可信输入：历史遗留或被改写的值一律回退默认语言
  const [locale, setLocale] = useState<Locale>(() =>
    parseLocalePreference(localStorage.getItem(LOCALE_STORAGE_KEY)),
  )
  const t = getTranslation(locale)

  // 持久化并广播语言偏好：悬浮窗是独立窗口，靠 storage 事件实现同步
  useEffect(() => {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale)
  }, [locale])

  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key !== LOCALE_STORAGE_KEY) return
      setLocale(parseLocalePreference(e.newValue))
    }
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [])

  const [sourceText, setSourceText] = useState('')
  const [targetLang, setTargetLang] = useState(
    () => localStorage.getItem('transly_main_target_lang') || 'zh-CN',
  )
  const [engine, setEngine] = useState(
    () => localStorage.getItem('transly_main_engine') || 'google',
  )
  const [copied, setCopied] = useState(false)
  const [serverToken, setServerToken] = useState<string>('')
  const [tokenCopied, setTokenCopied] = useState(false)
  const [rotatingToken, setRotatingToken] = useState(false)

  // 持久化主窗口引擎与目标语言偏好，并同步后端默认配置
  useEffect(() => {
    localStorage.setItem('transly_main_target_lang', targetLang)
    commands.savePreferredTargetLang(targetLang).catch(() => {})
  }, [targetLang])

  useEffect(() => {
    localStorage.setItem('transly_main_engine', engine)
  }, [engine])

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
  const [glossarySearch, setGlossarySearch] = useState('')
  const [glossarySource, setGlossarySource] = useState('')
  const [glossaryTarget, setGlossaryTarget] = useState('')
  const [glossaryMatchCase, setGlossaryMatchCase] = useState(true)
  const [glossaryList, setGlossaryList] = useState<
    Array<{ source: string; target: string; match_case: boolean }>
  >([])
  const saveStatusState = useTransientMessage<string | null>(null)
  const saveStatus = saveStatusState.value
  const saveStatusIsError = saveStatusState.error
  // clearAfterMs 取代了此前 18 处裸 `setTimeout(() => setSaveStatus(null), N)`：
  // 那些定时器从不清理，连续操作时前一次会抢先把后一次提示抹掉，卸载后还会再 setState。
  const setSaveStatus = (msg: string | null, isError = false, clearAfterMs?: number) => {
    saveStatusState.set(msg, { error: isError, clearAfterMs })
  }
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
  const [customScreenshotShortcut, setCustomScreenshotShortcut] = useState('Alt+Shift+S')
  const [screenshotShortcutConflict, setScreenshotShortcutConflict] = useState<string>('')
  const [screenshotCapability, setScreenshotCapability] = useState<{
    supported: boolean
    reason: string | null
  } | null>(null)
  const [preferredReplaceEngine, setPreferredReplaceEngine] = useState('google')
  const [proxyMode, setProxyMode] = useState<ProxyMode>('auto')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxySaving, setProxySaving] = useState(false)

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

  // 卸载时清理 Toast 定时器，避免组件销毁后仍触发 setState
  useEffect(
    () => () => {
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current)
    },
    [],
  )

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

  // 四类入口（输入框 / 划词 / 剪贴板 / 浮窗）共用同一任务状态机
  const runner = useTaskRunner(engine, targetLang)
  // runner 是每次渲染新建的对象字面量，不能进 effect 依赖：放进去会让事件订阅
  // 随每个流式 chunk（每个 chunk 都 setSnapshot → 重渲染）反复 unlisten/re-listen。
  // 用 ref 拿「最新的 run」，订阅本身只建立一次。
  const runnerRunRef = useRef(runner.run)
  runnerRunRef.current = runner.run
  const { state, text: translatedText, error, engine: usedEngine } = runner.snapshot
  const isTranslating = state === 'running'

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
        setShortcutConflict(status.translate_error || '')
        setReplaceShortcutConflict(status.replace_error || '')
        setScreenshotShortcutConflict(status.screenshot_error || '')
      })
      .catch((err) => {
        console.warn('[Transly] 读取快捷键注册状态失败：', err)
      })

    // 探测截图能力：Wayland / 缺 tesseract 等环境下按钮应禁用并说明原因，
    // 而不是让用户按了才发现不能用。
    commands
      .getScreenshotCapability()
      .then((cap) =>
        setScreenshotCapability({ supported: cap.supported, reason: cap.reason ?? null }),
      )
      .catch((err) => {
        console.warn('[Transly] 读取截图能力失败：', err)
      })

    commands
      .getScreenshotShortcut()
      .then((res) => {
        if (res.status === 'ok') setCustomScreenshotShortcut(res.data)
      })
      .catch((err) => {
        console.warn('[Transly] 读取截图快捷键失败：', err)
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
    listen<{ session_id: string; browser_hint?: string; expires_in?: number }>(
      'transly://pairing-requested',
      async (event) => {
        setPairingError(null)
        setPairingBusy(false)
        const expiresIn = event.payload.expires_in ?? PAIRING_TTL_SECONDS
        pairingInitialCountdownRef.current = expiresIn
        setPairingRequest({
          sessionId: event.payload.session_id,
          browserHint: event.payload.browser_hint,
          countdown: expiresIn,
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
  }, [])

  // 监听截图覆盖窗的「发送到主窗口」：把文本灌进输入框并立即翻译。
  //
  // 这是「OCR 结果可编辑」被砍掉后的替代路径——用户在覆盖窗里看到不满意的
  // 识别/翻译结果时，送到主窗口精修再重译。
  useEffect(() => {
    let unlisten: (() => void) | undefined
    let disposed = false
    listen<{ text: string }>('transly://screenshot-to-main', (event) => {
      const text = event.payload.text ?? ''
      if (!text.trim()) return
      setSourceText(text)
      runnerRunRef.current(text, { source: 'input', debounceMs: 0 })
    })
      .then((fn) => {
        if (disposed) fn()
        else unlisten = fn
      })
      .catch((err) => {
        console.warn('[Transly] 截图送主窗口事件订阅失败：', err)
      })
    return () => {
      disposed = true
      if (unlisten) unlisten()
    }
  }, [])

  // 监听划词原地替换失败事件并弹出 Toast 与桌面原生通知提醒（仅限主窗口处理，防止双窗口重复弹窗）
  useEffect(() => {
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
  }, [t])

  // 启动阶段按需预申请系统通知权限（仅限主窗口，供外部窗口原地替换失败时原生弹出通知）
  useEffect(() => {
    if (
      typeof window !== 'undefined' &&
      'Notification' in window &&
      Notification.permission === 'default'
    ) {
      Notification.requestPermission().catch(() => {})
    }
  }, [])

  // 依赖必须是稳定的 sessionId：依赖整个 pairingRequest 对象时，
  // 倒计时每秒重建一次对象，也会把 setInterval 每秒 clear 一次再重建。
  const pairingSessionId = pairingRequest?.sessionId ?? null
  const pairingCountdownRef = useRef(PAIRING_TTL_SECONDS)
  const pairingInitialCountdownRef = useRef(PAIRING_TTL_SECONDS)

  useEffect(() => {
    if (!pairingSessionId) return
    pairingCountdownRef.current = pairingInitialCountdownRef.current
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
    let timer: ReturnType<typeof setTimeout> | null = null
    const poll = () => {
      commands
        .getServerStatus()
        .then((status) => {
          if (cancelled) return
          setServerStatus(status)
          attempts += 1
          if (status.state === 'starting' && attempts < 10) {
            timer = setTimeout(poll, 500)
          }
        })
        .catch((err) => {
          console.warn('[Transly] 读取本地服务状态失败：', err)
        })
    }
    poll()
    return () => {
      cancelled = true
      // 卸载时清掉待触发的下一次轮询，避免残留定时器持续发起 IPC
      if (timer) clearTimeout(timer)
    }
  }, [])

  useEffect(() => {
    // 引擎能力与配置检查：展示批量上限与密钥配置状态
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
        setSaveStatus(res.error, true, 2500)
      }
    } catch (err) {
      // typedError 遇到 Error 实例会直接 rethrow：缺 try/catch 时下面的回滚永不执行
      setCacheEnabled(!val)
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusSaveFailed,
        true,
        2500,
      )
    }
  }

  const handleToggleClipboardMonitor = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.checked
    setClipboardMonitor(val)
    try {
      const res = await commands.setClipboardMonitor(val)
      if (res.status === 'error') {
        setClipboardMonitor(!val)
        setSaveStatus(res.error, true, 2500)
      }
    } catch (err) {
      setClipboardMonitor(!val)
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusSaveFailed,
        true,
        2500,
      )
    }
  }

  const handleSaveShortcut = async () => {
    try {
      const res = await commands.setShortcut(customShortcut.trim())
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusShortcutSaved, false, 2000)
        // 保存成功不等于注册成功：组合键可能被其它软件占用。
        // 向后端确认真实注册结果，失败时把原因摆到界面上。
        const status = await commands.getShortcutStatus()
        setShortcutConflict(status.translate_error || '')
      } else {
        setSaveStatus(res.error, true)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusShortcutFailed,
        true,
      )
    }
  }

  const handleSaveReplaceShortcut = async () => {
    try {
      const res = await commands.setReplaceShortcut(customReplaceShortcut.trim())
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusShortcutSaved, false, 2000)
        const status = await commands.getShortcutStatus()
        setReplaceShortcutConflict(status.replace_error || '')
      } else {
        setSaveStatus(res.error, true)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusShortcutFailed,
        true,
      )
    }
  }

  const handleSaveScreenshotShortcut = async () => {
    try {
      const res = await commands.setScreenshotShortcut(customScreenshotShortcut.trim())
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusShortcutSaved, false, 2000)
        const status = await commands.getShortcutStatus()
        setScreenshotShortcutConflict(status.screenshot_error || '')
      } else {
        setSaveStatus(res.error, true)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusShortcutFailed,
        true,
      )
    }
  }

  const handleSavePreferredReplaceEngine = async (val: string) => {
    setPreferredReplaceEngine(val)
    try {
      const res = await commands.savePreferredReplaceEngine(val)
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusReplaceEngineSaved, false, 2000)
      } else {
        setSaveStatus(res.error, true)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusReplaceEngineFailed,
        true,
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
        setSaveStatus(t.settings.statusProxySaved, false, 2500)
      } else {
        setSaveStatus(`${t.settings.statusProxySaveFailed}: ${res.error}`, true, 4000)
      }
    } catch (err) {
      setSaveStatus(
        `${t.settings.statusProxySaveFailed}: ${err instanceof Error ? err.message : String(err)}`,
        true,
        4000,
      )
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
        setSaveStatus(t.settings.statusTokenRotated, false, 2500)
      } else {
        setSaveStatus(res.error, true)
      }
    } catch (err) {
      // 保留后端具体原因（如凭据管理器不可用），仅在其缺失时回落通用文案
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusTokenRotateFailed,
        true,
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
      setSaveStatus(t.settings.statusKeysSaved, false, 2500)
    } catch (err) {
      // 透传后端校验原因（如「API 密钥长度超出限制」），避免退化为无法自查的通用文案
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusSaveFailed,
        true,
      )
    }
  }

  const handleDeleteKey = async (eng: string) => {
    try {
      const res = await commands.deleteApiKey(eng)
      if (res.status === 'error') throw new Error(res.error)
      checkConfiguredKeys()
      setSaveStatus(t.settings.statusKeyDeleted, false, 2000)
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusKeyDeleteFailed,
        true,
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
      setSaveStatus(t.settings.statusGlossaryAdded, false, 2000)
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusGlossaryFailed,
        true,
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
        true,
      )
    }
  }

  const handleClearSource = () => {
    setSourceText('')
    runner.reset()
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

  /** 触发截图翻译（主窗口按钮入口）。失败时用 t.ocrError 映射文案，不透传硬编码中文。 */
  const handleScreenshot = async () => {
    setOcrError(null)
    try {
      const res = await commands.startScreenshotTranslation()
      if (res.status === 'error') {
        const errMap = t.ocrError
        setOcrError(errMap[res.error.kind] || res.error.message)
      }
    } catch (e) {
      setOcrError(e instanceof Error && e.message ? e.message : String(e))
    }
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
                    onChange={(e) => {
                      const newLang = e.target.value
                      setTargetLang(newLang)
                      // 已有文本时立即用新语言重跑：setState 是异步的，
                      // 必须把新值随调用同步传入并锁死（forceTargetLang），
                      // 否则会沿用旧语言，或让「中译中」润色被智能对转改写成英文。
                      if (sourceText.trim()) {
                        runner.run(sourceText, {
                          source: 'input',
                          targetLang: newLang,
                          forceTargetLang: true,
                        })
                      }
                    }}
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
                <span className="text-xs text-slate-500 dark:text-slate-400">
                  {t.translation.engineSelect}:
                </span>
                <select
                  value={engine}
                  onChange={(e) => {
                    const newEng = e.target.value
                    setEngine(newEng)
                    // 已有文本时立即用新引擎重跑（setState 异步，须同步传入新值）
                    if (sourceText.trim()) {
                      runner.run(sourceText, {
                        source: 'input',
                        engine: newEng,
                        forceTargetLang: true,
                      })
                    }
                  }}
                  className="rounded border border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-xs text-slate-700 dark:text-slate-200 shadow-sm focus:border-indigo-500 focus:outline-none cursor-pointer"
                >
                  {ENGINE_OPTIONS.map(({ id, labelKey }) => (
                    <option key={id} value={id}>
                      {t.translation[labelKey]}
                    </option>
                  ))}
                </select>
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
                    // 即输即译：500ms 防抖，新输入立即作废上一代在途结果
                    runner.run(v, { source: 'input', debounceMs: 500 })
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
                {screenshotCapability && !screenshotCapability.supported && (
                  <div className="mt-1 rounded bg-amber-50 px-2 py-1 text-xs text-amber-700 border border-amber-200 dark:bg-amber-950/40 dark:text-amber-400 dark:border-amber-900">
                    {t.translation.screenshotUnsupported}：{screenshotCapability.reason}
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
                      onClick={handleScreenshot}
                      disabled={
                        isTranslating ||
                        (screenshotCapability !== null && !screenshotCapability.supported)
                      }
                      className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2.5 py-1.5 text-xs text-slate-600 shadow-sm hover:bg-slate-100 disabled:opacity-50"
                      title={
                        screenshotCapability && !screenshotCapability.supported
                          ? (screenshotCapability.reason ?? t.translation.screenshotUnsupported)
                          : t.translation.btnScreenshot
                      }
                    >
                      <Camera className="h-3.5 w-3.5 text-indigo-500" />
                      <span>{t.translation.btnScreenshot}</span>
                    </button>
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
                        onClick={runner.cancel}
                        className="rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-1 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700"
                      >
                        {t.task.btnCancel}
                      </button>
                    )}
                    <button
                      onClick={() =>
                        runner.run(sourceText, {
                          source: 'input',
                          // 用户已在下拉框显式选定目标语言，必须锁死它：
                          // 否则「中译中」润色场景会被智能对转逻辑改写成英文。
                          forceTargetLang: true,
                        })
                      }
                      disabled={isTranslating || !sourceText.trim()}
                      className="rounded bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 disabled:opacity-50"
                    >
                      {isTranslating ? t.translation.translating : t.translation.btnTranslate}
                    </button>
                  </div>
                </div>
              </div>

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
                  <div className="flex items-center gap-2">
                    {usedEngine && (
                      <span className="text-[10px] text-slate-400">engine: {usedEngine}</span>
                    )}
                    {runner.snapshot.durationMs != null && runner.snapshot.durationMs > 0 && (
                      <span className="text-[10px] text-slate-400">
                        · {(runner.snapshot.durationMs / 1000).toFixed(1)}s
                      </span>
                    )}
                  </div>
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
                    className="w-full rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500"
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
                    className="w-full rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500"
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
                      className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
                    />
                    <input
                      type="text"
                      value={deepseekModel}
                      onChange={(e) => setDeepseekModel(e.target.value)}
                      placeholder={t.settings.deepseekModelPlaceholder}
                      className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
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
                    className="w-full rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500"
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
                      className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
                    />
                    <input
                      type="text"
                      value={openaiModel}
                      onChange={(e) => setOpenaiModel(e.target.value)}
                      placeholder={t.settings.openaiModelPlaceholder}
                      className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
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

              {/* 截图翻译快捷键设置 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
                <div>
                  <div className="font-medium text-slate-700 dark:text-slate-200">
                    {t.translation.screenshotShortcutTitle}
                  </div>
                  <div className="text-[11px] text-slate-400">
                    {t.translation.screenshotShortcutDesc}
                  </div>
                </div>
                <ShortcutRecorder
                  value={customScreenshotShortcut}
                  onChange={setCustomScreenshotShortcut}
                  onSave={handleSaveScreenshotShortcut}
                  t={t}
                />
                {screenshotShortcutConflict && (
                  <div role="alert" className="text-[11px] text-amber-700 dark:text-amber-400">
                    {t.settings.shortcutRegisterFailed}: {screenshotShortcutConflict}
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
                  <option value="offline">{t.translation.engineOffline}</option>
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
                            false,
                            2000,
                          )
                        } else {
                          setSaveStatus(res.error, true, 2000)
                        }
                      } catch (err) {
                        setSaveStatus(
                          err instanceof Error && err.message
                            ? err.message
                            : t.settings.statusCacheClearFailed,
                          true,
                          2000,
                        )
                      }
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
                            const parsedTerms = parseGlossaryCsv(text)
                            if (parsedTerms.length === 0) {
                              setSaveStatus(
                                t.settings.glossaryImportDone.replace('{n}', '0'),
                                false,
                                3000,
                              )
                              return
                            }
                            try {
                              const res = await commands.importGlossaryTerms(
                                parsedTerms.map((term) => ({
                                  source: term.source,
                                  target: term.target,
                                  match_case: term.matchCase,
                                })),
                              )
                              if (res.status === 'error') {
                                setSaveStatus(res.error, true, 3000)
                              } else {
                                loadGlossaryList()
                                const imported = res.data
                                const failed = Math.max(0, parsedTerms.length - imported)
                                setSaveStatus(
                                  failed > 0
                                    ? t.settings.glossaryImportPartial
                                        .replace('{n}', String(imported))
                                        .replace('{m}', String(failed))
                                    : t.settings.glossaryImportDone.replace(
                                        '{n}',
                                        String(imported),
                                      ),
                                  failed > 0,
                                  3000,
                                )
                              }
                            } catch (err) {
                              setSaveStatus(
                                err instanceof Error && err.message
                                  ? err.message
                                  : t.settings.statusGlossaryFailed,
                                true,
                                3000,
                              )
                            }
                          }
                          reader.onerror = () => {
                            setSaveStatus(t.settings.statusGlossaryFailed, true, 3000)
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
                    className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500"
                  />
                  <input
                    type="text"
                    value={glossaryTarget}
                    onChange={(e) => setGlossaryTarget(e.target.value)}
                    placeholder={t.settings.glossaryTargetPlaceholder}
                    className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500"
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
                  <div className="mt-2 border-t border-slate-200 dark:border-slate-700 pt-2 flex flex-col gap-1.5">
                    <input
                      type="text"
                      value={glossarySearch}
                      onChange={(e) => setGlossarySearch(e.target.value)}
                      placeholder={t.settings.glossarySearchPlaceholder}
                      className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-800 dark:text-slate-200 outline-none focus:border-indigo-500"
                    />
                    <div className="max-h-36 overflow-y-auto flex flex-col gap-1.5">
                      {glossaryList
                        .filter((item) => {
                          const kw = glossarySearch.trim().toLowerCase()
                          if (!kw) return true
                          return (
                            item.source.toLowerCase().includes(kw) ||
                            item.target.toLowerCase().includes(kw)
                          )
                        })
                        .map((item) => (
                          <div
                            key={item.source}
                            className="flex items-center justify-between rounded bg-white dark:bg-slate-800 px-2 py-1 border border-slate-200 dark:border-slate-700 text-xs"
                          >
                            <div className="flex items-center gap-1.5 truncate">
                              <span className="font-semibold text-slate-700 dark:text-slate-200 truncate">
                                {item.source}
                              </span>
                              <span className="text-slate-400">→</span>
                              <span className="text-slate-600 dark:text-slate-300 truncate">
                                {item.target}
                              </span>
                              {!item.match_case && (
                                <span className="rounded bg-amber-50 dark:bg-amber-950/50 px-1 py-0.2 text-[10px] text-amber-600 dark:text-amber-400 border border-amber-200 dark:border-amber-800 shrink-0">
                                  Aa
                                </span>
                              )}
                            </div>
                            <button
                              onClick={() => handleDeleteGlossary(item.source)}
                              className="rounded p-0.5 text-slate-400 hover:bg-rose-50 dark:hover:bg-rose-950/50 hover:text-rose-600 shrink-0"
                              title={t.settings.btnDeleteGlossary}
                            >
                              <X className="h-3 w-3" />
                            </button>
                          </div>
                        ))}
                    </div>
                  </div>
                )}
              </div>

              {saveStatus && (
                <div
                  role={saveStatusIsError ? 'alert' : 'status'}
                  className={`rounded border p-2 text-center font-medium ${
                    saveStatusIsError
                      ? 'bg-rose-50 dark:bg-rose-950/50 border-rose-200 dark:border-rose-900/60 text-rose-600 dark:text-rose-400'
                      : 'bg-emerald-50 dark:bg-emerald-950/50 border-emerald-200 dark:border-emerald-900/60 text-emerald-700 dark:text-emerald-400'
                  }`}
                >
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
            className="w-full max-w-sm rounded-xl bg-white dark:bg-slate-900 p-5 shadow-2xl border border-indigo-100 dark:border-indigo-900 flex flex-col gap-4"
          >
            <div className="flex items-start gap-3">
              <div className="rounded-full bg-indigo-50 dark:bg-indigo-950/60 p-2 text-indigo-600 dark:text-indigo-400">
                <Key className="h-6 w-6" />
              </div>
              <div className="flex-1">
                <h3
                  id={PAIRING_DIALOG_TITLE_ID}
                  className="font-semibold text-slate-800 dark:text-slate-100 text-sm"
                >
                  {t.app.pairingModalTitle}
                </h3>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-1 leading-relaxed">
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
            className="flex w-full max-w-sm flex-col gap-4 rounded-xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 p-5 shadow-xl"
          >
            <h3
              id={OCR_DIALOG_TITLE_ID}
              className="text-sm font-semibold text-slate-800 dark:text-slate-100"
            >
              {t.translation.ocrOverwriteTitle}
            </h3>
            <p
              id={OCR_DIALOG_DESC_ID}
              className="text-xs leading-relaxed text-slate-500 dark:text-slate-400"
            >
              {t.translation.ocrOverwriteDesc.replace('{n}', String(sourceText.length))}
            </p>
            <div className="flex items-center justify-end gap-2 border-t border-slate-100 dark:border-slate-800 pt-3">
              <button
                onClick={closeOcrConfirm}
                className="rounded-lg px-3 py-1.5 text-xs font-medium text-slate-600 dark:text-slate-300 transition-colors hover:bg-slate-100 dark:hover:bg-slate-800"
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
