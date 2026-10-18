export interface InputSelectionInfo {
  text: string
  isPartial: boolean
  start: number
  end: number
}

/**
 * 提取输入框（input / textarea）中待翻译的文本选区或完整文本。
 */
export function getInputSelectionInfo(
  el: HTMLInputElement | HTMLTextAreaElement,
): InputSelectionInfo | null {
  const value = el.value || ''
  if (!value.trim()) return null

  const start = el.selectionStart ?? 0
  const end = el.selectionEnd ?? 0

  if (start !== end && start < end) {
    const selected = value.substring(start, end)
    if (selected.trim()) {
      return {
        text: selected,
        isPartial: true,
        start,
        end,
      }
    }
  }

  return {
    text: value,
    isPartial: false,
    start: 0,
    end: value.length,
  }
}

/**
 * 原地替换 input 或 textarea 内容，兼容 React/Vue/Angular 受控组件原型劫持。
 */
export function replaceInputValue(
  el: HTMLInputElement | HTMLTextAreaElement,
  newText: string,
  selection?: InputSelectionInfo,
): void {
  let updatedValue = newText
  let newSelectionStart = 0
  let newSelectionEnd = newText.length

  if (selection && selection.isPartial) {
    const originalValue = el.value || ''
    const before = originalValue.substring(0, selection.start)
    const after = originalValue.substring(selection.end)
    updatedValue = before + newText + after
    newSelectionStart = selection.start
    newSelectionEnd = selection.start + newText.length
  }

  // 利用原型 setter 绕过 React 内部对 value 属性的劫持与代理
  const prototype =
    el instanceof HTMLInputElement
      ? window.HTMLInputElement.prototype
      : window.HTMLTextAreaElement.prototype
  const descriptor = Object.getOwnPropertyDescriptor(prototype, 'value')

  if (descriptor && descriptor.set) {
    descriptor.set.call(el, updatedValue)
  } else {
    el.value = updatedValue
  }

  // 触发 input 与 change 事件，确保组件状态机与 DOM 完全同步
  el.dispatchEvent(new Event('input', { bubbles: true }))
  el.dispatchEvent(new Event('change', { bubbles: true }))

  try {
    el.setSelectionRange(newSelectionStart, newSelectionEnd)
  } catch {
    // 兼容可能不支持 setSelectionRange 的特殊 input 类型
  }
}

/**
 * 智能推断输入框替换的目标语言：输入中文自动对转为英文，输入外文自动对转为中文。
 */
export function resolveInputTargetLang(text: string, preferredLang?: string): string {
  if (preferredLang && preferredLang !== 'auto') {
    return preferredLang
  }
  return /[\u4e00-\u9fa5]/.test(text) ? 'en' : 'zh-CN'
}
