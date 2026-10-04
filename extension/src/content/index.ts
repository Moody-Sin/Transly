/**
 * Transly 网页内容脚本 - 沉浸式整页原地替换（仅译文）
 */

console.log('[Transly Content Script] 沉浸式翻译引擎已就绪')

// 标记节点属性以支持一键还原原文
export const ORIGINAL_TEXT_ATTR = 'data-transly-original'

export function replaceTextNode(node: Text, translatedText: string) {
  const parent = node.parentElement
  if (!parent) return

  if (!parent.hasAttribute(ORIGINAL_TEXT_ATTR)) {
    parent.setAttribute(ORIGINAL_TEXT_ATTR, node.nodeValue || '')
  }
  node.nodeValue = translatedText
}

export function restoreOriginalText(container: HTMLElement = document.body) {
  const elements = container.querySelectorAll(`[${ORIGINAL_TEXT_ATTR}]`)
  elements.forEach((el) => {
    const original = el.getAttribute(ORIGINAL_TEXT_ATTR)
    if (original && el.firstChild && el.firstChild.nodeType === Node.TEXT_NODE) {
      el.firstChild.nodeValue = original
      el.removeAttribute(ORIGINAL_TEXT_ATTR)
    }
  })
}
