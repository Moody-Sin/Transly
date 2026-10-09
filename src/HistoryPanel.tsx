import { useCallback, useEffect, useState } from 'react'
import { Copy, Check, Trash2, RefreshCw, Search, Download, Volume2 } from 'lucide-react'
import { commands } from './bindings'
import type { CacheEntry } from './bindings'
import type { I18nSchema, Locale } from './i18n'

const PAGE_SIZE = 50

interface Props {
  t: I18nSchema
  locale: Locale
}

/**
 * 翻译历史面板：把已存在的 `translation_cache` 表变为用户可见的「最近翻译」。
 * 支持关键词搜索过滤、CSV/JSON 一键导出、发音朗读。
 */
export default function HistoryPanel({ t, locale }: Props) {
  const [entries, setEntries] = useState<CacheEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [hasMore, setHasMore] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [copiedKey, setCopiedKey] = useState<string | null>(null)
  const [searchKeyword, setSearchKeyword] = useState('')

  const load = useCallback(
    async (offset: number, keyword: string) => {
      setLoading(true)
      setError(null)
      try {
        const res = keyword.trim()
          ? await commands.searchTranslationHistory(keyword.trim(), PAGE_SIZE, offset)
          : await commands.getTranslationHistory(PAGE_SIZE, offset)

        if (res.status === 'ok') {
          setEntries((prev) => (offset === 0 ? res.data : [...prev, ...res.data]))
          setHasMore(res.data.length === PAGE_SIZE)
        } else {
          setError(res.error)
        }
      } catch (e) {
        setError(e instanceof Error && e.message ? e.message : t.history.loadFailed)
      } finally {
        setLoading(false)
      }
    },
    [t],
  )

  useEffect(() => {
    const timer = setTimeout(() => {
      load(0, searchKeyword)
    }, 300)
    return () => clearTimeout(timer)
  }, [load, searchKeyword])

  const handleCopy = (entry: CacheEntry) => {
    navigator.clipboard.writeText(entry.translated_text)
    setCopiedKey(entry.cache_key)
    setTimeout(() => setCopiedKey(null), 1500)
  }

  const handleDelete = async (entry: CacheEntry) => {
    try {
      const res = await commands.deleteHistoryEntry(entry.cache_key)
      if (res.status === 'ok') {
        setEntries((prev) => prev.filter((e) => e.cache_key !== entry.cache_key))
      } else {
        setError(res.error)
      }
    } catch (e) {
      setError(e instanceof Error && e.message ? e.message : t.history.deleteFailed)
    }
  }

  const handleSpeak = (text: string, lang: string) => {
    if (!('speechSynthesis' in window)) return
    window.speechSynthesis.cancel()
    const utterance = new SpeechSynthesisUtterance(text)
    utterance.lang = lang
    window.speechSynthesis.speak(utterance)
  }

  const handleExportJson = () => {
    if (entries.length === 0) return
    const blob = new Blob([JSON.stringify(entries, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `transly-history-${new Date().toISOString().slice(0, 10)}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  const handleExportCsv = () => {
    if (entries.length === 0) return
    const header = [
      'Source Text',
      'Translated Text',
      'Source Lang',
      'Target Lang',
      'Engine',
      'Time',
    ]
    const rows = entries.map((e) => [
      `"${e.source_text.replace(/"/g, '""')}"`,
      `"${e.translated_text.replace(/"/g, '""')}"`,
      e.source_lang,
      e.target_lang,
      e.engine,
      new Date(e.created_at * 1000).toISOString(),
    ])
    const csvContent = '\uFEFF' + [header.join(','), ...rows.map((r) => r.join(','))].join('\n')
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `transly-history-${new Date().toISOString().slice(0, 10)}.csv`
    a.click()
    URL.revokeObjectURL(url)
  }

  const formatTime = (seconds: number) => new Date(seconds * 1000).toLocaleString(locale)

  return (
    <div className="flex flex-1 flex-col overflow-hidden rounded-lg border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 shadow-sm text-slate-800 dark:text-slate-100">
      <div className="flex flex-col gap-2 border-b border-slate-100 dark:border-slate-800 px-4 py-2.5">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium text-slate-700 dark:text-slate-200">
            {t.history.title}
          </span>
          <div className="flex items-center gap-1.5">
            {entries.length > 0 && (
              <>
                <button
                  onClick={handleExportJson}
                  className="flex items-center gap-1 rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700"
                  title={t.history.exportJson}
                >
                  <Download className="h-3 w-3" />
                  <span>JSON</span>
                </button>
                <button
                  onClick={handleExportCsv}
                  className="flex items-center gap-1 rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700"
                  title={t.history.exportCsv}
                >
                  <Download className="h-3 w-3" />
                  <span>CSV</span>
                </button>
              </>
            )}
            <button
              onClick={() => load(0, searchKeyword)}
              disabled={loading}
              className="flex items-center gap-1 rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-50"
              title={t.history.refresh}
            >
              <RefreshCw className={`h-3 w-3 ${loading ? 'animate-spin' : ''}`} />
              <span>{t.history.refresh}</span>
            </button>
          </div>
        </div>

        {/* 历史搜索过滤栏 */}
        <div className="relative flex items-center">
          <Search className="absolute left-2.5 h-3.5 w-3.5 text-slate-400" />
          <input
            type="text"
            value={searchKeyword}
            onChange={(e) => setSearchKeyword(e.target.value)}
            placeholder={t.history.searchPlaceholder}
            className="w-full rounded border border-slate-200 bg-slate-50/50 py-1 pl-8 pr-3 text-xs text-slate-700 outline-none focus:border-indigo-500 focus:bg-white transition-colors"
          />
        </div>
      </div>

      {error && (
        <div className="mx-4 mt-3 rounded bg-rose-50 px-2 py-1 text-xs text-rose-600 border border-rose-200">
          {error}
        </div>
      )}

      <div className="flex-1 overflow-auto p-3">
        {entries.length === 0 && !loading ? (
          <div className="flex h-full flex-col items-center justify-center gap-1 text-center">
            <span className="text-sm text-slate-400">{t.history.empty}</span>
            <span className="text-[11px] text-slate-300">{t.history.emptyHint}</span>
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {entries.map((entry) => (
              <div
                key={entry.cache_key}
                className="group flex flex-col gap-1 rounded-lg border border-slate-200 bg-slate-50/60 p-2.5"
              >
                <div className="flex items-start justify-between gap-2">
                  <span className="line-clamp-2 flex-1 text-xs text-slate-500">
                    {entry.source_text}
                  </span>
                  <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                    <button
                      onClick={() => handleSpeak(entry.source_text, entry.source_lang)}
                      className="rounded p-1 text-slate-400 hover:bg-white hover:text-indigo-600"
                      title={t.history.speak}
                    >
                      <Volume2 className="h-3.5 w-3.5" />
                    </button>
                    <button
                      onClick={() => handleCopy(entry)}
                      className="rounded p-1 text-slate-400 hover:bg-white hover:text-indigo-600"
                      title={t.history.copy}
                    >
                      {copiedKey === entry.cache_key ? (
                        <Check className="h-3.5 w-3.5 text-emerald-600" />
                      ) : (
                        <Copy className="h-3.5 w-3.5" />
                      )}
                    </button>
                    <button
                      onClick={() => handleDelete(entry)}
                      className="rounded p-1 text-slate-400 hover:bg-white hover:text-rose-600"
                      title={t.history.delete}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </div>
                <div className="flex items-start justify-between gap-2">
                  <span className="line-clamp-2 flex-1 text-sm text-slate-800">
                    {entry.translated_text}
                  </span>
                  <button
                    onClick={() => handleSpeak(entry.translated_text, entry.target_lang)}
                    className="shrink-0 rounded p-1 text-slate-400 opacity-0 group-hover:opacity-100 hover:bg-white hover:text-indigo-600"
                    title={t.history.speak}
                  >
                    <Volume2 className="h-3.5 w-3.5" />
                  </button>
                </div>
                <div className="flex items-center gap-2 text-[10px] text-slate-400">
                  <span>
                    {entry.source_lang} → {entry.target_lang}
                  </span>
                  <span>·</span>
                  <span>{entry.engine}</span>
                  <span>·</span>
                  <span>{formatTime(entry.created_at)}</span>
                </div>
              </div>
            ))}

            {hasMore && (
              <button
                onClick={() => load(entries.length, searchKeyword)}
                disabled={loading}
                className="mt-1 rounded border border-slate-200 bg-white py-1.5 text-xs text-slate-600 hover:bg-slate-50 disabled:opacity-50"
              >
                {loading ? t.history.loading : t.history.loadMore}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
