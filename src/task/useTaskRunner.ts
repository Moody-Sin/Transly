import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { commands } from '../bindings'
import {
  canRetry, nextState,
  type TaskError, type TaskState,
} from './taskState'
import { applyIfCurrent } from './generation'

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

    const res = await commands.translate(text, 'auto', langRef.current, engineRef.current)
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

  // 订阅划词 / 剪贴板事件：按 request_id 去重，同一段文本只翻译一次
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<FloatRequest>('float-translate', (event) => {
      const payload = event.payload
      if (!payload?.text?.trim()) return
      if (seenFloatIdsRef.current.has(payload.request_id)) return
      seenFloatIdsRef.current.add(payload.request_id)
      run(payload.text, { source: payload.source })
    }).then((fn) => { unlisten = fn }).catch(() => {})
    return () => { if (unlisten) unlisten() }
  }, [run])

  // 组件卸载时作废全部在途代际
  useEffect(() => () => { genRef.current += 1 }, [])

  // 翻译中/错误态/已取消时粘滞浮窗（不失焦隐藏），终态恢复自动收起
  const needsSticky = snapshot.state === 'running' || snapshot.state === 'queued'
    || snapshot.state === 'cancelled' || snapshot.error !== null
  useEffect(() => {
    commands.setFloatSticky(needsSticky).catch(() => {})
  }, [needsSticky])

  return { snapshot, run, cancel, retry, reset }
}
