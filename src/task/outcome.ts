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
