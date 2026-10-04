import { useState, useEffect, useCallback } from 'react'
import { Languages, ArrowRight, Copy, Check, Key, Settings, X, Save, Globe } from 'lucide-react'
import { listen } from '@tauri-apps/api/event'
import { commands } from './bindings'
import { getTranslation, Locale } from './i18n'

export default function App() {
  const [locale, setLocale] = useState<Locale>('zh-CN')
  const t = getTranslation(locale)

  const [sourceText, setSourceText] = useState('')
  const [translatedText, setTranslatedText] = useState('')
  const [targetLang, setTargetLang] = useState('zh-CN')
  const [engine, setEngine] = useState('google')
  const [isTranslating, setIsTranslating] = useState(false)
  const [copied, setCopied] = useState(false)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [serverToken, setServerToken] = useState<string>('')
  const [tokenCopied, setTokenCopied] = useState(false)

  // 设置面板状态
  const [showSettings, setShowSettings] = useState(false)
  const [deeplKey, setDeeplKey] = useState('')
  const [deepseekKey, setDeepseekKey] = useState('')
  const [glossarySource, setGlossarySource] = useState('')
  const [glossaryTarget, setGlossaryTarget] = useState('')
  const [glossaryList, setGlossaryList] = useState<[string, string][]>([])
  const [saveStatus, setSaveStatus] = useState<string | null>(null)

  const loadGlossaryList = () => {
    commands.getGlossaryTerms().then((res) => {
      if (res.status === 'ok') {
        setGlossaryList(res.data)
      }
    }).catch(() => {})
  }

  useEffect(() => {
    // 获取本地服务器 Token 供插件端授权配对
    commands.getServerToken().then((res) => {
      if (res.status === 'ok') {
        setServerToken(res.data)
      }
    }).catch(() => {})

    // 初始化读取已配置的密钥（若有）
    commands.getApiKey('deepl').then((res) => {
      if (res.status === 'ok') setDeeplKey(res.data)
    }).catch(() => {})
    commands.getApiKey('deepseek').then((res) => {
      if (res.status === 'ok') setDeepseekKey(res.data)
    }).catch(() => {})

    loadGlossaryList()
  }, [])

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
        await commands.saveApiKey('deepl', deeplKey.trim())
      }
      if (deepseekKey.trim()) {
        await commands.saveApiKey('deepseek', deepseekKey.trim())
      }
      setSaveStatus(t.settings.statusKeysSaved)
      setTimeout(() => setSaveStatus(null), 2500)
    } catch {
      setSaveStatus(t.settings.statusSaveFailed)
    }
  }

  const handleAddGlossary = async () => {
    if (!glossarySource.trim() || !glossaryTarget.trim()) return
    try {
      await commands.addGlossaryTerm(glossarySource.trim(), glossaryTarget.trim())
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
      await commands.deleteGlossaryTerm(source)
      loadGlossaryList()
    } catch {
      setSaveStatus(t.settings.statusGlossaryFailed)
    }
  }

  const handleTranslate = useCallback(async (customText?: string) => {
    const textToTranslate = customText !== undefined ? customText : sourceText
    if (!textToTranslate.trim()) return
    setIsTranslating(true)
    setErrorMsg(null)
    try {
      // 使用 tauri-specta 强类型命令安全调用，传递所选引擎
      const res = await commands.translate(textToTranslate, 'auto', targetLang, engine)
      if (res.status === 'ok') {
        setTranslatedText(res.data)
      } else {
        setErrorMsg(res.error)
      }
    } catch (err: unknown) {
      console.warn('Tauri IPC 暂不可用或调用失败，降级本地预览模式:', err)
      // 若在纯浏览器环境预览，提供占位回退
      setTranslatedText(`[${targetLang}] ${textToTranslate}`)
    } finally {
      setIsTranslating(false)
    }
  }, [sourceText, targetLang, engine])

  // 监听全局快捷键划词/剪贴板翻译事件
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<string>('quick-translate', (event) => {
      const text = event.payload
      if (text && text.trim()) {
        setSourceText(text)
        handleTranslate(text)
      }
    }).then((fn) => {
      unlisten = fn
    }).catch(() => {})

    return () => {
      if (unlisten) unlisten()
    }
  }, [handleTranslate])

  const handleCopy = () => {
    if (!translatedText) return
    navigator.clipboard.writeText(translatedText)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
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
            {errorMsg && (
              <div className="text-xs text-rose-500 mt-1">{errorMsg}</div>
            )}
            <div className="mt-2 flex justify-end">
              <button
                onClick={() => handleTranslate()}
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
            {translatedText && (
              <div className="mt-2 flex justify-end">
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
                  <label className="text-slate-500 mb-1 block">{t.settings.deeplKeyLabel}</label>
                  <input
                    type="password"
                    value={deeplKey}
                    onChange={(e) => setDeeplKey(e.target.value)}
                    placeholder="例如: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx:fx"
                    className="w-full rounded border border-slate-200 bg-white px-2.5 py-1.5 text-slate-800 outline-none focus:border-indigo-500"
                  />
                </div>
                <div>
                  <label className="text-slate-500 mb-1 block">{t.settings.deepseekKeyLabel}</label>
                  <input
                    type="password"
                    value={deepseekKey}
                    onChange={(e) => setDeepseekKey(e.target.value)}
                    placeholder="例如: sk-xxxxxxxx"
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
