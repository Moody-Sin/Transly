/**
 * 瞬时设置状态：一次性写入文案，N 毫秒后自动清除。
 *
 * 为什么需要它：设置面板里「已保存 / 保存失败」这类提示有 18 处，
 * 每处原本都是 `setSaveStatus(...)` 紧跟一个裸 `setTimeout(() => setSaveStatus(null), N)`。
 * 裸定时器从不清理，连续操作时会互相竞争（早先的定时器把后来的提示提前抹掉），
 * 且组件卸载后仍会触发一次 setState。
 *
 * 这里把「清除上一个定时器」和「卸载时清理」收敛到一处。
 */
import { useCallback, useEffect, useRef, useState } from 'react'

export interface TransientMessage<T> {
  value: T
  error: boolean
  /** 写入文案；`clearAfterMs > 0` 时在到期后自动复位为 `initial` */
  set: (next: T, options?: { error?: boolean; clearAfterMs?: number }) => void
}

export function useTransientMessage<T>(initial: T): TransientMessage<T> {
  const [value, setValue] = useState<T>(initial)
  const [error, setError] = useState(false)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current)
      timerRef.current = null
    }
  }, [])

  const set = useCallback(
    (next: T, options?: { error?: boolean; clearAfterMs?: number }) => {
      clearTimer()
      setValue(next)
      setError(options?.error ?? false)
      const delay = options?.clearAfterMs
      if (delay !== undefined && delay > 0) {
        timerRef.current = setTimeout(() => {
          timerRef.current = null
          setValue(initial)
          setError(false)
        }, delay)
      }
    },
    [clearTimer, initial],
  )

  useEffect(() => clearTimer, [clearTimer])

  return { value, error, set }
}
