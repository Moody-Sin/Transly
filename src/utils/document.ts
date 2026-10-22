import type { DocumentFormat } from '../bindings'

/**
 * 根据文件名后缀识别文档格式。
 */
export function detectDocumentFormat(filename: string): DocumentFormat | null {
  const lower = filename.trim().toLowerCase()
  if (lower.endsWith('.txt')) {
    return 'txt'
  }
  if (lower.endsWith('.md') || lower.endsWith('.markdown')) {
    return 'md'
  }
  if (lower.endsWith('.srt')) {
    return 'srt'
  }
  if (lower.endsWith('.json')) {
    return 'json'
  }
  if (lower.endsWith('.html') || lower.endsWith('.htm')) {
    return 'html'
  }
  return null
}

/**
 * 生成导出译文文件名。
 *
 * 例：
 * - `readme.md` + `zh-CN` => `readme.zh-CN.md`
 * - `movie.srt` + `en` => `movie.en.srt`
 * - `notes` + `ja` => `notes.ja.txt`
 */
export function generateExportFilename(originalName: string, targetLang: string): string {
  const clean = originalName.trim()
  const lastDot = clean.lastIndexOf('.')
  if (lastDot > 0) {
    const base = clean.slice(0, lastDot)
    const ext = clean.slice(lastDot)
    return `${base}.${targetLang}${ext}`
  }
  return `${clean}.${targetLang}.txt`
}

/**
 * 格式化文件尺寸为人类可读文本。
 */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
