import { useState } from 'react'
import { Languages, ArrowRight, Copy, Check } from 'lucide-react'
import { commands } from './bindings'

export default function App() {
  const [sourceText, setSourceText] = useState('')
  const [translatedText, setTranslatedText] = useState('')
  const [targetLang, setTargetLang] = useState('zh-CN')
  const [isTranslating, setIsTranslating] = useState(false)
  const [copied, setCopied] = useState(false)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const handleTranslate = async () => {
    if (!sourceText.trim()) return
    setIsTranslating(true)
    setErrorMsg(null)
    try {
      // 使用 tauri-specta 强类型命令安全调用
      const res = await commands.translate(sourceText, 'auto', targetLang)
      if (res.status === 'ok') {
        setTranslatedText(res.data)
      } else {
        setErrorMsg(res.error)
      }
    } catch (err: unknown) {
      console.warn('Tauri IPC 暂不可用或调用失败，降级本地预览模式:', err)
      // 若在纯浏览器环境预览，提供占位回退
      setTranslatedText(`[${targetLang}] ${sourceText}`)
    } finally {
      setIsTranslating(false)
    }
  }

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
        <div className="flex items-center gap-2 text-xs text-slate-500">
          <span>引擎: 默认</span>
        </div>
      </header>

      {/* 翻译工作区 */}
      <main className="flex flex-1 flex-col p-4 gap-4">
        <div className="flex items-center gap-3">
          <span className="text-xs font-medium text-slate-500">自动检测</span>
          <ArrowRight className="h-4 w-4 text-slate-400" />
          <select
            value={targetLang}
            onChange={(e) => setTargetLang(e.target.value)}
            className="rounded border border-slate-300 bg-white px-2 py-1 text-xs text-slate-700 shadow-sm focus:border-indigo-500 focus:outline-none"
          >
            <option value="zh-CN">简体中文 (zh-CN)</option>
            <option value="en">英语 (en)</option>
            <option value="ja">日语 (ja)</option>
          </select>
        </div>

        <div className="grid flex-1 grid-cols-2 gap-4">
          <div className="flex flex-col rounded-lg border border-slate-200 bg-white p-3 shadow-sm focus-within:border-indigo-500">
            <textarea
              value={sourceText}
              onChange={(e) => setSourceText(e.target.value)}
              placeholder="输入或粘贴需要翻译的文本..."
              className="flex-1 resize-none bg-transparent text-sm text-slate-800 outline-none"
            />
            {errorMsg && (
              <div className="text-xs text-rose-500 mt-1">{errorMsg}</div>
            )}
            <div className="mt-2 flex justify-end">
              <button
                onClick={handleTranslate}
                disabled={isTranslating || !sourceText.trim()}
                className="rounded bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-indigo-700 disabled:opacity-50"
              >
                {isTranslating ? '翻译中...' : '翻译'}
              </button>
            </div>
          </div>

          <div className="flex flex-col rounded-lg border border-slate-200 bg-white p-3 shadow-sm">
            <div className="flex-1 overflow-auto text-sm text-slate-800">
              {translatedText || <span className="text-slate-400">译文将显示在这里...</span>}
            </div>
            {translatedText && (
              <div className="mt-2 flex justify-end">
                <button
                  onClick={handleCopy}
                  className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2.5 py-1 text-xs text-slate-600 hover:bg-slate-100"
                >
                  {copied ? <Check className="h-3.5 w-3.5 text-emerald-600" /> : <Copy className="h-3.5 w-3.5" />}
                  <span>{copied ? '已复制' : '复制'}</span>
                </button>
              </div>
            )}
          </div>
        </div>
      </main>
    </div>
  )
}
