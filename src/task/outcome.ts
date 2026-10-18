import type { TaskError } from './taskState.ts'

/** translate 命令的结构化失败形态（与 tauri-specta 生成的 TranslateErrorDto 对齐） */
export type TranslateFailure = { status: 'error'; error: TaskError }

/**
 * IPC 层抛错（而非返回 status==='error'）时回落为可重试的 internal 错误。
 * 没有这一步，UI 会永久停留在 running —— 用户既看不到原因，也无法重试。
 */
export function toFailureOutcome(err: unknown): TranslateFailure {
  return {
    status: 'error',
    error: {
      kind: 'internal',
      message: err instanceof Error ? err.message : String(err),
      retryable: true,
    },
  }
}

/**
 * 清洗打断取消时的流式残缺文本，杜绝内部代码保护占位符（形如 `⟦TRANSLY_CODE_0⟧`）残留泄漏至用户界面。
 */
export function sanitizeCancelledStreamText(text: string): string {
  if (!text) return ''
  return text.replace(/⟦\s*TRANSLY_CODE_\d+\s*⟧/g, '[代码]').replace(/⟦\s*TRANSLY_CODE_[^⟧]*$/g, '')
}
