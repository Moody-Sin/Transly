import { useState, useEffect } from 'react'
import { Languages, ArrowRight, Copy, Check, Key, Settings, X, Save, Globe } from 'lucide-react'
import { commands } from './bindings'
import { getTranslation, Locale } from './i18n'
import { useTaskRunner } from './task/useTaskRunner'
import { canRetry } from './task/taskState'

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
  const [deeplConfigured, setDeeplConfigured] = useState(false)
  const [deepseekConfigured, setDeepseekConfigured] = useState(false)
  const [openaiConfigured, setOpenaiConfigured] = useState(false)
  const [deeplKey, setDeeplKey] = useState('')
  const [deepseekKey, setDeepseekKey] = useState('')
  const [openaiKey, setOpenaiKey] = useState('')
  const [clipboardMonitor, setClipboardMonitor] = useState(false)
  const [cacheEnabled, setCacheEnabled] = useState(true)
  const [glossarySource, setGlossarySource] = useState('')
  const [glossaryTarget, setGlossaryTarget] = useState('')
  const [glossaryList, setGlossaryList] = useState<[string, string][]>([])
  const [saveStatus, setSaveStatus] = useState<string | null>(null)

  const isFloatWindow = window.location.hash === '#float'

  // 四类入口（输入框 / 划词 / 剪贴板 / 浮窗）共用同一任务状态机
  const runner = useTaskRunner(engine, targetLang)
  const { state, text: translatedText, error, engine: usedEngine } = runner.snapshot
  const isTranslating = state === 'running' || state === 'queued'

  const loadGlossaryList = () => {
    commands.getGlossaryTerms().then((res) => {
      if (res.status === 'ok') {
        setGlossaryList(res.data)
      }
    }).catch(() => {})
  }

  const checkConfiguredKeys = () => {
    commands.isApiKeyConfigured('deepl').then(setDeeplConfigured).catch(() => {})
    commands.isApiKeyConfigured('deepseek').then(setDeepseekConfigured).catch(() => {})
    commands.isApiKeyConfigured('openai').then(setOpenaiConfigured).catch(() => {})
  }

  useEffect(() => {
    // 获取本地服务器 Token 供插件端授权配对
    commands.getServerToken().then((res) => {
      if (res.status === 'ok') {
        setServerToken(res.data)
      }
    }).catch(() => {})

    checkConfiguredKeys()

    commands.getClipboardMonitor().then((enabled) => {
      setClipboardMonitor(enabled)
    }).catch(() => {})

    commands.getCacheEnabled().then((enabled) => {
      setCacheEnabled(enabled)
    }).catch(() => {})

    loadGlossaryList()
  }, [])

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

  const handleRotateToken = async () => {
    try {
      const res = await commands.rotateServerToken()
      if (res.status === 'ok') {
        setServerToken(res.data)
        setSaveStatus('Token 已重新生成，旧插件配对已自动失效！')
        setTimeout(() => setSaveStatus(null), 2500)
      } else {
        setSaveStatus(res.error)
      }
    } catch {
      setSaveStatus('重新生成 Token 失败')
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
      checkConfiguredKeys()
      setSaveStatus(t.settings.statusKeysSaved)
      setTimeout(() => setSaveStatus(null), 2500)
    } catch {
      setSaveStatus(t.settings.statusSaveFailed)
    }
  }

  const handleDeleteKey = async (eng: string) => {
    try {
      const res = await commands.deleteApiKey(eng)
      if (res.status === 'error') throw new Error(res.error)
      checkConfiguredKeys()
      setSaveStatus('密钥已从系统凭据库移除')
      setTimeout(() => setSaveStatus(null), 2000)
    } catch {
      setSaveStatus('删除密钥失败')
    }
  }

  const handleAddGlossary = async () => {
    if (!glossarySource.trim() || !glossaryTarget.trim()) return
    try {
      const res = await commands.addGlossaryTerm(glossarySource.trim(), glossaryTarget.trim())
      if (res.status === 'error') throw new Error(res.error)
      setGlossarySource('')
      setGlossaryTarget('')
      loadGlossaryList()
      setSaveStatus(t.settings.statusGlossaryAdded)
      setTimeout(() => setSaveStatus(null), 2000)
    } catch {
      setSaveStatus(t.settings.statusGlossaryFailed)
    }
  }

  const handleDeleteGlossary = async (source: string) => {
    try {
      const res = await commands.deleteGlossaryTerm(source)
      if (res.status === 'error') throw new Error(res.error)
      loadGlossaryList()
    } catch {
      setSaveStatus(t.settings.statusGlossaryFailed)
    }
  }

  const handleCopy = () => {
    if (!translatedText) return
    navigator.clipboard.writeText(translatedText)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
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
              <option value="zh-CN">中文</option>
              <option value="en">EN</option>
              <option value="ja">日文</option>
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
            <button
              onClick={handleCopy}
              className="flex items-center gap-1 rounded bg-indigo-50 px-2 py-0.5 text-[11px] font-medium text-indigo-600 hover:bg-indigo-100"
            >
              {copied ? <Check className="h-3 w-3 text-emerald-600" /> : <Copy className="h-3 w-3" />}
              <span>{copied ? t.translation.copied : t.translation.copy}</span>
            </button>
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
            title="切换语言 / Switch Language"
          >
            <Globe className="h-3.5 w-3.5 text-indigo-500" />
            <span>{locale === 'zh-CN' ? 'EN' : '中文'}</span>
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
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <span className="text-xs font-medium text-slate-500">{t.translation.autoDetect}</span>
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
              <option value="google">Google 网页翻译 (免Key)</option>
              <option value="deepl">DeepL (需API Key / 自动降级)</option>
              <option value="deepseek">DeepSeek LLM (需API Key / 自动降级)</option>
              <option value="openai">OpenAI (需API Key / 自动降级)</option>
            </select>
          </div>
        </div>

        <div className="grid flex-1 grid-cols-2 gap-4">
          <div className="flex flex-col rounded-lg border border-slate-200 bg-white p-3 shadow-sm focus-within:border-indigo-500">
            <textarea
              value={sourceText}
              onChange={(e) => setSourceText(e.target.value)}
              placeholder={t.translation.inputPlaceholder}
              className="flex-1 resize-none bg-transparent text-sm text-slate-800 outline-none"
            />
            {error && (
              <div className="mt-1 flex items-start gap-2 text-xs">
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
            )}
            <div className="mt-2 flex items-center justify-end gap-2">
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

          <div className="flex flex-col rounded-lg border border-slate-200 bg-white p-3 shadow-sm">
            <div className="flex-1 overflow-auto text-sm text-slate-800">
              {translatedText || <span className="text-slate-400">{t.translation.outputPlaceholder}</span>}
            </div>
            <div className="mt-2 flex items-center justify-between">
              {usedEngine && (
                <span className="text-[10px] text-slate-400">engine: {usedEngine}</span>
              )}
              {translatedText && (
                <div className="flex justify-end">
                  <button
                    onClick={handleCopy}
                    className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-100"
                  >
                    {copied ? <Check className="h-3.5 w-3.5 text-emerald-600" /> : <Copy className="h-3.5 w-3.5" />}
                    <span>{copied ? t.translation.copied : t.translation.copy}</span>
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>
      </main>
      {/* 设置与密钥配置模态框 */}
      {showSettings && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 backdrop-blur-sm">
          <div className="w-[480px] rounded-xl border border-slate-200 bg-white p-5 shadow-xl">
            <div className="flex items-center justify-between border-b border-slate-100 pb-3">
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

            <div className="mt-4 flex flex-col gap-4 text-xs">
              {/* API 密钥管理 */}
              <div className="flex flex-col gap-2 rounded-lg bg-slate-50 p-3 border border-slate-100">
                <span className="font-medium text-slate-700">{t.settings.credentialsTitle}</span>
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <label className="text-slate-500">{t.settings.deeplKeyLabel}</label>
                    {deeplConfigured && (
                      <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
                        已配置 ✓
                        <button
                          onClick={() => handleDeleteKey('deepl')}
                          className="text-slate-400 hover:text-rose-600 ml-1 underline"
                        >
                          删除
                        </button>
                      </span>
                    )}
                  </div>
                  <input
                    type="password"
                    value={deeplKey}
                    onChange={(e) => setDeeplKey(e.target.value)}
                    placeholder={deeplConfigured ? '重新输入以覆盖当前密钥' : '例如: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx:fx'}
                    className="w-full rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                </div>
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <label className="text-slate-500">{t.settings.deepseekKeyLabel}</label>
                    {deepseekConfigured && (
                      <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
                        已配置 ✓
                        <button
                          onClick={() => handleDeleteKey('deepseek')}
                          className="text-slate-400 hover:text-rose-600 ml-1 underline"
                        >
                          删除
                        </button>
                      </span>
                    )}
                  </div>
                  <input
                    type="password"
                    value={deepseekKey}
                    onChange={(e) => setDeepseekKey(e.target.value)}
                    placeholder={deepseekConfigured ? '重新输入以覆盖当前密钥' : '例如: sk-xxxxxxxx'}
                    className="w-full rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                </div>
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <label className="text-slate-500">OpenAI API Key:</label>
                    {openaiConfigured && (
                      <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
                        已配置 ✓
                        <button
                          onClick={() => handleDeleteKey('openai')}
                          className="text-slate-400 hover:text-rose-600 ml-1 underline"
                        >
                          删除
                        </button>
                      </span>
                    )}
                  </div>
                  <input
                    type="password"
                    value={openaiKey}
                    onChange={(e) => setOpenaiKey(e.target.value)}
                    placeholder={openaiConfigured ? '重新输入以覆盖当前密钥' : '例如: sk-proj-xxxxxxxx'}
                    className="w-full rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
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

              {/* 剪贴板监听开关 */}
              <div className="flex items-center justify-between rounded-lg bg-slate-50 p-3 border border-slate-100">
                <div>
                  <div className="font-medium text-slate-700">{t.settings.clipboardMonitorLabel}</div>
                  <div className="text-[11px] text-slate-400">{t.settings.clipboardMonitorDesc}</div>
                </div>
                <input
                  type="checkbox"
                  checked={clipboardMonitor}
                  onChange={handleToggleClipboardMonitor}
                  className="h-4 w-4 rounded text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                />
              </div>

              {/* 插件通信 Token 轮换管理 */}
              <div className="flex items-center justify-between rounded-lg bg-slate-50 p-3 border border-slate-100">
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
                      const res = await commands.clearCache()
                      if (res.status === 'ok') {
                        setSaveStatus(`已清理 ${res.data} 条缓存记录`)
                        setTimeout(() => setSaveStatus(null), 2000)
                      }
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
                <div className="flex justify-end pt-1">
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
                    {glossaryList.map(([src, tgt]) => (
                      <div
                        key={src}
                        className="flex items-center justify-between rounded bg-white px-2 py-1 border border-slate-200"
                      >
                        <div className="flex items-center gap-1.5">
                          <span className="font-semibold text-slate-700">{src}</span>
                          <span className="text-slate-400">→</span>
                          <span className="text-slate-600">{tgt}</span>
                        </div>
                        <button
                          onClick={() => handleDeleteGlossary(src)}
                          className="rounded p-0.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600"
                          title="删除术语"
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
    </div>
  )
}
