// 错误类别与任务状态均为跨层共享契约：ErrorKind 直接来自 tauri-specta 生成的绑定，
// 避免前后端枚举各自维护导致漂移。`export type` 在编译/类型剥离时完全擦除，
// 不会让 node:test 运行时去 import 依赖 @tauri-apps/api 的 bindings 模块。
import type { ErrorKind } from '../bindings'
export type { ErrorKind }
export type TaskState =
  | 'idle' | 'running' | 'succeeded' | 'failed' | 'cancelled'

export interface TaskError {
  kind: ErrorKind
  message: string
  retryable: boolean
}

const TERMINAL: ReadonlySet<TaskState> = new Set<TaskState>([
  'succeeded', 'failed', 'cancelled',
])

export function isTerminal(state: TaskState): boolean {
  return TERMINAL.has(state)
}

/** 仅失败态且错误本身可重试时提供重试入口；后端已在 classify_error 判定过一次 */
export function canRetry(state: TaskState, error: TaskError | null): boolean {
  return state === 'failed' && error?.retryable === true
}

/**
 * 重试执行闸门：hook 与 UI 共用同一判定，避免 hook 无自保护。
 * 返回 true 表示允许重新执行上一次请求。
 */
export function canRetryExecution(
  state: TaskState,
  error: TaskError | null,
  hasLastRun: boolean,
): boolean {
  return hasLastRun && canRetry(state, error)
}

export function nextState(
  current: TaskState,
  event: 'submit' | 'resolve' | 'reject' | 'cancel' | 'reset',
): TaskState {
  switch (event) {
    case 'submit':
      return 'running'
    case 'resolve':
      return 'succeeded'
    case 'reject':
      return 'failed'
    case 'cancel':
      return current === 'running' || current === 'idle'
        ? 'cancelled'
        : current
    case 'reset':
      return 'idle'
  }
}
