import { useState, useEffect } from 'react'
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
} from 'lucide-react'
import { commands } from './bindings'
import type { EngineCapabilitiesDto, ServerStatus } from './bindings'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getTranslation, Locale } from './i18n'
import { useTaskRunner } from './task/useTaskRunner'
import { canRetry } from './task/taskState'
import HistoryPanel from './HistoryPanel'

export default function App() {
  const [locale, setLocale] = useState<Locale>('zh-CN')
  const t = getTranslation(locale)

  const [sourceText, setSourceText] = useState('')
  const [targetLang, setTargetLang] = useState('zh-CN')
  const [engine, setEngine] = useState('google')
  const [copied, setCopied] = useState(false)
  const [serverToken, setServerToken] = useState<string>('')
  const [tokenCopied, setTokenCopied] = useState(false)

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
  const [serverStatus, setServerStatus] = useState<ServerStatus | null>(null)
  const [customShortcut, setCustomShortcut] = useState('Alt+Shift+D')

  // 浏览器扩展一键配对请求浮层
  const [pairingRequest, setPairingRequest] = useState<{
    sessionId: string
    browserHint?: string
    countdown: number
  } | null>(null)

  const isFloatWindow = window.location.hash === '#float'

  // 四类入口（输入框 / 划词 / 剪贴板 / 浮窗）共用同一任务状态机
  const runner = useTaskRunner(engine, targetLang)
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

    loadGlossaryList()
  }, [])

  useEffect(() => {
    // 监听本地浏览器插件配对请求
    let unlisten: (() => void) | undefined
    listen<{ session_id: string; browser_hint?: string }>(
      'transly://pairing-requested',
      async (event) => {
        setPairingRequest({
          sessionId: event.payload.session_id,
          browserHint: event.payload.browser_hint,
          countdown: 60,
        })
        try {
          const win = getCurrentWindow()
          await win.show()
          await win.setFocus()
        } catch {
          // 在无窗口或测试环境下静默回落
        }
      },
    ).then((fn) => {
      unlisten = fn
    })

    return () => {
      if (unlisten) unlisten()
    }
  }, [])

  useEffect(() => {
    if (!pairingRequest) return
    const timer = setInterval(() => {
      setPairingRequest((prev) => {
        if (!prev) return null
        if (prev.countdown <= 1) {
          return null
        }
        return { ...prev, countdown: prev.countdown - 1 }
      })
    }, 1000)

    return () => clearInterval(timer)
  }, [pairingRequest])

  const handleApprovePairing = async (sessionId: string) => {
    try {
      await commands.approvePairing(sessionId)
    } finally {
      setPairingRequest(null)
    }
  }

  const handleRejectPairing = async (sessionId: string) => {
    try {
      await commands.rejectPairing(sessionId)
    } finally {
      setPairingRequest(null)
    }
  }

  useEffect(() => {
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
    const res = await commands.setCacheEnabled(val)
    if (res.status === 'error') {
      setCacheEnabled(!val)
      setSaveStatus(res.error)
      setTimeout(() => setSaveStatus(null), 2500)
    }
  }

  const handleToggleClipboardMonitor = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.checked
    setClipboardMonitor(val)
    const res = await commands.setClipboardMonitor(val)
    if (res.status === 'error') {
      setClipboardMonitor(!val)
      setSaveStatus(res.error)
      setTimeout(() => setSaveStatus(null), 2500)
    }
  }

  const handleSaveShortcut = async () => {
    try {
      const res = await commands.setShortcut(customShortcut.trim())
      if (res.status === 'ok') {
        setSaveStatus(t.settings.statusShortcutSaved)
        setTimeout(() => setSaveStatus(null), 2000)
      } else {
        setSaveStatus(res.error)
      }
    } catch (err) {
      setSaveStatus(
        err instanceof Error && err.message ? err.message : t.settings.statusShortcutFailed,
      )
    }
  }

  const handleRotateToken = async () => {
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
    }
  }

  const handleCopyToken = () => {
    if (!serverToken) return
    navigator.clipboard.writeText(serverToken)
    setTokenCopied(true)
    setTimeout(() => setTokenCopied(false), 2000)
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
      // 保存 LLM 自定义端点与模型名称
      await commands.saveLlmConfig(
        'deepseek',
        deepseekBaseUrl.trim() || null,
        deepseekModel.trim() || null,
      )
      await commands.saveLlmConfig(
        'openai',
        openaiBaseUrl.trim() || null,
        openaiModel.trim() || null,
      )
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
    runner.cancel()
  }

  const handleCopy = () => {
    if (!translatedText) return
    navigator.clipboard.writeText(translatedText)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const handleSpeak = (text: string, lang: string) => {
    if (!('speechSynthesis' in window) || !text.trim()) return
    window.speechSynthesis.cancel()
    const utterance = new SpeechSynthesisUtterance(text.trim())
    utterance.lang = lang
    window.speechSynthesis.speak(utterance)
  }

  const handleOcrClipboard = async () => {
    setOcrLoading(true)
    setOcrError(null)
    try {
      const res = await commands.ocrClipboardImage(targetLang)
      if (res.status === 'ok') {
        const text = res.data.trim()
        setSourceText(text)
        // debounceMs: 0 立即投递统一任务状态机，秒级出译文
        runner.run(text, { source: 'clipboard', debounceMs: 0 })
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

  // 迷你悬浮小窗模式 (无边框、紧凑置顶卡片)
  if (isFloatWindow) {
    return (
      <div className="flex h-screen w-screen flex-col overflow-hidden rounded-xl border border-indigo-100 bg-white/95 p-3.5 shadow-2xl backdrop-blur-md select-none">
        <div className="flex items-center justify-between border-b border-slate-100 pb-2">
          <div className="flex items-center gap-1.5">
            <Languages className="h-4 w-4 text-indigo-600" />
            <span className="text-xs font-semibold text-slate-800">{t.float.title}</span>
          </div>
          <div className="flex items-center gap-2">
            {isTranslating && (
              <button
                onClick={runner.cancel}
                className="rounded bg-slate-100 px-1.5 py-0.5 text-[11px] text-slate-600 hover:bg-slate-200"
              >
                {t.task.btnCancel}
              </button>
            )}
            <select
              value={targetLang}
              onChange={(e) => setTargetLang(e.target.value)}
              className="rounded border border-slate-200 bg-slate-50 px-1.5 py-0.5 text-[11px] text-slate-700 outline-none"
            >
              <option value="zh-CN">{t.translation.languageZhCN}</option>
              <option value="en">{t.languages.en}</option>
              <option value="ja">{t.translation.languageJa}</option>
            </select>
          </div>
        </div>

        <div className="my-2 flex-1 overflow-auto rounded-lg bg-slate-50/70 p-2 text-xs leading-relaxed text-slate-800 border border-slate-100">
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
    <div className="flex h-screen flex-col bg-slate-50 font-sans">
      {/* 顶部标题栏 */}
      <header className="flex h-12 items-center justify-between border-b border-slate-200 bg-white px-4">
        <div className="flex items-center gap-2">
          <Languages className="h-5 w-5 text-indigo-600" />
          <span className="font-semibold text-slate-800">Transly</span>
        </div>
        <div className="flex items-center gap-3 text-xs text-slate-500">
          <button
            onClick={() => setLocale(locale === 'zh-CN' ? 'en' : 'zh-CN')}
            className="flex items-center gap-1 rounded bg-slate-100 px-2 py-1 text-slate-600 hover:bg-slate-200"
            title={t.app.switchLanguage}
          >
            <Globe className="h-3.5 w-3.5 text-indigo-500" />
            <span>{locale === 'zh-CN' ? 'EN' : t.translation.languageZhCN}</span>
          </button>
          {serverToken && (
            <button
              onClick={handleCopyToken}
              title={t.app.copyToken}
              className="flex items-center gap-1 rounded bg-slate-100 px-2 py-1 text-slate-600 hover:bg-slate-200"
            >
              <Key className="h-3.5 w-3.5 text-indigo-500" />
              <span>{tokenCopied ? t.app.tokenCopied : t.app.copyToken}</span>
            </button>
          )}
          <button
            onClick={() => setShowSettings(true)}
            className="flex items-center gap-1 rounded bg-slate-100 px-2 py-1 text-slate-600 hover:bg-slate-200"
            title={t.app.settings}
          >
            <Settings className="h-3.5 w-3.5 text-slate-500" />
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
                  {t.translation.autoDetect}
                </span>
                <ArrowRight className="h-4 w-4 text-slate-400" />
                <select
                  value={targetLang}
                  onChange={(e) => setTargetLang(e.target.value)}
                  className="rounded border border-slate-300 bg-white px-2 py-1 text-xs text-slate-700 shadow-sm focus:border-indigo-500 focus:outline-none"
                >
                  <option value="zh-CN">{t.languages['zh-CN']}</option>
                  <option value="en">{t.languages.en}</option>
                  <option value="ja">{t.languages.ja}</option>
                  <option value="ko">{t.languages.ko}</option>
                  <option value="fr">{t.languages.fr}</option>
                  <option value="de">{t.languages.de}</option>
                </select>
              </div>

              <div className="flex items-center gap-2">
                <span className="text-xs text-slate-500">{t.translation.engineSelect}:</span>
                <select
                  value={engine}
                  onChange={(e) => setEngine(e.target.value)}
                  className="rounded border border-slate-300 bg-white px-2 py-1 text-xs text-slate-700 shadow-sm focus:border-indigo-500 focus:outline-none"
                >
                  <option value="google">{t.translation.engineGoogle}</option>
                  <option value="deepl">{t.translation.engineDeepl}</option>
                  <option value="deepseek">{t.translation.engineDeepseek}</option>
                  <option value="openai">{t.translation.engineOpenai}</option>
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
              <div className="flex flex-col rounded-lg border border-slate-200 bg-white p-3 shadow-sm focus-within:border-indigo-500">
                <textarea
                  value={sourceText}
                  onChange={(e) => {
                    const v = e.target.value
                    setSourceText(v)
                    // 即输即译：500ms 防抖，新输入立即作废上一代在途结果
                    runner.run(v, { source: 'input', debounceMs: 500 })
                  }}
                  placeholder={t.translation.inputPlaceholder}
                  className="flex-1 resize-none bg-transparent text-sm text-slate-800 outline-none"
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
                        className="rounded border border-slate-200 bg-slate-50 px-2 py-1.5 text-xs text-slate-500 shadow-sm hover:bg-slate-100"
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
                        className="rounded border border-slate-200 bg-slate-50 px-2 py-1 text-xs text-slate-600 hover:bg-slate-100"
                      >
                        {t.task.btnCancel}
                      </button>
                    )}
                    <button
                      onClick={() => runner.run(sourceText, { source: 'input' })}
                      disabled={isTranslating || !sourceText.trim()}
                      className="rounded bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 disabled:opacity-50"
                    >
                      {isTranslating ? t.translation.translating : t.translation.btnTranslate}
                    </button>
                  </div>
                </div>
              </div>

              <div className="flex flex-col rounded-lg border border-slate-200 bg-white p-3 shadow-sm">
                <div className="flex-1 overflow-auto text-sm text-slate-800 whitespace-pre-wrap">
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
                        className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2 py-1 text-xs text-slate-600 hover:bg-slate-100"
                        title={t.translation.speak}
                      >
                        <Volume2 className="h-3.5 w-3.5" />
                        <span>{t.translation.speak}</span>
                      </button>
                      <button
                        onClick={handleCopy}
                        className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-100"
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
          <div className="flex max-h-[85vh] w-[480px] flex-col rounded-xl border border-slate-200 bg-white p-5 shadow-xl">
            <div className="flex items-center justify-between border-b border-slate-100 pb-3 shrink-0">
              <div className="flex items-center gap-2">
                <Settings className="h-4 w-4 text-indigo-600" />
                <h3 className="font-semibold text-slate-800 text-sm">{t.settings.title}</h3>
              </div>
              <button
                onClick={() => setShowSettings(false)}
                className="rounded p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="mt-4 flex flex-1 flex-col gap-4 overflow-y-auto pr-1 text-xs">
              {/* API 密钥管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 p-3 border border-slate-100">
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
                  <div className="mt-1.5 grid grid-cols-2 gap-2">
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
                  <div className="mt-1.5 grid grid-cols-2 gap-2">
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
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 p-3 border border-slate-100">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-medium text-slate-700">{t.settings.shortcutTitle}</div>
                    <div className="text-[11px] text-slate-400">{t.settings.shortcutDesc}</div>
                  </div>
                  <button
                    onClick={handleSaveShortcut}
                    className="rounded border border-indigo-200 bg-indigo-50 px-2.5 py-1 text-[11px] font-medium text-indigo-600 hover:bg-indigo-100"
                  >
                    {t.settings.btnSaveShortcut}
                  </button>
                </div>
                <input
                  type="text"
                  value={customShortcut}
                  onChange={(e) => setCustomShortcut(e.target.value)}
                  placeholder="Alt+Shift+D"
                  className="rounded border border-slate-200 bg-white px-2.5 py-1 text-slate-700 outline-none focus:border-indigo-500 font-mono text-[11px]"
                />
              </div>

              {/* 剪贴板监听开关 */}
              <div className="flex items-center justify-between rounded-lg bg-slate-50 p-3 border border-slate-100">
                <div>
                  <div className="font-medium text-slate-700">
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
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 p-3 border border-slate-100">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-medium text-slate-700">{t.settings.tokenRotateTitle}</div>
                    <div className="text-[11px] text-slate-400">{t.settings.tokenRotateDesc}</div>
                  </div>
                  <button
                    onClick={handleRotateToken}
                    className="rounded border border-indigo-200 bg-indigo-50 px-2.5 py-1 text-[11px] font-medium text-indigo-600 hover:bg-indigo-100"
                  >
                    {t.settings.btnRotateToken}
                  </button>
                </div>
                {/* 本地扩展服务状态：启动失败（如端口被占）此前只进 stderr，
                    用户只能看到扩展端“桌面端未运行”而无法自查 */}
                <div className="flex items-center justify-between border-t border-slate-200 pt-2">
                  <div className="text-[11px] text-slate-500">{t.settings.serverStatusTitle}</div>
                  <span
                    className={
                      serverStatus?.state === 'running'
                        ? 'text-[11px] font-medium text-emerald-600'
                        : serverStatus?.state === 'failed'
                          ? 'text-[11px] font-medium text-rose-600'
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
                  <div className="rounded bg-rose-50 border border-rose-200 px-2 py-1 text-[11px] leading-snug text-rose-600">
                    {serverStatus.reason}
                  </div>
                )}
              </div>

              {/* 缓存数据管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 p-3 border border-slate-100">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="font-medium text-slate-700">{t.settings.cachePersistTitle}</div>
                    <div className="text-[11px] text-slate-400">{t.settings.cachePersistDesc}</div>
                  </div>
                  <input
                    type="checkbox"
                    checked={cacheEnabled}
                    onChange={handleToggleCacheEnabled}
                    className="h-4 w-4 rounded text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                  />
                </div>
                <div className="flex items-center justify-between border-t border-slate-200 pt-2 mt-1">
                  <div>
                    <div className="font-medium text-slate-700">{t.settings.cacheClearTitle}</div>
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
                    className="rounded border border-rose-200 bg-rose-50 px-2.5 py-1 text-[11px] font-medium text-rose-600 hover:bg-rose-100"
                  >
                    {t.settings.btnClearCache}
                  </button>
                </div>
              </div>

              {/* 术语库管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 p-3 border border-slate-100">
                <span className="font-medium text-slate-700">{t.settings.glossaryTitle}</span>
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
          <div className="w-full max-w-sm rounded-xl bg-white p-5 shadow-2xl border border-indigo-100 flex flex-col gap-4">
            <div className="flex items-start gap-3">
              <div className="rounded-full bg-indigo-50 p-2 text-indigo-600">
                <Key className="h-6 w-6" />
              </div>
              <div className="flex-1">
                <h3 className="font-semibold text-slate-800 text-sm">{t.app.pairingModalTitle}</h3>
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

            <div className="flex items-center justify-end gap-2 pt-2 border-t border-slate-100">
              <button
                onClick={() => handleRejectPairing(pairingRequest.sessionId)}
                className="rounded-lg px-3 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-100 transition-colors"
              >
                {t.app.pairingReject}
              </button>
              <button
                onClick={() => handleApprovePairing(pairingRequest.sessionId)}
                className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 transition-colors flex items-center gap-1.5"
              >
                <span>{t.app.pairingAllow}</span>
                <span className="text-[10px] bg-indigo-700/60 rounded px-1">
                  {pairingRequest.countdown}s
                </span>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
