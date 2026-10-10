import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { commands } from '../bindings'
import { canRetryExecution, nextState, type TaskError, type TaskState } from './taskState'
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
  detectedLang?: string | null
  effectiveTargetLang?: string | null
}

export interface TaskRunner {
  snapshot: RunnerSnapshot
  run: (text: string, opts?: { source?: RunnerSnapshot['source']; debounceMs?: number }) => void
  cancel: () => void
  retry: () => void
}

interface FloatRequest {
  text: string
  source: 'selection' | 'clipboard'
  request_id: number
}

const EMPTY: RunnerSnapshot = {
  state: 'idle',
  text: '',
  engine: null,
  error: null,
  source: null,
  sourceText: '',
  detectedLang: null,
  effectiveTargetLang: null,
}

export interface UseTaskRunnerOptions {
  isPinned?: boolean
}

export function useTaskRunner(
  engine: string,
  targetLang: string,
  options?: UseTaskRunnerOptions,
): TaskRunner {
  const [snapshot, setSnapshot] = useState<RunnerSnapshot>(EMPTY)
  // 供回调读取最新快照而不重建回调（retry 需同步判定当前是否允许重试）
  const snapshotRef = useRef(snapshot)
  snapshotRef.current = snapshot

  const genRef = useRef(0)
  const lastRunRef = useRef<{ text: string; source: RunnerSnapshot['source'] } | null>(null)
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const seenFloatIdsRef = useRef<Set<number>>(new Set())

  // 引擎/语言变化时立即生效，无需重建回调
  const engineRef = useRef(engine)
  const langRef = useRef(targetLang)
  engineRef.current = engine
  langRef.current = targetLang

  // 浮窗判定：只有浮窗订阅宿主广播的 float-translate 事件，
  // 主窗口若也订阅，划词/剪贴板文本会被两个窗口各翻译一次
  const isFloatWindow = typeof window !== 'undefined' && window.location.hash === '#float'

  const clearDebounce = useCallback(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current)
      debounceRef.current = null
    }
  }, [])

  const execute = useCallback(
    async (text: string, source: RunnerSnapshot['source']) => {
      const gen = ++genRef.current
      lastRunRef.current = { text, source }
      setSnapshot((s) => ({
        ...s,
        state: nextState(s.state, 'submit'),
        text: '', // 新一轮运行重置译文
        error: null,
      }))

      // 探测源语言并在中英智能双向对转时自适应目标语言
      let effectiveTarget = langRef.current
      let detected: string | null = null
      try {
        const detectRes = await commands.detectTextLanguage(text)

        // 代际闸门必须同时覆盖「请求投递」，不能只挡响应回流。
        // detectTextLanguage 是一次 IPC 往返，其间用户可能已继续输入并触发
        // run()/cancel()：代际被作废、后端 cancelStreamTask 因任务尚未注册而
        // 是 no-op。若此处不复查就继续投递请求，后端会完整跑一次 LLM——
        // 用户看不到任何结果，但 API 已计费，且这条译文还会被写进缓存与历史。
        if (gen !== genRef.current) return

        if (detectRes.status === 'ok' && detectRes.data) {
          detected = detectRes.data
          const isTargetChinese =
            effectiveTarget === 'zh' ||
            effectiveTarget === 'zh-CN' ||
            effectiveTarget === 'zh-Hans' ||
            effectiveTarget === 'zh-TW'
          const isTargetEnglish =
            effectiveTarget === 'en' || effectiveTarget === 'en-US' || effectiveTarget === 'en-GB'

          if (isTargetChinese && detected === 'zh-CN') {
            effectiveTarget = 'en'
          } else if (isTargetEnglish && detected === 'en') {
            effectiveTarget = 'zh-CN'
          }
        }
      } catch {
        // 探测失败时静默回退默认目标语言
      }

      // 主窗口如果使用 LLM 引擎（OpenAI/DeepSeek），优先使用 translateStream 实现流式打字机效果
      const currentEng = engineRef.current
      const isLlm = currentEng === 'openai' || currentEng === 'deepseek'

      // IPC 层抛错（非 status==='error'）时也必须回落为失败态，
      // 否则 UI 永久停留在 running。代际闸门同样适用：迟到异常不污染新代。
      const reqPromise =
        isLlm && !isFloatWindow
          ? commands
              .translateStream(gen, text, 'auto', effectiveTarget, currentEng)
              .catch(toFailureOutcome)
          : commands.translate(text, 'auto', effectiveTarget, currentEng).catch(toFailureOutcome)

      const res = await reqPromise
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
          detectedLang: detected,
          effectiveTargetLang: effectiveTarget,
        })
      } else {
        setSnapshot({
          state: nextState('running', 'reject'),
          text: '',
          engine: null,
          error: {
            kind: res.error.kind,
            message: res.error.message,
            retryable: res.error.retryable,
          },
          source,
          sourceText: text,
          detectedLang: detected,
          effectiveTargetLang: effectiveTarget,
        })
      }
    },
    [isFloatWindow],
  )

  const run = useCallback(
    (text: string, opts?: { source?: RunnerSnapshot['source']; debounceMs?: number }) => {
      // 空串（如用户全选删除）同样要走完整的作废流程。
      // 旧实现在此直接 return，导致上一次按键留下的防抖定时器不会被清除、
      // 在途任务也不会被取消——输入框已清空，右侧却冒出了旧文本的译文。
      // 注意取消/清防抖必须排在空串守卫之前。
      clearDebounce()
      const oldTaskId = genRef.current
      genRef.current += 1
      commands.cancelStreamTask(oldTaskId).catch(() => {})

      if (!text.trim()) return

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
    },
    [clearDebounce, execute],
  )

  const cancel = useCallback(() => {
    clearDebounce()
    const activeTaskId = genRef.current
    // 递增代际使全部在途响应失效；已应用的译文保持（取消 ≠ 还原）
    genRef.current += 1
    // 向后端发送物理取消信号，中断底层 LLM 流式连接与 Token 消耗
    commands.cancelStreamTask(activeTaskId).catch(() => {})
    setSnapshot((s) => (s.state === 'running' ? { ...s, state: nextState(s.state, 'cancel') } : s))
  }, [clearDebounce])

  const retry = useCallback(() => {
    const last = lastRunRef.current
    // 从未发起过请求时无可重跑内容；静默返回会让按钮看起来失效，因此记录一次
    if (!last) {
      console.warn('[Transly] 重试被忽略：尚无历史请求')
      return
    }
    // 守卫：仅失败态且错误可重试时才允许重跑，与 UI 的 canRetry 判定一致
    if (!canRetryExecution(snapshotRef.current.state, snapshotRef.current.error, true)) return
    void execute(last.text, last.source)
  }, [execute])

  // 订阅流式分块事件 (translate-stream-chunk)，实现打字机式实时逐字累加
  useEffect(() => {
    let unlisten: (() => void) | undefined
    listen<{ task_id: number; chunk: string }>('translate-stream-chunk', (event) => {
      const payload = event.payload
      if (!payload || payload.task_id !== genRef.current) return
      // 代际一致时将 chunk 追加到当前 snapshot
      setSnapshot((s) => {
        if (s.state !== 'running') return s
        return {
          ...s,
          text: s.text + payload.chunk,
        }
      })
    })
      .then((fn) => {
        unlisten = fn
      })
      .catch((err) => {
        console.warn('[Transly] translate-stream-chunk 事件订阅失败：', err)
      })

    return () => {
      if (unlisten) unlisten()
    }
  }, [])

  // 订阅划词 / 剪贴板事件：按 request_id 去重，同一段文本只翻译一次。
  // 浮窗首次显示前页面 JS 尚未就绪，emit 的事件会永久丢失；
  // 因此宿主会暂存最近一次请求，浮窗就绪后主动拉取一次兜底。
  useEffect(() => {
    if (!isFloatWindow) return
    let unlisten: (() => void) | undefined
    const handle = (payload: FloatRequest | null) => {
      if (!payload?.text?.trim()) return
      if (seenFloatIdsRef.current.has(payload.request_id)) return
      seenFloatIdsRef.current.add(payload.request_id)
      if (seenFloatIdsRef.current.size > 200) {
        const kept = Array.from(seenFloatIdsRef.current).slice(-100)
        seenFloatIdsRef.current = new Set(kept)
      }
      run(payload.text, { source: payload.source })
    }
    // 注册失败必须暴露：此处空 catch 曾让 ACL 拒绝静默数月，浮窗永远空白
    // （见 Lessons Learned 第 8 条）。只降噪，不吞错。
    listen<FloatRequest>('float-translate', (event) => handle(event.payload))
      .then((fn) => {
        unlisten = fn
      })
      .catch((err) => {
        console.warn('[Transly] float-translate 事件订阅失败：', err)
      })
    // 兜底：循环排空宿主暂存的请求队列（浮窗首次就绪前的 emit 会丢失，
    // 且划词与剪贴板两条链路可能各积压一条，只拉一次会漏掉其余）
    void (async () => {
      try {
        for (;;) {
          const res = await commands.takePendingFloatRequest()
          if (res.status !== 'ok' || !res.data.has_request) break
          handle({
            text: res.data.text,
            source: res.data.source as FloatRequest['source'],
            request_id: res.data.request_id,
          })
        }
      } catch (err) {
        console.warn('[Transly] 拉取待处理浮窗请求失败：', err)
      }
    })()
    return () => {
      if (unlisten) unlisten()
    }
  }, [run, isFloatWindow])

  // 组件卸载时作废全部在途代际
  useEffect(
    () => () => {
      genRef.current += 1
    },
    [],
  )

  // 翻译中/错误态或用户手动钉住时粘滞浮窗（不失焦隐藏），终态且未钉住时恢复自动收起。
  // 仅浮窗窗口需要：主窗口失焦不隐藏，误调会把浮窗永久钉住。
  const isPinned = Boolean(options?.isPinned)
  const needsSticky =
    isFloatWindow && (isPinned || snapshot.state === 'running' || snapshot.error !== null)
  useEffect(() => {
    if (!isFloatWindow) return
    commands.setFloatSticky(needsSticky).catch((err) => {
      console.warn('[Transly] 设置浮窗粘滞失败：', err)
    })
  }, [needsSticky, isFloatWindow])

  return { snapshot, run, cancel, retry }
}
