import { useCallback, useEffect, useState } from 'react'
import { Copy, Check, Trash2, RefreshCw } from 'lucide-react'
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
 *
 * 该表是去重缓存（`cache_key` 为主键，重复翻译同一文本会刷新时间戳），
 * 因此展示的是「近期不重复的翻译记录」，而非逐次操作日志。
 */
export default function HistoryPanel({ t, locale }: Props) {
  const [entries, setEntries] = useState<CacheEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [hasMore, setHasMore] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [copiedKey, setCopiedKey] = useState<string | null>(null)

  const load = useCallback(
    async (offset: number) => {
      setLoading(true)
      setError(null)
      try {
        const res = await commands.getTranslationHistory(PAGE_SIZE, offset)
        if (res.status === 'ok') {
          setEntries((prev) => (offset === 0 ? res.data : [...prev, ...res.data]))
          // 返回不足一页即说明已到末尾
          setHasMore(res.data.length === PAGE_SIZE)
        } else {
          setError(res.error)
        }
      } catch (e) {
        // 失败必须可见，不得静默无反馈
        setError(e instanceof Error && e.message ? e.message : t.history.loadFailed)
      } finally {
        setLoading(false)
      }
    },
    [t],
  )

  useEffect(() => {
    load(0)
  }, [load])

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

  const formatTime = (seconds: number) => new Date(seconds * 1000).toLocaleString(locale)

  return (
    <div className="flex flex-1 flex-col overflow-hidden rounded-lg border border-slate-200 bg-white shadow-sm">
      <div className="flex items-center justify-between border-b border-slate-100 px-4 py-2.5">
        <span className="text-sm font-medium text-slate-700">{t.history.title}</span>
        <div className="flex items-center gap-2">
          {entries.length > 0 && (
            <span className="text-[11px] text-slate-400">
              {t.history.entryCount.replace('{n}', String(entries.length))}
            </span>
          )}
          <button
            onClick={() => load(0)}
            disabled={loading}
            className="flex items-center gap-1 rounded border border-slate-200 bg-slate-50 px-2 py-1 text-xs text-slate-600 hover:bg-slate-100 disabled:opacity-50"
            title={t.history.refresh}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
            <span>{t.history.refresh}</span>
          </button>
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
                <span className="line-clamp-2 text-sm text-slate-800">{entry.translated_text}</span>
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
                onClick={() => load(entries.length)}
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
