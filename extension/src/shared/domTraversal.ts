/**
 * DOM 深度遍历工具（light DOM + 开放模式 Shadow DOM）。
 *
 * 为什么需要这一层：`TreeWalker` 与 `querySelectorAll` 都**不会跨越 shadow 边界**。
 * 翻译侧会显式递归进入 open shadow root，若还原侧不递归，使用 Web Components
 * 的页面上「还原原文」会永久失效——shadow 内的译文一直留着且没有恢复路径。
 *
 * 这些函数刻意不依赖 `document.createTreeWalker` 或全局单例，只使用
 * Node/Element 上通用的 `nodeType` / `childNodes` / `querySelectorAll` /
 * `shadowRoot`，因此可以在无浏览器环境下用轻量 mock 直接单测。
 */

export const TEXT_NODE = 3
export const ELEMENT_NODE = 1

/**
 * 深度遍历 `root` 下的所有元素，并下探进入**开放模式**的 Shadow DOM。
 *
 * 只处理 `open` 模式：`closed` 模式的 shadowRoot 从外部不可访问，本就无从遍历。
 *
 * @param root 遍历起点；非元素节点（如 document）会被直接跳过，
 *             调用方应传入 `document.body` 或某个元素。
 */
export function* iterateElementsDeep(root: Node): Generator<Element> {
  if (root.nodeType !== ELEMENT_NODE) return
  const all = Array.from((root as Element).querySelectorAll('*'))
  for (const el of all) {
    yield el
  }
  for (const el of all) {
    const shadow = el.shadowRoot
    if (shadow && shadow.mode === 'open') {
      yield* iterateElementsDeep(shadow)
    }
  }
}

/**
 * 深度遍历 `root` 下的所有文本节点，包含开放模式 Shadow DOM 内部。
 */
export function* iterateTextNodesDeep(root: Node): Generator<Text> {
  for (const child of Array.from(root.childNodes)) {
    if (child.nodeType === TEXT_NODE) {
      yield child as Text
    } else if (child.nodeType === ELEMENT_NODE) {
      yield* iterateTextNodesDeep(child)
      const shadow = (child as Element).shadowRoot
      if (shadow && shadow.mode === 'open') {
        yield* iterateTextNodesDeep(shadow)
      }
    }
  }
}
