import { useState, useEffect } from 'react'
import { Languages, Copy, Check, Volume2, Pin, PinOff } from 'lucide-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { commands } from './bindings'
import { getTranslation, Locale } from './i18n'
import { useTaskRunner } from './task/useTaskRunner'
import { canRetry } from './task/taskState'
import { speakText } from './utils/speech'

export default function FloatWindow() {
  const [locale] = useState<Locale>('zh-CN')
  const t = getTranslation(locale)
  const [targetLang, setTargetLang] = useState('zh-CN')
  const [engine] = useState('google')
  const [copied, setCopied] = useState(false)
  const [isFloatPinned, setIsFloatPinned] = useState(false)

  // 迷你悬浮窗 Escape 键极速关闭交互
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        getCurrentWindow()
          .hide()
          .catch((err) => console.warn('[Transly] 隐藏浮窗失败：', err))
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  const runner = useTaskRunner(engine, targetLang, { isPinned: isFloatPinned })
  const { state, text: translatedText, error } = runner.snapshot
  const isTranslating = state === 'running'

  const showNetworkEngineHint =
    error?.kind === 'network' && (engine === 'google' || engine === 'google_web')

  const handleCopy = () => {
    if (!translatedText) return
    navigator.clipboard
      .writeText(translatedText)
      .then(() => {
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
      })
      .catch(() => {})
  }

  const handleSpeak = (text: string, lang: string) => {
    speakText(text, lang, runner.snapshot.detectedLang)
  }

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
