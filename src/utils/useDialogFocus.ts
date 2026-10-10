import { useEffect, useRef, type RefObject } from 'react'

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ')

/**
 * 模态浮层的最小无障碍契约：
 * 打开时聚焦容器、Tab / Shift+Tab 焦点循环、Escape 关闭、关闭后焦点归位到触发元素。
 *
 * 监听挂在 document 的**冒泡**阶段：快捷键录制控件在 window 捕获阶段
 * `stopPropagation`，因此录制中的按键不会再冒泡到这里被误当成「关闭模态」。
 */
export function useDialogFocus(
  isOpen: boolean,
  onClose: () => void,
): RefObject<HTMLDivElement | null> {
  const dialogRef = useRef<HTMLDivElement>(null)
  const onCloseRef = useRef(onClose)
  onCloseRef.current = onClose

  useEffect(() => {
    if (!isOpen) return
    const dialog = dialogRef.current
    if (!dialog) return

    const previouslyFocused = document.activeElement as HTMLElement | null
    dialog.focus()

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onCloseRef.current()
        return
      }
      if (event.key !== 'Tab') return

      const focusables = dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)
      if (focusables.length === 0) {
        event.preventDefault()
        return
      }
      const first = focusables[0]
      const last = focusables[focusables.length - 1]
      const active = document.activeElement
      if (event.shiftKey) {
        if (active === first || active === dialog) {
          event.preventDefault()
          last.focus()
        }
      } else if (active === last || active === dialog) {
        event.preventDefault()
        first.focus()
      }
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      previouslyFocused?.focus?.()
    }
  }, [isOpen])

  return dialogRef
}
