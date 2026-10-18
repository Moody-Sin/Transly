import { useState, useRef, useCallback, useEffect } from 'react'
import {
  FileText,
  UploadCloud,
  Check,
  Copy,
  Download,
  AlertCircle,
  Loader2,
  RefreshCw,
} from 'lucide-react'
import { commands } from './bindings'
import type { DocumentFormat, DocumentTranslateOutcomeDto } from './bindings'
import type { I18nSchema } from './i18n'
import { downloadBlob } from './utils/download'
import { detectDocumentFormat, generateExportFilename, formatFileSize } from './utils/document'

interface DocumentPanelProps {
  t: I18nSchema
  targetLang: string
  engine: string
}

export default function DocumentPanel({ t, targetLang, engine }: DocumentPanelProps) {
  const [file, setFile] = useState<{
    name: string
    size: number
    format: DocumentFormat
    content: string
  } | null>(null)

  const [isDragging, setIsDragging] = useState(false)
  const [isTranslating, setIsTranslating] = useState(false)
  const [outcome, setOutcome] = useState<DocumentTranslateOutcomeDto | null>(null)
  const [durationSec, setDurationSec] = useState<string | null>(null)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [previewTab, setPreviewTab] = useState<'translated' | 'original'>('translated')
  const [copied, setCopied] = useState(false)

  const fileInputRef = useRef<HTMLInputElement>(null)

  const handleFile = useCallback(
    (selectedFile: File) => {
      setErrorMsg(null)
      setOutcome(null)
      setDurationSec(null)

      const fmt = detectDocumentFormat(selectedFile.name)
      if (!fmt) {
        setErrorMsg(t.document.unsupportedFormat)
        return
      }

      if (selectedFile.size > 10 * 1024 * 1024) {
        setErrorMsg(t.document.fileTooLarge)
        return
      }

      const reader = new FileReader()
      reader.onload = () => {
        const text = typeof reader.result === 'string' ? reader.result : ''
        if (!text.trim()) {
          setErrorMsg(t.document.emptyFile)
          return
        }
        setFile({
          name: selectedFile.name,
          size: selectedFile.size,
          format: fmt,
          content: text,
        })
      }
      reader.onerror = () => {
        setErrorMsg(t.document.readFailed)
      }
      reader.readAsText(selectedFile, 'utf-8')
    },
    [t],
  )

  const onDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      e.preventDefault()
      setIsDragging(false)
      const droppedFiles = e.dataTransfer.files
      if (droppedFiles && droppedFiles.length > 0) {
        handleFile(droppedFiles[0])
      }
    },
    [handleFile],
  )

  const onDragOver = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    setIsDragging(true)
  }, [])

  const onDragLeave = useCallback((e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    setIsDragging(false)
  }, [])

  const startTranslation = useCallback(async () => {
    if (!file || isTranslating) return

    setIsTranslating(true)
    setErrorMsg(null)
    const startTime = performance.now()

    try {
      const res = await commands.translateDocument(
        file.content,
        file.format,
        null,
        targetLang,
        engine,
      )

      if (res.status === 'ok') {
        const elapsed = ((performance.now() - startTime) / 1000).toFixed(1)
        setOutcome(res.data)
        setDurationSec(elapsed)
        setPreviewTab('translated')
      } else {
        setErrorMsg(`${t.document.translateFailed}${res.error.message}`)
      }
    } catch (err) {
      setErrorMsg(
        `${t.document.translateFailed}${err instanceof Error ? err.message : String(err)}`,
      )
    } finally {
      setIsTranslating(false)
    }
  }, [file, isTranslating, targetLang, engine, t])

  // 支持 Ctrl+Enter / Cmd+Enter 快捷键触发翻译
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        if (file && !isTranslating) {
          e.preventDefault()
          void startTranslation()
        }
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [file, isTranslating, startTranslation])

  const exportTranslated = () => {
    if (!file || !outcome) return
    const filename = generateExportFilename(file.name, targetLang)
    const blob = new Blob([outcome.translated_content], {
      type: 'text/plain;charset=utf-8',
    })
    downloadBlob(blob, filename)
  }

  const copyTranslated = async () => {
    if (!outcome) return
    try {
      await navigator.clipboard.writeText(outcome.translated_content)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      setErrorMsg(t.translation.copyFailed)
    }
  }

  const resetAll = () => {
    setFile(null)
    setOutcome(null)
    setErrorMsg(null)
    setDurationSec(null)
    if (fileInputRef.current) {
      fileInputRef.current.value = ''
    }
  }

  return (
    <div className="flex flex-1 flex-col gap-4 overflow-y-auto">
      {/* 隐形文件输入 */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".txt,.md,.markdown,.srt,.json,.html,.htm"
        className="hidden"
        onChange={(e) => {
          if (e.target.files && e.target.files.length > 0) {
            handleFile(e.target.files[0])
          }
        }}
      />

      {/* 拖拽上传区域 */}
      {!file ? (
        <div
          data-testid="document-dropzone"
          onDrop={onDrop}
          onDragOver={onDragOver}
          onDragLeave={onDragLeave}
          onClick={() => fileInputRef.current?.click()}
          className={`flex flex-col items-center justify-center p-8 border-2 border-dashed rounded-xl cursor-pointer transition-colors ${
            isDragging
              ? 'border-indigo-500 bg-indigo-50/50 dark:bg-indigo-950/20'
              : 'border-slate-300 dark:border-slate-700 bg-slate-50/50 dark:bg-slate-900/50 hover:bg-slate-100/60 dark:hover:bg-slate-800/60'
          }`}
        >
          <div className="flex h-12 w-12 items-center justify-center rounded-full bg-indigo-100 dark:bg-indigo-950/60 text-indigo-600 dark:text-indigo-400 mb-3">
            <UploadCloud className="h-6 w-6" />
          </div>
          <p className="text-sm font-medium text-slate-700 dark:text-slate-200 mb-1">
            {t.document.dropHint}
          </p>
          <p className="text-xs text-slate-500 dark:text-slate-400">
            {t.document.supportedFormats}
          </p>
        </div>
      ) : (
        /* 文件信息卡片与翻译操作 */
        <div className="flex flex-col gap-3 p-4 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-lg shadow-sm">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-indigo-50 dark:bg-indigo-950/50 text-indigo-600 dark:text-indigo-400">
                <FileText className="h-5 w-5" />
              </div>
              <div>
                <h4 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
                  {file.name}
                </h4>
                <div className="flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                  <span>{formatFileSize(file.size)}</span>
                  <span>•</span>
                  <span className="uppercase font-mono px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-300">
                    {file.format}
                  </span>
                </div>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <button
                onClick={resetAll}
                disabled={isTranslating}
                className="text-xs px-2.5 py-1.5 text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-700 rounded transition-colors disabled:opacity-50"
              >
                <RefreshCw className="h-3.5 w-3.5 inline mr-1" />
                重新选择
              </button>
              <button
                data-testid="start-document-translate"
                onClick={startTranslation}
                disabled={isTranslating}
                className="flex items-center gap-1.5 px-4 py-1.5 text-xs font-medium text-white bg-indigo-600 hover:bg-indigo-700 active:bg-indigo-800 rounded shadow-sm disabled:opacity-60 transition-colors"
              >
                {isTranslating ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    <span>{t.document.translating}</span>
                  </>
                ) : (
                  <span>{t.document.btnStart}</span>
                )}
              </button>
            </div>
          </div>

          {/* 进度与统计 */}
          {outcome && (
            <div className="flex items-center justify-between pt-2 border-t border-slate-100 dark:border-slate-700/60 text-xs text-slate-600 dark:text-slate-300">
              <div className="flex items-center gap-2">
                <span className="inline-flex items-center text-emerald-600 dark:text-emerald-400 font-medium">
                  <Check className="h-3.5 w-3.5 mr-1" />
                  {t.document.progressChunks
                    .replace('{done}', String(outcome.translated_chunks))
                    .replace('{total}', String(outcome.total_chunks))
                    .replace('{hits}', String(outcome.cache_hits))}
                </span>
                {durationSec && (
                  <span className="text-slate-400">
                    • {t.document.duration.replace('{s}', durationSec)}
                  </span>
                )}
              </div>

              <div className="flex items-center gap-2">
                <button
                  data-testid="copy-document-translation"
                  onClick={copyTranslated}
                  className="flex items-center gap-1 px-2.5 py-1 rounded bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-200 transition-colors"
                >
                  {copied ? (
                    <>
                      <Check className="h-3.5 w-3.5 text-emerald-500" />
                      <span>{t.translation.copied}</span>
                    </>
                  ) : (
                    <>
                      <Copy className="h-3.5 w-3.5" />
                      <span>{t.document.copyTranslated}</span>
                    </>
                  )}
                </button>
                <button
                  data-testid="export-document-file"
                  onClick={exportTranslated}
                  className="flex items-center gap-1 px-3 py-1 rounded bg-indigo-50 dark:bg-indigo-950/60 hover:bg-indigo-100 dark:hover:bg-indigo-900/60 text-indigo-600 dark:text-indigo-400 font-medium transition-colors"
                >
                  <Download className="h-3.5 w-3.5" />
                  <span>{t.document.exportFile}</span>
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {/* 错误警告条 */}
      {errorMsg && (
        <div className="flex items-center gap-2 p-3 text-xs text-rose-600 dark:text-rose-400 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900 rounded-lg">
          <AlertCircle className="h-4 w-4 shrink-0" />
          <span>{errorMsg}</span>
        </div>
      )}

      {/* 预览区域 */}
      {file && (
        <div className="flex flex-1 flex-col min-h-0 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-lg overflow-hidden shadow-sm">
          <div className="flex items-center justify-between px-3 py-2 border-b border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800/80">
            <div className="flex items-center gap-1">
              <button
                onClick={() => setPreviewTab('translated')}
                className={`px-3 py-1 text-xs rounded font-medium transition-colors ${
                  previewTab === 'translated'
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700'
                }`}
              >
                {t.document.translatedPreview}
              </button>
              <button
                onClick={() => setPreviewTab('original')}
                className={`px-3 py-1 text-xs rounded font-medium transition-colors ${
                  previewTab === 'original'
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700'
                }`}
              >
                {t.document.originalPreview}
              </button>
            </div>
            <span className="text-xs text-slate-400 font-mono">
              {previewTab === 'translated'
                ? outcome
                  ? `${outcome.translated_content.length} 字符`
                  : '未翻译'
                : `${file.content.length} 字符`}
            </span>
          </div>

          <div className="flex-1 p-3 overflow-auto font-mono text-xs text-slate-800 dark:text-slate-200 whitespace-pre-wrap select-text leading-relaxed">
            {previewTab === 'translated'
              ? outcome?.translated_content || (
                  <span className="text-slate-400 dark:text-slate-500 italic font-sans">
                    {isTranslating ? t.document.translating : '点击上方“开始翻译”按钮生成译文'}
                  </span>
                )
              : file.content}
          </div>
        </div>
      )}
    </div>
  )
}
