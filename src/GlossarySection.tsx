import { X } from 'lucide-react'
import { commands } from './bindings'
import type { I18nSchema } from './i18n'
import { formatGlossaryCsv, parseGlossaryCsv } from './utils/csv'
import { downloadBlob } from './utils/download'

export type GlossaryTerm = {
  source: string
  target: string
  match_case: boolean
}

type GlossarySectionProps = {
  t: I18nSchema
  glossaryList: GlossaryTerm[]
  glossarySearch: string
  glossarySource: string
  glossaryTarget: string
  glossaryMatchCase: boolean
  setGlossarySearch: (value: string) => void
  setGlossarySource: (value: string) => void
  setGlossaryTarget: (value: string) => void
  setGlossaryMatchCase: (value: boolean) => void
  handleAddGlossary: () => void
  handleDeleteGlossary: (source: string) => void
  loadGlossaryList: () => void
  onMessage: (message: string, isError?: boolean) => void
}

/**
 * 设置中心的术语库区块：录入、检索、CSV 导入导出与列表渲染。
 *
 * 从 `App.tsx` 拆出只为控制入口文件体积；业务状态仍由 `App` 持有，
 * 这里只负责术语库自身的展示与 CSV 文件读写。
 */
export function GlossarySection({
  t,
  glossaryList,
  glossarySearch,
  glossarySource,
  glossaryTarget,
  glossaryMatchCase,
  setGlossarySearch,
  setGlossarySource,
  setGlossaryTarget,
  setGlossaryMatchCase,
  handleAddGlossary,
  handleDeleteGlossary,
  loadGlossaryList,
  onMessage,
}: GlossarySectionProps) {
  const filtered = glossaryList.filter((item) => {
    const kw = glossarySearch.trim().toLowerCase()
    if (!kw) return true
    return item.source.toLowerCase().includes(kw) || item.target.toLowerCase().includes(kw)
  })

  return (
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
              downloadBlob(blob, `transly-glossary-${new Date().toISOString().slice(0, 10)}.csv`)
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
                  const text = typeof evt.target?.result === 'string' ? evt.target.result : ''
                  if (!text) return
                  const parsedTerms = parseGlossaryCsv(text)
                  if (parsedTerms.length === 0) {
                    onMessage(t.settings.glossaryImportDone.replace('{n}', '0'))
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
                      onMessage(res.error, true)
                    } else {
                      loadGlossaryList()
                      const imported = res.data
                      const failed = Math.max(0, parsedTerms.length - imported)
                      onMessage(
                        failed > 0
                          ? t.settings.glossaryImportPartial
                              .replace('{n}', String(imported))
                              .replace('{m}', String(failed))
                          : t.settings.glossaryImportDone.replace('{n}', String(imported)),
                        failed > 0,
                      )
                    }
                  } catch (err) {
                    onMessage(
                      err instanceof Error && err.message
                        ? err.message
                        : t.settings.statusGlossaryFailed,
                      true,
                    )
                  }
                }
                reader.onerror = () => {
                  onMessage(t.settings.statusGlossaryFailed, true)
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
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return
            if (e.key === 'Enter' && glossarySource.trim() && glossaryTarget.trim()) {
              e.preventDefault()
              handleAddGlossary()
            }
          }}
          placeholder={t.settings.glossarySourcePlaceholder}
          autoComplete="off"
          spellCheck={false}
          className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500 text-xs"
        />
        <input
          type="text"
          value={glossaryTarget}
          onChange={(e) => setGlossaryTarget(e.target.value)}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return
            if (e.key === 'Enter' && glossarySource.trim() && glossaryTarget.trim()) {
              e.preventDefault()
              handleAddGlossary()
            }
          }}
          placeholder={t.settings.glossaryTargetPlaceholder}
          autoComplete="off"
          spellCheck={false}
          className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2.5 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500 text-xs"
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
            {filtered.length === 0 ? (
              <div className="py-3 text-center text-[11px] text-slate-400">
                {t.settings.glossaryNoMatch}
              </div>
            ) : (
              filtered.map((item) => (
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
              ))
            )}
          </div>
        </div>
      )}
    </div>
  )
}
