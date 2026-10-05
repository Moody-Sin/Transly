import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { commands } from '../bindings'
import {
  canRetry, nextState,
  type TaskError, type TaskState,
} from './taskState'
import { applyIfCurrent } from './generation'
import { toFailureOutcome } from './outcome'

export { applyIfCurrent }

export interface RunnerSnapshot {
  state: TaskState
  text: string
  engine: string | null
  error: TaskError | null
  source: 'input' | 'selection' | 'clipboard' | null
  sourceText: string
}

export interface TaskRunner {
  snapshot: RunnerSnapshot
  run: (text: string, opts?: { source?: RunnerSnapshot['source']; debounceMs?: number }) => void
  cancel: () => void
  retry: () => void
  reset: () => void
}

interface FloatRequest {
  text: string
  source: 'selection' | 'clipboard'
  request_id: number
}

const EMPTY: RunnerSnapshot = {
  state: 'idle', text: '', engine: null, error: null, source: null, sourceText: '',
}

export function useTaskRunner(engine: string, targetLang: string): TaskRunner {
  const [snapshot, setSnapshot] = useState<RunnerSnapshot>(EMPTY)

  const genRef = useRef(0)
  const lastRunRef = useRef<{ text: string; source: RunnerSnapshot['source'] } | null>(null)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const seenFloatIdsRef = useRef<Set<number>>(new Set())

  // 引擎/语言变化时立即生效，无需重建回调
  const engineRef = useRef(engine)
  const langRef = useRef(targetLang)
  engineRef.current = engine
  langRef.current = targetLang

  const clearDebounce = useCallback(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current)
      debounceRef.current = null
    }
  }, [])

  const execute = useCallback(async (text: string, source: RunnerSnapshot['source']) => {
    const gen = ++genRef.current
    lastRunRef.current = { text, source }
    setSnapshot((s) => ({ ...s, state: nextState(s.state, 'submit'), error: null }))

    // IPC 层抛错（非 status==='error'）时也必须回落为失败态，
    // 否则 UI 永久停留在 running。代际闸门同样适用：迟到异常不污染新代。
    const res = await commands.translate(text, 'auto', langRef.current, engineRef.current)
      .catch(toFailureOutcome)
    // 代际闸门：取消或新请求后，旧代响应一律丢弃，绝不覆盖新代状态
    if (applyIfCurrent(gen, genRef.current, true) !== true) return

    if (res.status === 'ok') {
      setSnapshot({
        state: nextState('running', 'resolve'),
        text: res.data.text,
        engine: res.data.engine,
        error: null,
        source,
        sourceText: text,
      })
    } else {
      setSnapshot({
        state: nextState('running', 'reject'),
        text: '',
        engine: null,
        error: { kind: res.error.kind, message: res.error.message, retryable: res.error.retryable },
        source,
        sourceText: text,
      })
    }
  }, [])

  const run = useCallback((
    text: string,
    opts?: { source?: RunnerSnapshot['source']; debounceMs?: number },
  ) => {
    if (!text.trim()) return
    // 新输入立即作废上一代：清掉待触发的防抖，并让在途响应失效
    clearDebounce()
    genRef.current += 1
    const source = opts?.source ?? 'input'
    const delay = opts?.debounceMs ?? 0
    if (delay > 0) {
      const gen = genRef.current
      debounceRef.current = setTimeout(() => {
        if (gen !== genRef.current) return
        void execute(text, source)
      }, delay)
    } else {
      void execute(text, source)
    }
  }, [clearDebounce, execute])

  const cancel = useCallback(() => {
    clearDebounce()
    // 递增代际使全部在途响应失效；已应用的译文保持（取消 ≠ 还原）
    genRef.current += 1
    setSnapshot((s) => (
      s.state === 'running' || s.state === 'queued'
        ? { ...s, state: nextState(s.state, 'cancel') }
        : s
    ))
  }, [clearDebounce])

  const retry = useCallback(() => {
    const last = lastRunRef.current
    if (!last) return
    setSnapshot((s) => (canRetry(s.state, s.error) ? { ...s } : s))
    void execute(last.text, last.source)
  }, [execute])

  const reset = useCallback(() => {
    clearDebounce()
    genRef.current += 1
    lastRunRef.current = null
    setSnapshot(EMPTY)
  }, [clearDebounce])

  // 订阅划词 / 剪贴板事件：按 request_id 去重，同一段文本只翻译一次。
  // 浮窗首次显示前页面 JS 尚未就绪，emit 的事件会永久丢失；
  // 因此宿主会暂存最近一次请求，浮窗就绪后主动拉取一次兜底。
  useEffect(() => {
    let unlisten: (() => void) | undefined
    const handle = (payload: FloatRequest | null) => {
      if (!payload?.text?.trim()) return
      if (seenFloatIdsRef.current.has(payload.request_id)) return
      seenFloatIdsRef.current.add(payload.request_id)
      run(payload.text, { source: payload.source })
    }
    listen<FloatRequest>('float-translate', (event) => handle(event.payload))
      .then((fn) => { unlisten = fn })
      .catch(() => {})
    // 兜底：拉取宿主暂存的请求（浮窗首次就绪前的 emit 会丢失）
    commands.takePendingFloatRequest().then((res) => {
      if (res.status !== 'ok' || !res.data.has_request) return
      handle({
        text: res.data.text,
        source: res.data.source as FloatRequest['source'],
        request_id: res.data.request_id,
      })
    }).catch(() => {})
    return () => { if (unlisten) unlisten() }
  }, [run])

  // 组件卸载时作废全部在途代际
  useEffect(() => () => { genRef.current += 1 }, [])

  // 翻译中/错误态/已取消时粘滞浮窗（不失焦隐藏），终态恢复自动收起。
  // 仅浮窗窗口需要：主窗口失焦不隐藏，误调会把浮窗永久钉住。
  const isFloatWindow = typeof window !== 'undefined' && window.location.hash === '#float'
  const needsSticky = isFloatWindow && (
    snapshot.state === 'running' || snapshot.state === 'queued'
    || snapshot.state === 'cancelled' || snapshot.error !== null
  )
  useEffect(() => {
    if (!isFloatWindow) return
    commands.setFloatSticky(needsSticky).catch(() => {})
  }, [needsSticky, isFloatWindow])

  return { snapshot, run, cancel, retry, reset }
}
