import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Copy,
  Check,
  Trash2,
  RefreshCw,
  Search,
  Download,
  Volume2,
  CornerUpLeft,
  X,
} from 'lucide-react'
import { commands } from './bindings'
import type { CacheEntry } from './bindings'
import type { I18nSchema, Locale } from './i18n'
import { speakText } from './utils/speech'
import { applyIfCurrent } from './task/generation'
import { formatCsvRow, guardCsvFormulaInjection } from './utils/csv'
import { downloadBlob } from './utils/download'

const PAGE_SIZE = 50

interface Props {
  t: I18nSchema
  locale: Locale
  onSelectEntry?: (entry: CacheEntry) => void
}

/** 关键词高亮展示辅助组件 */
function HighlightMatch({ text, keyword }: { text: string; keyword: string }) {
  const kw = keyword.trim()
  if (!kw) return <>{text}</>
  try {
    const escaped = kw.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    const regex = new RegExp(`(${escaped})`, 'gi')
    const parts = text.split(regex)
    return (
      <>
        {parts.map((part, i) =>
          part.toLowerCase() === kw.toLowerCase() ? (
            <mark
              key={i}
              className="rounded-sm bg-amber-200/90 dark:bg-amber-800/80 text-inherit px-0.5 font-medium"
            >
              {part}
            </mark>
          ) : (
            part
          ),
        )}
      </>
    )
  } catch {
    return <>{text}</>
  }
}

/**
 * 翻译历史面板：把已存在的 `translation_cache` 表变为用户可见的「最近翻译」。
 * 支持关键词搜索过滤、CSV/JSON 一键导出、发音朗读。
 */
export default function HistoryPanel({ t, locale, onSelectEntry }: Props) {
  const [entries, setEntries] = useState<CacheEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [hasMore, setHasMore] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [copiedKey, setCopiedKey] = useState<string | null>(null)
  const [deletingKey, setDeletingKey] = useState<string | null>(null)
  const [searchKeyword, setSearchKeyword] = useState('')
  const [exportNotice, setExportNotice] = useState<string | null>(null)
  const genRef = useRef(0)
  const deleteTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const load = useCallback(
    async (offset: number, keyword: string) => {
      const gen = ++genRef.current
      setLoading(true)
      setError(null)
      try {
        const res = keyword.trim()
          ? await commands.searchTranslationHistory(keyword.trim(), PAGE_SIZE, offset)
          : await commands.getTranslationHistory(PAGE_SIZE, offset)

        // 代际闸门：clearTimeout 挡不住在途 IPC。快速输入时后发先至会让搜索框与列表对不上，
        // 分页更会把旧关键词的结果追加进新结果集。迟到的一代一律丢弃。
        if (applyIfCurrent(gen, genRef.current, true) !== true) return

        if (res.status === 'ok') {
          setEntries((prev) => (offset === 0 ? res.data : [...prev, ...res.data]))
          setHasMore(res.data.length === PAGE_SIZE)
        } else {
          setError(res.error)
        }
      } catch (e) {
        if (applyIfCurrent(gen, genRef.current, true) !== true) return
        setError(e instanceof Error && e.message ? e.message : t.history.loadFailed)
      } finally {
        // loading 也归本代所有：旧代收尾不得熄灭新代的加载态
        if (applyIfCurrent(gen, genRef.current, true) === true) setLoading(false)
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

  // 复制/导出提示的定时器句柄：卸载时必须清理，否则组件销毁后仍会 setState
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const exportTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // 卸载时作废全部在途代际，并清掉提示与确认定时器
  useEffect(
    () => () => {
      genRef.current += 1
      if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current)
      if (exportTimerRef.current) clearTimeout(exportTimerRef.current)
      if (deleteTimerRef.current) clearTimeout(deleteTimerRef.current)
    },
    [],
  )

  const handleCopy = (entry: CacheEntry) => {
    navigator.clipboard
      .writeText(entry.translated_text)
      .then(() => {
        setCopiedKey(entry.cache_key)
        if (copiedTimerRef.current) clearTimeout(copiedTimerRef.current)
        copiedTimerRef.current = setTimeout(() => {
          copiedTimerRef.current = null
          setCopiedKey(null)
        }, 1500)
      })
      .catch((e: unknown) => {
        // 剪贴板写入失败时不得显示「已复制」
        setError(e instanceof Error && e.message ? e.message : t.history.copyFailed)
      })
  }

  const requestDelete = (entry: CacheEntry) => {
    setDeletingKey(entry.cache_key)
    if (deleteTimerRef.current) clearTimeout(deleteTimerRef.current)
    deleteTimerRef.current = setTimeout(() => {
      deleteTimerRef.current = null
      setDeletingKey(null)
    }, 3500)
  }

  const cancelDelete = () => {
    setDeletingKey(null)
    if (deleteTimerRef.current) {
      clearTimeout(deleteTimerRef.current)
      deleteTimerRef.current = null
    }
  }

  const handleDelete = async (entry: CacheEntry) => {
    cancelDelete()
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
    speakText(text, lang)
  }

  const handleExport = async (format: 'json' | 'csv') => {
    if (entries.length === 0) return
    setLoading(true)
    setError(null)
    setExportNotice(null)
    try {
      const res = await commands.exportTranslationHistory(format, searchKeyword.trim() || null)
      if (res.status === 'ok') {
        setExportNotice(`${t.history.exportSuccess}: ${res.data.file_path} (${res.data.count})`)
        if (exportTimerRef.current) clearTimeout(exportTimerRef.current)
        exportTimerRef.current = setTimeout(() => {
          exportTimerRef.current = null
          setExportNotice(null)
        }, 5000)
      } else {
        setError(`${t.history.exportFailed}: ${res.error}`)
      }
    } catch {
      // 浏览器端纯网页或桩环境兜底回退客户端直接下载
      try {
        if (format === 'json') {
          const blob = new Blob([JSON.stringify(entries, null, 2)], { type: 'application/json' })
          downloadBlob(blob, `transly-history-${new Date().toISOString().slice(0, 10)}.json`)
        } else {
          const header = [
            'Source Text',
            'Translated Text',
            'Source Lang',
            'Target Lang',
            'Engine',
            'Time',
          ]
          const cell = (raw: string) => guardCsvFormulaInjection(raw)
          const rows = entries.map((e) =>
            formatCsvRow([
              cell(e.source_text),
              cell(e.translated_text),
              cell(e.source_lang),
              cell(e.target_lang),
              cell(e.engine),
              new Date(e.created_at * 1000).toISOString(),
            ]),
          )
          const csvContent = '\uFEFF' + [header.join(','), ...rows].join('\n')
          const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' })
          downloadBlob(blob, `transly-history-${new Date().toISOString().slice(0, 10)}.csv`)
        }
      } catch (err) {
        setError(err instanceof Error && err.message ? err.message : t.history.exportFailed)
      }
    } finally {
      setLoading(false)
    }
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
                  onClick={() => handleExport('json')}
                  disabled={loading}
                  className="flex items-center gap-1 rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-50"
                  title={t.history.exportJson}
                >
                  <Download className="h-3 w-3" />
                  <span>JSON</span>
                </button>
                <button
                  onClick={() => handleExport('csv')}
                  disabled={loading}
                  className="flex items-center gap-1 rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-2 py-0.5 text-[11px] text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-50"
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
            className="w-full rounded border border-slate-200 dark:border-slate-700 bg-slate-50/50 dark:bg-slate-800 py-1 pl-8 pr-7 text-xs text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500 focus:bg-white dark:focus:bg-slate-800 transition-colors"
          />
          {searchKeyword && (
            <button
              onClick={() => setSearchKeyword('')}
              className="absolute right-2 rounded-full p-0.5 text-slate-400 hover:bg-slate-200 dark:hover:bg-slate-700 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
              title={t.history.clearSearch}
            >
              <X className="h-3 w-3" />
            </button>
          )}
        </div>
      </div>

      {exportNotice && (
        <div className="mx-4 mt-2.5 flex items-center justify-between rounded bg-emerald-50 dark:bg-emerald-950/60 px-2.5 py-1 text-xs text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
          <span className="truncate">{exportNotice}</span>
          <button
            onClick={() => setExportNotice(null)}
            className="ml-2 text-emerald-500 hover:text-emerald-700 dark:hover:text-emerald-200"
          >
            ✕
          </button>
        </div>
      )}

      {error && (
        <div className="mx-4 mt-3 rounded bg-rose-50 dark:bg-rose-950/40 px-2 py-1 text-xs text-rose-600 dark:text-rose-400 border border-rose-200 dark:border-rose-900/60">
          {error}
        </div>
      )}

      <div className="flex-1 overflow-auto p-3">
        {entries.length === 0 && !loading ? (
          <div className="flex h-full flex-col items-center justify-center gap-1 text-center">
            <span className="text-sm text-slate-400">{t.history.empty}</span>
            <span className="text-[11px] text-slate-400 dark:text-slate-500">
              {t.history.emptyHint}
            </span>
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {entries.map((entry) => (
              <div
                key={entry.cache_key}
                className="group flex flex-col gap-1 rounded-lg border border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-800/40 p-2.5"
              >
                <div className="flex items-start justify-between gap-2">
                  <span className="line-clamp-2 flex-1 text-xs text-slate-500 dark:text-slate-400">
                    <HighlightMatch text={entry.source_text} keyword={searchKeyword} />
                  </span>
                  <div
                    className={`flex shrink-0 items-center gap-1 transition-opacity ${
                      deletingKey === entry.cache_key
                        ? 'opacity-100'
                        : 'opacity-0 group-hover:opacity-100'
                    }`}
                  >
                    {onSelectEntry && (
                      <button
                        data-testid="history-use-entry"
                        onClick={() => onSelectEntry(entry)}
                        className="rounded p-1 text-slate-400 hover:bg-white dark:hover:bg-slate-700 hover:text-indigo-600 dark:hover:text-indigo-400"
                        title={t.history.useAsInput}
                      >
                        <CornerUpLeft className="h-3.5 w-3.5" />
                      </button>
                    )}
                    <button
                      onClick={() => handleSpeak(entry.source_text, entry.source_lang)}
                      className="rounded p-1 text-slate-400 hover:bg-white dark:hover:bg-slate-700 hover:text-indigo-600 dark:hover:text-indigo-400"
                      title={t.history.speak}
                    >
                      <Volume2 className="h-3.5 w-3.5" />
                    </button>
                    <button
                      onClick={() => handleCopy(entry)}
                      className="rounded p-1 text-slate-400 hover:bg-white dark:hover:bg-slate-700 hover:text-indigo-600 dark:hover:text-indigo-400"
                      title={t.history.copy}
                    >
                      {copiedKey === entry.cache_key ? (
                        <Check className="h-3.5 w-3.5 text-emerald-600" />
                      ) : (
                        <Copy className="h-3.5 w-3.5" />
                      )}
                    </button>
                    {deletingKey === entry.cache_key ? (
                      <div className="flex items-center gap-1 rounded bg-rose-50 dark:bg-rose-950/60 p-0.5 border border-rose-200 dark:border-rose-900/60">
                        <button
                          onClick={() => handleDelete(entry)}
                          className="rounded bg-rose-600 px-1.5 py-0.5 text-[10px] font-medium text-white hover:bg-rose-700 transition-colors"
                        >
                          {t.history.confirmDelete}
                        </button>
                        <button
                          onClick={cancelDelete}
                          className="rounded p-0.5 text-slate-400 hover:text-slate-600 dark:hover:text-slate-300"
                          title={t.history.cancelDelete}
                        >
                          <X className="h-3 w-3" />
                        </button>
                      </div>
                    ) : (
                      <button
                        onClick={() => requestDelete(entry)}
                        className="rounded p-1 text-slate-400 hover:bg-white dark:hover:bg-slate-700 hover:text-rose-600 dark:hover:text-rose-400"
                        title={t.history.delete}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    )}
                  </div>
                </div>
                <div className="flex items-start justify-between gap-2">
                  <span className="line-clamp-2 flex-1 text-sm text-slate-800 dark:text-slate-100">
                    <HighlightMatch text={entry.translated_text} keyword={searchKeyword} />
                  </span>
                  <button
                    onClick={() => handleSpeak(entry.translated_text, entry.target_lang)}
                    className="shrink-0 rounded p-1 text-slate-400 opacity-0 group-hover:opacity-100 hover:bg-white dark:hover:bg-slate-700 hover:text-indigo-600"
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
                className="mt-1 rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 py-1.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 disabled:opacity-50"
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
