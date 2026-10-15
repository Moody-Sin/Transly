import { useState, useEffect } from 'react'
import { Languages, Copy, Check, Volume2, Pin, PinOff } from 'lucide-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { commands } from './bindings'
import { getTranslation, Locale } from './i18n'
import { useTaskRunner } from './task/useTaskRunner'
import { canRetry } from './task/taskState'
import { speakText } from './utils/speech'
import { parseLocalePreference, parseThemePreference, LOCALE_STORAGE_KEY } from './utils/prefs'

export default function FloatWindow() {
  const [locale, setLocale] = useState<Locale>(() =>
    parseLocalePreference(localStorage.getItem(LOCALE_STORAGE_KEY)),
  )
  const t = getTranslation(locale)
  const [targetLang, setTargetLang] = useState('zh-CN')
  const [engine, setEngine] = useState('google')
  const [copied, setCopied] = useState(false)
  const [copyError, setCopyError] = useState<string | null>(null)
  const [isFloatPinned, setIsFloatPinned] = useState(false)

  // 跟随主窗口切换界面语言：悬浮窗是独立窗口，状态不共享，
  // 原先 locale 直接写死 zh-CN，主窗口切成英文后浮窗仍是中文。
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key !== LOCALE_STORAGE_KEY) return
      setLocale(parseLocalePreference(e.newValue))
    }
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [])

  // 同步暗黑模式主题类名（并监听主窗口切换主题时的 localStorage 变化）
  useEffect(() => {
    const syncTheme = () => {
      const theme = parseThemePreference(localStorage.getItem('transly_theme'))
      const isDark =
        theme === 'dark' ||
        (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
      if (isDark) {
        document.documentElement.classList.add('dark')
      } else {
        document.documentElement.classList.remove('dark')
      }
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

  // 迷你悬浮窗启动时尝试读取用户首选翻译引擎
  useEffect(() => {
    commands
      .getPreferredReplaceEngine()
      .then((res) => {
        if (res.status === 'ok' && res.data) {
          setEngine(res.data)
        }
      })
      .catch(() => {})
  }, [])

  const runner = useTaskRunner(engine, targetLang, { isPinned: isFloatPinned })
  const { state, text: translatedText, error } = runner.snapshot
  const isTranslating = state === 'running'

  // 迷你悬浮窗 Escape 键极速关闭交互与 Ctrl+S / Cmd+S 快速对转切换语言
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        getCurrentWindow()
          .hide()
          .catch((err) => console.warn('[Transly] 隐藏浮窗失败：', err))
      } else if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
        e.preventDefault()
        // 对转基准以当前生效语言为准：如果是有效目标为英文，则转为中文，反之转为英文
        const currentActive = runner.snapshot.effectiveTargetLang || targetLang
        const nextLang =
          currentActive === 'zh-CN' || currentActive.startsWith('zh') ? 'en' : 'zh-CN'
        setTargetLang(nextLang)
        if (runner.snapshot.sourceText) {
          // 无论当前是否在运行，直接打断并以 forceTargetLang 显式发起新目标语言翻译
          runner.run(runner.snapshot.sourceText, { targetLang: nextLang, forceTargetLang: true })
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [targetLang, runner])

  const showNetworkEngineHint =
    error?.kind === 'network' && (engine === 'google' || engine === 'google_web')

  const handleCopy = () => {
    if (!translatedText) return
    navigator.clipboard
      .writeText(translatedText)
      .then(() => {
        setCopyError(null)
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
      })
      .catch((e: unknown) => {
        setCopyError(e instanceof Error && e.message ? e.message : t.translation.copyFailed)
        setTimeout(() => setCopyError(null), 2500)
      })
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
            value={engine}
            onChange={(e) => {
              const newEng = e.target.value
              setEngine(newEng)
              // 不再用 `!isTranslating` 拦截：翻译进行中用户改用其他引擎
              // （例如等云端 LLM 太久想换 Google）正是最需要生效的时刻，
              // 拦截会让切换静默失效、旧引擎结果照旧覆盖上来。
              // runner.run 内部已递增代际并 cancelStreamTask，旧任务会被正确作废。
              if (runner.snapshot.sourceText) {
                // 同步传入新引擎：setEngine 是异步的，此刻 runner 内部 ref 仍是旧值
                runner.run(runner.snapshot.sourceText, { engine: newEng })
              }
            }}
            className="rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-1.5 py-0.5 text-[11px] text-slate-700 dark:text-slate-200 outline-none"
          >
            <option value="google">{t.translation.engineGoogle}</option>
            <option value="offline">{t.translation.engineOffline}</option>
            <option value="deepl">{t.translation.engineDeepl}</option>
            <option value="deepseek">{t.translation.engineDeepseek}</option>
            <option value="openai">{t.translation.engineOpenai}</option>
          </select>
          <select
            value={targetLang}
            onChange={(e) => {
              const newLang = e.target.value
              setTargetLang(newLang)
              if (runner.snapshot.sourceText) {
                // 用户在下拉框显式指定目标语言：传入 forceTargetLang 避免被离线语言检测覆盖
                runner.run(runner.snapshot.sourceText, {
                  targetLang: newLang,
                  forceTargetLang: true,
                })
              }
            }}
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
        <span
          className={`text-[10px] truncate max-w-[200px] ${copyError ? 'text-rose-500' : 'text-slate-400'}`}
        >
          {copyError
            ? copyError
            : state === 'cancelled'
              ? t.task.cancelled
              : runner.snapshot.sourceText
                ? `“${runner.snapshot.sourceText.slice(0, 24)}...”${runner.snapshot.durationMs != null && runner.snapshot.durationMs > 0 ? ` (${(runner.snapshot.durationMs / 1000).toFixed(1)}s)` : ''}`
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
