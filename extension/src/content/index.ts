/**
 * Transly 网页内容脚本 - 沉浸式整页原地替换（仅译文）
 *
 * 注：本文件中 `chrome.runtime.sendMessage(...).catch(() => {})` 的写法专用于
 * **广播类**消息（进度、终态）。Popup 关闭或 Service Worker 休眠时无接收方，
 * reject 属正常路径而非故障，因此静默；需要确认投递结果的请求
 * （如 `TRANSLATE_BATCH`）一律走 `sendBatch` 并处理失败。
 */

console.log('[Transly Content Script] 沉浸式翻译引擎已就绪')

import {
  iterateElementsDeep,
  iterateTextNodesDeep,
  ELEMENT_NODE,
  DOCUMENT_FRAGMENT_NODE,
} from '../shared/domTraversal'

// 使用 WeakMap 严格基于 Text 节点维护原始内容，杜绝父节点多 Text 节点互相覆盖
const originalTextMap = new WeakMap<Text, string>()
const translatedTextSet = new WeakSet<Text>()

// 需要忽略的非正文/代码标签
const IGNORED_TAGS = new Set([
  'SCRIPT',
  'STYLE',
  'NOSCRIPT',
  'TEXTAREA',
  'INPUT',
  'CODE',
  'PRE',
  'KBD',
  'SAMP',
  'SVG',
])

/**
 * 叶子富文本容器选择器：这些块级元素内部常含 <a>/<strong>/<code> 等行内子标签，
 * 需要走「占位符聚合 → 整句翻译 → 原位回填」的富文本路径，而非逐文本节点切分。
 */
const LEAF_RICH_TEXT_SELECTOR = 'p, h1, h2, h3, h4, h5, h6, li, blockquote, td, th'

export function shouldTranslateElement(element: Element): boolean {
  // tagName 大小写取决于命名空间：HTML 元素返回大写，SVG 等返回小写，
  // 归一化后比较，否则 IGNORED_TAGS 中的 'SVG' 永远不会命中。
  if (IGNORED_TAGS.has(element.tagName.toUpperCase())) return false
  if (element.hasAttribute('data-no-translate')) return false
  if (element.classList.contains('notranslate')) return false

  // 沿祖先链深度检查是否有屏蔽标签或属性，防止嵌套在屏蔽区域内的文本节点被误翻；
  // 遵循通用安全约定，默认跳过 contenteditable 区域以及 Monaco/CodeMirror 专用编辑器容器。
  // svg 必须在此列出：SVG 命名空间下的 tagName 为小写（text / title / tspan），
  // 单靠 IGNORED_TAGS 拦不住图表、地图标注等被翻译后破版的情况。
  if (
    element.closest(
      '[data-no-translate], .notranslate, pre, code, textarea, input, script, style, svg, [contenteditable]:not([contenteditable="false"]), .monaco-editor, .cm-editor',
    )
  ) {
    return false
  }

  return true
}

export function replaceTextNode(node: Text, translatedText: string, mode: string = 'replace') {
  if (!originalTextMap.has(node)) {
    originalTextMap.set(node, node.nodeValue || '')
  }
  translatedTextSet.add(node)

  if (mode === 'bilingual') {
    // 双语对照模式：保留原文节点不变，紧随其后追加译文兄弟节点（若已存在则原地更新，避免重试或动态变更时重复插入）
    const parent = node.parentElement
    if (parent) {
      const next = node.nextSibling
      if (
        next &&
        next.nodeType === ELEMENT_NODE &&
        (next as Element).classList.contains('transly-bilingual-trans')
      ) {
        next.textContent = ` (${translatedText})`
      } else {
        const transEl = document.createElement('span')
        transEl.className = 'transly-bilingual-trans'
        transEl.setAttribute('data-no-translate', 'true')
        transEl.style.display = 'inline-block'
        transEl.style.marginLeft = '4px'
        transEl.style.color = '#4f46e5'
        transEl.style.opacity = '0.9'
        transEl.textContent = ` (${translatedText})`

        // 插入到当前 textNode 之后
        if (next) {
          parent.insertBefore(transEl, next)
        } else {
          parent.appendChild(transEl)
        }
      }
    }
  } else {
    // 仅译文模式：原地替换文本内容
    node.nodeValue = translatedText
  }

  // 若父节点是元素，挂载 data-transly-original 标记（若尚未挂载或为空）
  // 方便快捷悬浮查看原文（按下 Alt 键悬停查看）
  const parent = node.parentElement
  if (parent && !parent.hasAttribute('data-transly-has-translation')) {
    parent.setAttribute('data-transly-has-translation', 'true')
  }
}

export function restoreOriginalText(container: Node = document.body) {
  // 1. 深度遍历元素：单趟同时移除双语译文节点与清除父标记（避免两次全量 DOM 扫描）
  for (const el of iterateElementsDeep(container)) {
    if (el.classList.contains('transly-bilingual-trans')) {
      el.remove()
    }
    if (el.hasAttribute('data-transly-has-translation')) {
      el.removeAttribute('data-transly-has-translation')
    }
  }

  // 2. 还原文本节点内容
  for (const textNode of iterateTextNodesDeep(container)) {
    const orig = originalTextMap.get(textNode)
    if (orig !== undefined) {
      textNode.nodeValue = orig
      originalTextMap.delete(textNode)
      translatedTextSet.delete(textNode)
    }
  }

  hideOriginalTooltip()
}

// ==========================================
// 悬浮查看原文 Tooltip 交互（Shadow DOM 隔离）
// 按住 Alt 键（或直接悬停已翻译段落）时，在鼠标附近浮现对应节点的原文，
// 既维持整页翻译仅译文的纯净版面，又提供随时对照原文的能力。
// ==========================================
const TOOLTIP_HOST_ID = 'transly-original-tooltip-host'
let tooltipHost: HTMLElement | null = null
let tooltipShadow: ShadowRoot | null = null
let tooltipContainer: HTMLElement | null = null

function ensureTooltipHost(): HTMLElement {
  if (tooltipHost && tooltipHost.isConnected) return tooltipHost
  tooltipHost = document.createElement('div')
  tooltipHost.id = TOOLTIP_HOST_ID
  tooltipHost.setAttribute('data-no-translate', 'true')
  tooltipHost.style.cssText =
    'position: fixed; z-index: 2147483647; top: 0; left: 0; width: 0; height: 0; pointer-events: none;'
  tooltipShadow = tooltipHost.attachShadow({ mode: 'closed' })

  const style = document.createElement('style')
  style.textContent = `
    :host { all: initial; }
    .tooltip {
      position: fixed;
      display: none;
      max-width: 480px;
      max-height: 280px;
      overflow-y: auto;
      padding: 8px 12px;
      background: #1e293b;
      color: #f8fafc;
      font-family: system-ui, -apple-system, sans-serif;
      font-size: 13px;
      line-height: 1.5;
      border-radius: 8px;
      box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.3);
      pointer-events: none;
      white-space: pre-wrap;
      word-break: break-word;
      transition: opacity 0.15s ease;
      opacity: 0;
      border: 1px solid #334155;
    }
    .tooltip.visible {
      display: block;
      opacity: 1;
    }
    .badge {
      display: inline-block;
      margin-bottom: 4px;
      font-size: 10px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: #94a3b8;
    }
  `
  tooltipContainer = document.createElement('div')
  tooltipContainer.className = 'tooltip'
  tooltipShadow.appendChild(style)
  tooltipShadow.appendChild(tooltipContainer)

  document.documentElement.appendChild(tooltipHost)
  return tooltipHost
}

export function hideOriginalTooltip() {
  if (tooltipContainer) {
    tooltipContainer.classList.remove('visible')
  }
}

function showOriginalTooltip(originalText: string, clientX: number, clientY: number) {
  if (!originalText.trim()) return
  ensureTooltipHost()
  if (!tooltipContainer) return

  // 用 DOM API 而非 innerHTML 赋值：译文/原文属不可信输入，
  // 字符串拼接一旦被误用到属性上下文即构成注入面；纯文本节点天然免疫。
  tooltipContainer.textContent = ''
  const badge = document.createElement('div')
  badge.className = 'badge'
  badge.textContent = t('tooltipOriginalBadge')
  const body = document.createElement('div')
  body.textContent = originalText
  tooltipContainer.appendChild(badge)
  tooltipContainer.appendChild(body)
  tooltipContainer.classList.add('visible')

  const pad = 12
  const rect = tooltipContainer.getBoundingClientRect()
  let left = clientX + 16
  let top = clientY + 16

  if (left + rect.width > window.innerWidth - pad) {
    left = Math.max(pad, clientX - rect.width - 16)
  }
  if (top + rect.height > window.innerHeight - pad) {
    top = Math.max(pad, clientY - rect.height - 16)
  }

  tooltipContainer.style.left = `${left}px`
  tooltipContainer.style.top = `${top}px`
}

// 监听鼠标在网页上的移动，当目标为已翻译节点且用户悬停时显示完整段落原文 Tooltip
document.addEventListener('mouseover', (e) => {
  const target = e.target
  if (!(target instanceof Element)) return
  const translatedEl = target.closest('[data-transly-has-translation]')
  if (!translatedEl) return
  // 若该元素的祖先也是已翻译富文本段落（如 <p> 内含 <a>），优先以最外层已翻译段落为单位提取完整句子原文
  const container =
    translatedEl.parentElement?.closest('[data-transly-has-translation]') || translatedEl

  let origText = ''
  let hasTranslatedNode = false
  for (const textNode of iterateTextNodesDeep(container)) {
    if (textNode.parentElement?.classList.contains('transly-bilingual-trans')) continue
    if (originalTextMap.has(textNode)) {
      hasTranslatedNode = true
      origText += originalTextMap.get(textNode) || ''
    } else {
      origText += textNode.nodeValue || ''
    }
  }

  if (hasTranslatedNode && origText.trim()) {
    showOriginalTooltip(origText.trim(), e.clientX, e.clientY)
  }
})

document.addEventListener('mouseout', (e) => {
  const target = e.target
  if (!(target instanceof Element)) return
  const container = target.closest('[data-transly-has-translation]')
  if (!container) return
  const related = e.relatedTarget
  if (related instanceof Node && container.contains(related)) return
  hideOriginalTooltip()
})

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    hideOriginalTooltip()
  }
})

export interface TextNodeEntry {
  id: string
  node: Text
  text: string
  inViewport?: boolean
  compositeInfo?: {
    parts: Array<{ id: string; node: Text }>
    baseIds: (string | null)[]
    placeholderToId: Map<string, string>
  }
}

export function isElementInViewport(el: Element, cache?: WeakMap<Element, boolean>): boolean {
  if (cache && cache.has(el)) {
    return cache.get(el)!
  }
  try {
    const rect = el.getBoundingClientRect()
    const inVp =
      rect.bottom > 0 &&
      rect.top < (window.innerHeight || document.documentElement.clientHeight) &&
      rect.right > 0 &&
      rect.left < (window.innerWidth || document.documentElement.clientWidth) &&
      rect.width > 0 &&
      rect.height > 0
    if (cache) {
      cache.set(el, inVp)
    }
    return inVp
  } catch {
    return false
  }
}

export function isNodeInViewport(node: Text, cache?: WeakMap<Element, boolean>): boolean {
  const parent = node.parentElement
  if (!parent) return false
  return isElementInViewport(parent, cache)
}

// 维护失败文本节点映射，供局部重试与状态报告。
const failedItemsMap = new Map<string, TextNodeEntry & { errorKind?: string }>()
const MAX_FAILED_ITEMS = 2_000
let nodeSequenceId = 0

/** 允许出现在叶子富文本段落内部的合法行内标签白名单 */
const INLINE_TAGS = new Set([
  'A',
  'SPAN',
  'STRONG',
  'EM',
  'B',
  'I',
  'CODE',
  'MARK',
  'SMALL',
  'SUB',
  'SUP',
  'U',
  'S',
  'ABBR',
  'CITE',
  'TIME',
])

/**
 * 收集叶子富文本容器内的合法行内子孙元素（例如 <p>、<li> 等段落，仅包含行内标签子节点）。
 * 若非合法的叶子富文本容器则返回 null；单次扫描即可供调用方复用，消除二次 querySelectorAll('*')。
 */
export function getLeafRichTextDescendants(el: Element): Element[] | null {
  const tag = el.tagName.toUpperCase()
  if (!['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'LI', 'BLOCKQUOTE', 'TD', 'TH'].includes(tag)) {
    return null
  }
  if (el.children.length === 0) return null

  // 必须检查全部子孙元素：<p><strong><a>link</a></strong></p> 的一级子元素只有
  // <strong>，只看 el.children 会把嵌套行内结构误判为叶子段落
  const descendants = Array.from(el.querySelectorAll('*'))
  for (const descendant of descendants) {
    if (!INLINE_TAGS.has(descendant.tagName.toUpperCase())) return null
    if (descendant.hasAttribute('data-no-translate')) return null
    if (descendant.classList.contains('notranslate')) return null
    if ((descendant as HTMLElement).isContentEditable) return null
  }
  return descendants
}

export function isLeafRichTextContainer(el: Element): boolean {
  return getLeafRichTextDescendants(el) !== null
}

function rememberFailed(entry: TextNodeEntry & { errorKind?: string }) {
  failedItemsMap.delete(entry.id)
  failedItemsMap.set(entry.id, entry)
  while (failedItemsMap.size > MAX_FAILED_ITEMS) {
    const oldest = failedItemsMap.keys().next().value
    if (oldest === undefined) break
    failedItemsMap.delete(oldest)
  }
}

/**
 * 递归收集容器内所有有效的正文文本节点，支持叶子富文本段落占位聚合
 */
export function collectTextNodes(
  root: Node = document.body,
  viewportCache?: WeakMap<Element, boolean>,
): TextNodeEntry[] {
  const vpCache = viewportCache || new WeakMap<Element, boolean>()
  const entries: TextNodeEntry[] = []

  // 如果 root 自身即是文本节点且尚未翻译
  if (root.nodeType === Node.TEXT_NODE) {
    const textNode = root as Text
    const parent = textNode.parentElement
    if (parent && shouldTranslateElement(parent) && !translatedTextSet.has(textNode)) {
      const text = textNode.nodeValue?.trim()
      if (text && text.length >= 2 && !/^[\d\s.,/#!$%^&*;:{}=\-_`~()]+$/.test(text)) {
        nodeSequenceId += 1
        return [
          {
            id: `node_${nodeSequenceId}`,
            node: textNode,
            text: textNode.nodeValue || '',
            inViewport: isNodeInViewport(textNode, vpCache),
          },
        ]
      }
    }
    return []
  }

  // 1. 先检测并收集叶子富文本容器（例如段落 <p>、<li> 等内部含行内 <a>、<span>、<strong> 的节点）
  const processedContainers = new WeakSet<Element>()
  const processedNodes = new WeakSet<Text>()

  if (root.nodeType === ELEMENT_NODE || root.nodeType === DOCUMENT_FRAGMENT_NODE) {
    const rootEl = root as Element | DocumentFragment
    // `querySelectorAll` 只检索**子孙**节点，永不包含元素自身。
    // SPA（React/Vue）增量插入的往往就是一个孤立的 <p> / <li>，
    // 若只查子孙，这类根候选会整段漏掉富文本聚合，被 TreeWalker 砸成
    // 碎文本节点各自独立翻译，语序与排版随之破损。因此必须显式补上根自身。
    const leafCandidates: Element[] = []
    if (root.nodeType === ELEMENT_NODE && (root as Element).matches(LEAF_RICH_TEXT_SELECTOR)) {
      leafCandidates.push(root as Element)
    }
    leafCandidates.push(...Array.from(rootEl.querySelectorAll(LEAF_RICH_TEXT_SELECTOR)))
    leafCandidates.forEach((candidate) => {
      if (!shouldTranslateElement(candidate)) return
      const descendants = getLeafRichTextDescendants(candidate)
      if (!descendants || descendants.length === 0) return

      // 提取容器内所有的连续 text parts
      const rawParts: Array<{ node: Text; text: string; isInline: boolean }> = []
      let validTextLen = 0

      for (const child of Array.from(candidate.childNodes)) {
        if (child.nodeType === Node.TEXT_NODE) {
          const tNode = child as Text
          if (!translatedTextSet.has(tNode)) {
            const val = tNode.nodeValue || ''
            rawParts.push({ node: tNode, text: val, isInline: false })
            validTextLen += val.trim().length
          }
        } else if (child.nodeType === Node.ELEMENT_NODE) {
          const el = child as Element
          // 行内元素取其子树内所有未翻译文本节点（中序深度遍历）：
          // <strong><a>text</a></strong> 这类嵌套结构下，只遍历一级 childNodes
          // 拿到的是 <a> 元素而非 Text，深层文本会被整段漏掉
          const inlineWalker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null)
          let inlineNode = inlineWalker.nextNode()
          while (inlineNode) {
            const tNode = inlineNode as Text
            if (!translatedTextSet.has(tNode)) {
              const val = tNode.nodeValue || ''
              rawParts.push({ node: tNode, text: val, isInline: true })
              validTextLen += val.trim().length
            }
            inlineNode = inlineWalker.nextNode()
          }
        }
      }

      // 如果包含行内文本且存在有效内容，组装为整句富文本复合项
      const hasInline = rawParts.some((p) => p.isInline)
      if (hasInline && rawParts.length >= 2 && validTextLen >= 2) {
        processedContainers.add(candidate)
        // 子孙元素全部登记，防止主 TreeWalker 再次扫到嵌套行内子树里的文本节点
        for (const descendant of descendants) {
          processedContainers.add(descendant)
        }
        for (const p of rawParts) {
          processedNodes.add(p.node)
        }

        nodeSequenceId += 1
        const compId = `node_${nodeSequenceId}`
        const textParts: TextPart[] = rawParts.map((p, idx) => ({
          id: `${compId}_p${idx}`,
          text: p.text,
          isInline: p.isInline,
        }))

        const composite = buildRichTextComposite(textParts, true)
        const baseIds = composite.baseSlots || []
        const placeholderToId = new Map<string, string>()
        for (const [id, ph] of composite.idToPlaceholder) {
          placeholderToId.set(ph, id)
        }

        entries.push({
          id: compId,
          node: rawParts[0].node,
          text: composite.compositeText,
          inViewport: isElementInViewport(candidate, vpCache),
          compositeInfo: {
            parts: rawParts.map((p, idx) => ({ id: `${compId}_p${idx}`, node: p.node })),
            baseIds,
            placeholderToId,
          },
        })
      }
    })
  }

  // 2. 遍历其余常规 Text 节点
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement
      if (!parent) return NodeFilter.FILTER_REJECT
      if (!shouldTranslateElement(parent)) return NodeFilter.FILTER_REJECT
      if (translatedTextSet.has(node as Text)) return NodeFilter.FILTER_REJECT
      if (processedNodes.has(node as Text) || processedContainers.has(parent)) {
        return NodeFilter.FILTER_REJECT
      }

      const text = node.nodeValue?.trim()
      if (!text || text.length < 2) return NodeFilter.FILTER_SKIP
      // 纯数字或仅符号标点跳过
      if (/^[\d\s.,/#!$%^&*;:{}=\-_`~()]+$/.test(text)) return NodeFilter.FILTER_SKIP

      return NodeFilter.FILTER_ACCEPT
    },
  })

  let currentNode = walker.nextNode()
  while (currentNode) {
    nodeSequenceId += 1
    const textNode = currentNode as Text
    entries.push({
      id: `node_${nodeSequenceId}`,
      node: textNode,
      text: textNode.nodeValue || '',
      inViewport: isNodeInViewport(textNode, vpCache),
    })
    currentNode = walker.nextNode()
  }

  // 3. 递归穿透开放模式 Shadow DOM（Web Components 现代前端组件树，采用流式 TreeWalker 消除 querySelectorAll('*') 大数组分配）
  if (root.nodeType === ELEMENT_NODE || root.nodeType === DOCUMENT_FRAGMENT_NODE) {
    const elWalker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, {
      acceptNode(elNode) {
        const elem = elNode as Element
        if (!shouldTranslateElement(elem)) {
          return NodeFilter.FILTER_REJECT
        }
        if (elem.shadowRoot && elem.shadowRoot.mode === 'open') {
          return NodeFilter.FILTER_ACCEPT
        }
        return NodeFilter.FILTER_SKIP
      },
    })

    let shadowHost = elWalker.nextNode()
    while (shadowHost) {
      const subEl = shadowHost as Element
      if (subEl.shadowRoot) {
        entries.push(...collectTextNodes(subEl.shadowRoot, vpCache))
      }
      shadowHost = elWalker.nextNode()
    }
  }

  return entries
}

let isObserving = false
let currentObserver: MutationObserver | null = null
let observerDebounceTimer: number | null = null
let observerFirstPendingAt = 0
const observerPendingNodes = new Set<Node>()

/** 增量翻译的静默期：最后一次 DOM 变更后等待这么久就 flush。 */
const OBSERVER_DEBOUNCE_MS = 500
/** 增量翻译的最大等待：无论变更多频繁，最多这么久也必须 flush 一次。 */
const OBSERVER_MAX_WAIT_MS = 2000

/**
 * 节点是否仍挂在文档树上。
 *
 * 不能直接用 `document.body.contains(node)`：对 Shadow DOM 内部的节点它恒为
 * false，会让增量监听路径永远跳过 shadow 内新增的内容。
 */
function isNodeAttached(node: Node): boolean {
  if (node.isConnected) return true
  const el = node.nodeType === ELEMENT_NODE ? (node as Element) : node.parentElement
  if (!el) return false
  let current: Element | null = el
  while (current) {
    const root = current.getRootNode()
    if (root instanceof ShadowRoot) {
      if (root.host.isConnected) return true
      current = root.host.parentElement
    } else {
      return root === document
    }
  }
  return false
}

let currentTargetLang = 'zh-CN'
let currentEngine = 'google'

// 任务版本号协调机制：杜绝请求跨越“还原原文”或“重新翻译”边界时写回污染
let currentGeneration = 0
let currentTaskEntries: TextNodeEntry[] = []
let currentTaskTotal = 0
let currentTaskCompleted = 0

/**
 * 启动动态增量监听（针对 SPA 单页或瀑布流懒加载内容，监听 childList 与 characterData）
 *
 * 代际由模块级 `currentGeneration` 统一裁决，不接收调用方传入的 `generation`：
 * 该监听是单例（重复调用直接 return），传入的旧代际会与后续代际不匹配，
 * 导致监听器永久不再收集新节点。
 */
export function startDynamicObserver(targetLang: string = 'zh-CN', engine: string = 'google') {
  currentTargetLang = targetLang
  currentEngine = engine

  // 若已在监听，更新语言和引擎即可
  if (isObserving && currentObserver) return
  isObserving = true

  observerPendingNodes.clear()
  observerFirstPendingAt = 0
  if (observerDebounceTimer) {
    clearTimeout(observerDebounceTimer)
    observerDebounceTimer = null
  }

  currentObserver = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === 'childList') {
        mutation.addedNodes.forEach((node) => {
          if (node.nodeType === Node.ELEMENT_NODE || node.nodeType === Node.TEXT_NODE) {
            observerPendingNodes.add(node)
          }
        })
      } else if (mutation.type === 'characterData') {
        if (mutation.target.nodeType === Node.TEXT_NODE) {
          const textNode = mutation.target as Text
          if (!translatedTextSet.has(textNode)) {
            observerPendingNodes.add(textNode)
          }
        }
      }
    }

    if (observerPendingNodes.size > 0) {
      // 防抖 + 最大等待兜底。
      //
      // 纯防抖在「持续有 DOM 变更」的页面（实时聊天、比分跳动、动画计数器、
      // 高频轮询组件）上会被无限重置，回调**永不执行**，SPA 增量翻译彻底停摆。
      // 因此无论变更多频繁，最多间隔 MAX_WAIT 也必须 flush 一次。
      const now = Date.now()
      if (observerFirstPendingAt === 0) {
        observerFirstPendingAt = now
      }
      const elapsed = now - observerFirstPendingAt
      const delay = elapsed >= OBSERVER_MAX_WAIT_MS ? 0 : OBSERVER_DEBOUNCE_MS

      if (observerDebounceTimer) clearTimeout(observerDebounceTimer)
      observerDebounceTimer = window.setTimeout(async () => {
        const generation = currentGeneration
        const nodesToProcess = Array.from(observerPendingNodes)
        observerPendingNodes.clear()
        observerDebounceTimer = null
        observerFirstPendingAt = 0

        // 增量批次是独立任务：重置计数基准，否则进度百分比与失败判定会被
        // 首屏任务的累计值污染（total 只增不减 → 永不收敛；
        // failedItemsMap 不清空 → 后续批次被误判为“部分失败”）。
        const batchEntries: TextNodeEntry[] = []
        for (const root of nodesToProcess) {
          if (generation !== currentGeneration) break
          if (isNodeAttached(root)) {
            batchEntries.push(...collectTextNodes(root))
          }
        }
        if (batchEntries.length === 0 || generation !== currentGeneration) return

        // 增量批次是独立任务：用当前批次整体替换而不是追加。
        // 追加会让无限滚动页面持续增长该数组，并强引用已脱离文档的 Text 节点。
        // 注意：不得清空 failedItemsMap，否则先前全页或上一批次失败的节点会被全部抹除。
        currentTaskEntries = batchEntries
        currentTaskTotal = batchEntries.length
        currentTaskCompleted = 0

        const failedBatches = await translateEntries(
          batchEntries,
          currentTargetLang,
          currentEngine,
          generation,
        )
        if (generation === currentGeneration) {
          broadcastTerminalState(generation, failedBatches)
        }
      }, delay)
    }
  })

  currentObserver.observe(document.body, {
    childList: true,
    subtree: true,
    characterData: true,
  })
}

export function stopDynamicObserver() {
  if (currentObserver) {
    currentObserver.disconnect()
    currentObserver = null
  }
  if (observerDebounceTimer) {
    clearTimeout(observerDebounceTimer)
    observerDebounceTimer = null
  }
  observerPendingNodes.clear()
  observerFirstPendingAt = 0
  isObserving = false
}

interface BatchResponsePayload {
  translations?: string[]
  items?: Array<{
    status: 'success' | 'failed'
    id: string
    text?: string
    error_kind?: string
    message?: string
  }>
}

import { planBatchRanges, splitByViewport, type BatchLimits } from '../shared/batchPlan'
import {
  buildRichTextComposite,
  restoreRichTextComposite,
  cleanCompositeText,
  type TextPart,
} from '../shared/richText'
import { t } from '../i18n'

/**
 * 当前任务使用的批量上限。
 *
 * 由任务入口向 background 索取（background 再从服务端 `/health` 读），
 * 不在本文件写死数字：写死的项数上限与服务端的字符总量上限量纲不同，
 * 会造成整批 413 而页面静默无译文。
 */
let currentLimits: BatchLimits = {
  maxBatchItems: 30,
  maxBatchTotalChars: 50_000,
  maxItemChars: 10_000,
}

async function refreshBatchLimits(): Promise<void> {
  try {
    const res = await chrome.runtime.sendMessage({ type: 'GET_BATCH_LIMITS' })
    if (res?.ok && res.limits) {
      currentLimits = res.limits as BatchLimits
    }
  } catch {
    // 取不到时沿用上一次的（或保守默认值），不阻断翻译
  }
}

/** 发送一批并返回服务端响应 */
async function sendBatch(
  items: Array<{ id: string; text: string }>,
  targetLang: string,
  engine: string,
): Promise<{ ok: boolean; data?: BatchResponsePayload; error?: string; status?: number }> {
  return new Promise((resolve) => {
    try {
      chrome.runtime.sendMessage(
        {
          type: 'TRANSLATE_BATCH',
          items,
          source_lang: 'auto',
          target_lang: targetLang,
          engine,
        },
        (res) => {
          if (chrome.runtime.lastError || !res) {
            resolve({
              ok: false,
              error: chrome.runtime.lastError?.message || '扩展后台通信中断或未响应',
              status: 503,
            })
          } else {
            resolve(res)
          }
        },
      )
    } catch (err) {
      resolve({
        ok: false,
        error: err instanceof Error ? err.message : String(err),
        status: 503,
      })
    }
  })
}

/**
 * 服务端因批次超限拒绝（413）时，把该批对半拆开重试。
 *
 * 这是上限发现失败时的兜底：即使本端持有的上限已过期，也能自适应收敛到服务端接受的粒度。
 */
function splitEntries(entries: TextNodeEntry[]): [TextNodeEntry[], TextNodeEntry[]] | null {
  if (entries.length < 2) return null
  const mid = Math.ceil(entries.length / 2)
  return [entries.slice(0, mid), entries.slice(mid)]
}

/**
 * 发送一批并在必要时递归对半拆分。
 *
 * 服务端以 413 拒绝时说明本端持有的上限已过期（如服务端升级后收紧）。
 * 对半拆分能自适应收敛到服务端接受的粒度，而不必要求两端同时升级。
 * 拆到单项仍被拒即停止，避免无限递归。
 */
/** 瞬态可重试状态码：并发令牌耗尽（429）与服务端内部错误。 */
const TRANSIENT_RETRY_STATUSES = new Set([429, 500, 502, 503])
/** 瞬态重试的退避序列（毫秒），最多重试 2 次。 */
const RETRY_BACKOFF_MS = [300, 900]
const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

async function sendBatchWithSplit(
  batch: TextNodeEntry[],
  targetLang: string,
  engine: string,
  expectedGen: number,
): Promise<{ failedBatches: number }> {
  if (batch.length === 0) return { failedBatches: 0 }

  let response = await sendBatch(
    batch.map((item) => ({ id: item.id, text: item.text })),
    targetLang,
    engine,
  )

  // 瞬态故障退避重试：服务端并发信号量打满会返回 429，内部故障返回 500。
  // 不重试意味着一次瞬时限流就永久损失整批（最多 30 个节点）的译文，
  // 用户只能手动点「重试失败项」。Rust 侧对同一类状态码已有指数退避先例。
  // 401/403/400 与 413 不重试：前者是配置问题，后者已有专门的拆批分支。
  for (let attempt = 0; attempt < RETRY_BACKOFF_MS.length; attempt += 1) {
    const status = response.status
    if (status === undefined || !TRANSIENT_RETRY_STATUSES.has(status)) break
    // 代际已变更（用户取消或重新翻译）时立即放弃，避免在废弃批次上空转
    if (expectedGen !== currentGeneration) return { failedBatches: 0 }
    await sleep(RETRY_BACKOFF_MS[attempt])
    if (expectedGen !== currentGeneration) return { failedBatches: 0 }
    response = await sendBatch(
      batch.map((item) => ({ id: item.id, text: item.text })),
      targetLang,
      engine,
    )
  }

  if (response.ok && response.data) {
    const { succeeded } = applyBatchResponse(response.data, batch, expectedGen)
    return { failedBatches: succeeded === 0 && batch.length > 0 ? 1 : 0 }
  }

  // 413：批次超限，对半拆开重试
  if (response.status === 413) {
    const halves = splitEntries(batch)
    if (halves) {
      let failed = 0
      for (const half of halves) {
        const sub = await sendBatchWithSplit(half, targetLang, engine, expectedGen)
        failed += sub.failedBatches
      }
      return { failedBatches: failed }
    }
    // 单项仍超限：该项自身过长，交服务端逐项报错
    if (expectedGen === currentGeneration) markBatchFailed(batch, 'payload_too_large')
    return { failedBatches: 1 }
  }

  // 按状态码区分失败性质：服务端有四种语义完全不同的 400，统一归为 'network'
  // 会把「语言对不支持」「引擎名非法」这类配置问题误导成网络不可达，
  // 让用户去查网络而查不到真因。
  if (expectedGen === currentGeneration) {
    markBatchFailed(batch, classifyFailure(response.status))
  }
  return { failedBatches: 1 }
}

/** 把 HTTP 状态码映射为对用户有意义的失败类别。undefined 表示未拿到响应（网络层失败）。 */
export function classifyFailure(status: number | undefined): string {
  if (status === undefined) return 'network'
  if (TRANSIENT_RETRY_STATUSES.has(status)) return 'network'
  switch (status) {
    case 401:
    case 403:
      return 'unauthorized'
    case 413:
      return 'payload_too_large'
    case 400:
      return 'bad_request'
    default:
      return 'internal'
  }
}

/**
 * 翻译一批文本节点并原地写回 DOM（视口首批先行 + 受控并发流水线）。
 *
 * 返回本批次的失败批次数；**不广播任务终态**（由调用方在任务级广播一次），
 * 仅广播中途进度。
 */
async function translateEntries(
  entries: TextNodeEntry[],
  targetLang: string,
  engine: string = 'google',
  expectedGen: number = currentGeneration,
): Promise<number> {
  const texts = entries.map((e) => e.text)
  const ranges = planBatchRanges(texts, currentLimits)
  if (ranges.length === 0) return 0

  let failedBatches = 0
  let isAborted = false

  // 执行单批次翻译、DOM 写回与进度广播
  const processBatch = async (batch: TextNodeEntry[]): Promise<boolean> => {
    if (isAborted || expectedGen !== currentGeneration) {
      return false
    }

    try {
      const response = await sendBatchWithSplit(batch, targetLang, engine, expectedGen)
      failedBatches += response.failedBatches

      if (expectedGen !== currentGeneration) {
        chrome.runtime
          .sendMessage({
            type: 'TRANSLATION_CANCELLED',
            generation: expectedGen,
          })
          .catch(() => {})
        return false
      }
    } catch (err) {
      failedBatches += 1
      if (expectedGen === currentGeneration) markBatchFailed(batch, 'network')
      console.warn('[Transly] 批量翻译出错:', err)
    }

    if (expectedGen === currentGeneration) {
      chrome.runtime
        .sendMessage({
          type: 'TRANSLATION_PROGRESS',
          generation: expectedGen,
          completed: currentTaskCompleted,
          total: currentTaskTotal,
          failedBatches,
          failedCount: failedItemsMap.size,
        })
        .catch(() => {})
    }

    // 快速熔断防假死：若连续 2 批全部失败且累计成功数为 0，中断后续批次
    if (failedBatches >= 2 && currentTaskCompleted === 0) {
      console.warn(`[Transly] 连续 ${failedBatches} 批全部失败且成功数为 0，触发快速熔断保护`)
      isAborted = true
      return false
    }

    return true
  }

  // 1. 首屏视口第一批（Fast Path）：以最高优先级立即发出并等待，确保首屏最低延迟且快速验证网络
  const firstBatch = entries.slice(ranges[0][0], ranges[0][1])
  const firstOk = await processBatch(firstBatch)
  if (!firstOk || isAborted || expectedGen !== currentGeneration || ranges.length === 1) {
    return failedBatches
  }

  // 2. 视口外其余批次（Pipelining）：受控并发流水线（并发度 3），打满吞吐同时防止本地与网络过载
  const CONCURRENCY = 3
  const remainingRanges = ranges.slice(1)
  let nextBatchIdx = 0

  const worker = async () => {
    while (
      nextBatchIdx < remainingRanges.length &&
      !isAborted &&
      expectedGen === currentGeneration
    ) {
      const [start, end] = remainingRanges[nextBatchIdx++]
      const batch = entries.slice(start, end)
      const ok = await processBatch(batch)
      if (!ok) break
    }
  }

  const workerCount = Math.min(CONCURRENCY, remainingRanges.length)
  await Promise.all(Array.from({ length: workerCount }, () => worker()))

  return failedBatches
}

/** 将一批节点的失败结果记入失败集，供“重试失败项”与部分失败判定使用 */
function markBatchFailed(batch: TextNodeEntry[], errorKind: string) {
  for (const entry of batch) {
    rememberFailed({ ...entry, errorKind })
  }
}

let currentTranslateMode = 'replace'

/** 按 ID 将服务端响应写回 DOM，逐项记录失败，返回本批次成功与失败项计数 */
function applyBatchResponse(
  resData: BatchResponsePayload | undefined,
  batch: TextNodeEntry[],
  expectedGen: number,
): { succeeded: number; failed: number } {
  if (!resData || expectedGen !== currentGeneration) {
    return { succeeded: 0, failed: batch.length }
  }

  let succeeded = 0
  let failed = 0

  if (resData.items && resData.items.length > 0) {
    // 优先消费结构化 items 契约：精准映射 ID
    const itemMap = new Map(resData.items.map((it) => [it.id, it]))
    for (const entry of batch) {
      const itemRes = itemMap.get(entry.id)
      if (itemRes && itemRes.status === 'success' && itemRes.text) {
        if (entry.compositeInfo) {
          const restored = restoreRichTextComposite(
            itemRes.text,
            entry.compositeInfo.baseIds,
            entry.compositeInfo.placeholderToId,
          )
          if (restored) {
            for (const part of entry.compositeInfo.parts) {
              const partText = restored.get(part.id)
              if (partText !== undefined) {
                replaceTextNode(part.node, partText, currentTranslateMode)
              }
            }
            failedItemsMap.delete(entry.id)
            currentTaskCompleted += 1
            succeeded += 1
          } else {
            // 占位符丢失或语序损坏：保持原文 DOM 结构完整
            // 在双语对照模式下，若无法做原位切分，可优雅降级为在复合段落末尾追加整句译文
            if (currentTranslateMode === 'bilingual') {
              const cleanedText = cleanCompositeText(itemRes.text)
              if (cleanedText && entry.compositeInfo.parts.length > 0) {
                const lastNode =
                  entry.compositeInfo.parts[entry.compositeInfo.parts.length - 1].node
                replaceTextNode(lastNode, cleanedText, 'bilingual')
                failedItemsMap.delete(entry.id)
                currentTaskCompleted += 1
                succeeded += 1
                continue
              }
            }
            rememberFailed({
              ...entry,
              errorKind: 'rich_text_mismatch',
            })
            failed += 1
          }
          continue
        }
        replaceTextNode(entry.node, itemRes.text, currentTranslateMode)
        failedItemsMap.delete(entry.id)
        currentTaskCompleted += 1
        succeeded += 1
      } else {
        // 失败项：保留当前 TextNode 原文不变，记录至 failedItemsMap
        rememberFailed({
          ...entry,
          errorKind: itemRes?.error_kind || 'internal',
        })
        failed += 1
      }
    }
  } else if (resData.translations) {
    // 兼容旧版 translations
    resData.translations.forEach((translated, idx) => {
      if (batch[idx]) {
        // 只要返回了非空译文即视为成功。
        // 不能再要求 `translated !== 原文`：专有名词、品牌名、代码标识符
        // （Python / Linux / Transly 等）译后本就与原文一致，旧判定会把它们
        // 记成失败，导致整页明明翻译成功，徽标却显示 WARN、Popup 提示「部分失败」。
        if (translated) {
          if (batch[idx].compositeInfo) {
            const restored = restoreRichTextComposite(
              translated,
              batch[idx].compositeInfo.baseIds,
              batch[idx].compositeInfo.placeholderToId,
            )
            if (restored) {
              for (const part of batch[idx].compositeInfo.parts) {
                const partText = restored.get(part.id)
                if (partText !== undefined) {
                  replaceTextNode(part.node, partText, currentTranslateMode)
                }
              }
              failedItemsMap.delete(batch[idx].id)
              currentTaskCompleted += 1
              succeeded += 1
            } else {
              if (currentTranslateMode === 'bilingual') {
                const cleanedText = cleanCompositeText(translated)
                if (cleanedText && batch[idx].compositeInfo.parts.length > 0) {
                  const parts = batch[idx].compositeInfo.parts
                  const lastNode = parts[parts.length - 1].node
                  replaceTextNode(lastNode, cleanedText, 'bilingual')
                  failedItemsMap.delete(batch[idx].id)
                  currentTaskCompleted += 1
                  succeeded += 1
                  return
                }
              }
              rememberFailed({
                ...batch[idx],
                errorKind: 'rich_text_mismatch',
              })
              failed += 1
            }
            return
          }
          replaceTextNode(batch[idx].node, translated, currentTranslateMode)
          failedItemsMap.delete(batch[idx].id)
          currentTaskCompleted += 1
          succeeded += 1
        } else {
          rememberFailed({ ...batch[idx] })
          failed += 1
        }
      }
    })
  }

  return { succeeded, failed }
}

/**
 * 广播任务终态。
 *
 * 必须由任务级入口（首屏 / 重试 / 增量批次）在**单个批次组**结束后调用一次，
 * 而不是在 `translateEntries` 内部：增量监听会为每个 pending root 各调一次
 * `translateEntries`，若终态在函数内广播，无限滚动页面会持续闪烁徽标，
 * 且 `currentTaskTotal` 只增不减导致百分比永不收敛。
 */
function broadcastTerminalState(expectedGen: number, failedBatches: number) {
  const isTotalFailed = currentTaskCompleted === 0 && currentTaskTotal > 0
  const isPartialFailed = failedItemsMap.size > 0 && currentTaskCompleted > 0
  chrome.runtime
    .sendMessage({
      type: isTotalFailed
        ? 'TRANSLATION_FAILED'
        : isPartialFailed
          ? 'TRANSLATION_PARTIAL_FAILED'
          : 'TRANSLATION_FINISHED',
      generation: expectedGen,
      completed: currentTaskCompleted,
      total: currentTaskTotal,
      failedBatches,
      failedCount: failedItemsMap.size,
    })
    .catch(() => {})
}

async function retryFailedTranslations() {
  // 必须整体浅拷贝而非只挑 node/text/id：富文本失败项的 `text` 是含 ⟦T_i⟧ 占位符的
  // 复合文本，`compositeInfo` 才是把它还原回各子节点的唯一依据。若在此丢弃
  // `compositeInfo`，`applyBatchResponse` 会走普通分支，把整段译文连同未还原的
  // 占位符一起塞进第一个文本节点，造成「占位符泄漏 + 段落结构错乱」。
  const entries = Array.from(failedItemsMap.values()).map((entry) => ({
    ...entry,
    errorKind: undefined,
  }))
  if (entries.length === 0) return

  stopDynamicObserver()
  currentGeneration += 1
  const retryGeneration = currentGeneration

  // 重置重试批次的计数基准，杜绝 completed 累加超过 total
  currentTaskTotal = entries.length
  currentTaskCompleted = 0
  failedItemsMap.clear()

  const failedBatches = await translateEntries(
    entries,
    currentTargetLang,
    currentEngine,
    retryGeneration,
  )
  if (retryGeneration === currentGeneration) {
    broadcastTerminalState(retryGeneration, failedBatches)
    startDynamicObserver(currentTargetLang, currentEngine)
  }
}

function cancelTranslation() {
  // 先冻结当前任务中尚未成功应用的节点，再递增代际丢弃迟到响应
  for (const entry of currentTaskEntries) {
    if (!translatedTextSet.has(entry.node) && document.body.contains(entry.node)) {
      rememberFailed({ ...entry, errorKind: 'cancelled' })
    }
  }
  currentGeneration += 1
  stopDynamicObserver()
  chrome.runtime
    .sendMessage({
      type: 'TRANSLATION_CANCELLED',
      generation: currentGeneration,
      completed: currentTaskCompleted,
      total: currentTaskTotal,
      failedCount: failedItemsMap.size,
    })
    .catch(() => {})
}

/**
 * 执行整页沉浸式翻译（分批次打包请求，原地替换）
 */
export async function translateFullPage(
  targetLang: string = 'zh-CN',
  engine: string = 'google',
  mode: string = 'replace',
) {
  currentTranslateMode = mode
  // 递增任务代际版本号，立即使所有历史在途异步批次响应失效
  currentGeneration += 1
  const taskGeneration = currentGeneration
  failedItemsMap.clear()
  currentTaskEntries = collectTextNodes(document.body)
  stopDynamicObserver()

  // 切批上限必须向服务端索取：本端写死项数而服务端按字符总量拒绝会造成整批 413
  await refreshBatchLimits()
  if (taskGeneration !== currentGeneration) return

  // 视口感知调度（首屏优先 Viewport-First）：
  // 将当前视口内可见文本优先置顶先发请求，极速呈现首屏译文；离屏内容平滑后置处理
  const { prioritized } = splitByViewport(currentTaskEntries)
  currentTaskEntries = prioritized
  const entries = prioritized
  currentTaskTotal = entries.length
  currentTaskCompleted = 0

  if (entries.length === 0) {
    console.log('[Transly] 未检测到可翻译的文本节点')
    broadcastTerminalState(taskGeneration, 0)
    startDynamicObserver(targetLang, engine)
    return
  }

  chrome.runtime
    .sendMessage({
      type: 'TRANSLATION_STARTED',
      generation: taskGeneration,
      total: entries.length,
    })
    .catch(() => {})

  console.log(
    `[Transly] 收集到 ${entries.length} 个文本节点，任务版本=${taskGeneration}，开始批量翻译...`,
  )
  const failedBatches = await translateEntries(entries, targetLang, engine, taskGeneration)

  // 翻译结束时再次校验代际版本，避免中途已被还原后误开启动态监听
  if (taskGeneration === currentGeneration) {
    broadcastTerminalState(taskGeneration, failedBatches)
    console.log('[Transly] 首屏整页翻译完成，已开启动态增量监听')
    startDynamicObserver(targetLang, engine)
  }
}

// 监听来自扩展 Popup 或快捷指令的消息
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === 'PING_CONTENT_SCRIPT') {
    // 注入探测：响应存在即说明本脚本已就绪，宿主无需重复注入。
    // 同步响应，不返回 true，避免留下悬挂的消息端口。
    sendResponse({ ok: true })
    return
  }

  if (message.type === 'START_PAGE_TRANSLATION') {
    translateFullPage(
      message.target_lang || 'zh-CN',
      message.engine || 'google',
      message.mode || 'replace',
    )
      .then(() => sendResponse({ ok: true }))
      .catch((err) => sendResponse({ ok: false, error: String(err) }))
    return true
  }

  if (message.type === 'CANCEL_PAGE_TRANSLATION') {
    cancelTranslation()
    sendResponse({ ok: true })
    return true
  }

  if (message.type === 'RETRY_FAILED_TRANSLATION') {
    retryFailedTranslations()
      .then(() => sendResponse({ ok: true }))
      .catch((err) => sendResponse({ ok: false, error: String(err) }))
    return true
  }

  if (message.type === 'RESTORE_PAGE_TRANSLATION') {
    // 递增任务版本号，彻底废弃所有在途异步批次响应写回
    currentGeneration += 1
    failedItemsMap.clear()
    currentTaskEntries = []
    currentTaskTotal = 0
    currentTaskCompleted = 0
    stopDynamicObserver()
    restoreOriginalText()
    chrome.runtime
      .sendMessage({
        type: 'TRANSLATION_RESTORED',
        generation: currentGeneration,
      })
      .catch(() => {})
    sendResponse({ ok: true })
    return true
  }
})
