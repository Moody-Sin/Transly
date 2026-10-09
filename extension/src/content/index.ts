/**
 * Transly 网页内容脚本 - 沉浸式整页原地替换（仅译文）
 *
 * 注：本文件中 `chrome.runtime.sendMessage(...).catch(() => {})` 的写法专用于
 * **广播类**消息（进度、终态）。Popup 关闭或 Service Worker 休眠时无接收方，
 * reject 属正常路径而非故障，因此静默；需要确认投递结果的请求
 * （如 `TRANSLATE_BATCH`）一律走 `sendBatch` 并处理失败。
 */

console.log('[Transly Content Script] 沉浸式翻译引擎已就绪')

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

export function replaceTextNode(node: Text, translatedText: string) {
  if (!originalTextMap.has(node)) {
    originalTextMap.set(node, node.nodeValue || '')
  }
  translatedTextSet.add(node)
  node.nodeValue = translatedText

  // 若父节点是元素，挂载 data-transly-original 标记（若尚未挂载或为空）
  // 方便快捷悬浮查看原文（按下 Alt 键悬停查看）
  const parent = node.parentElement
  if (parent && !parent.hasAttribute('data-transly-has-translation')) {
    parent.setAttribute('data-transly-has-translation', 'true')
  }
}

export function restoreOriginalText(container: Node = document.body) {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT)
  let curr = walker.nextNode()
  while (curr) {
    const textNode = curr as Text
    if (originalTextMap.has(textNode)) {
      const orig = originalTextMap.get(textNode)
      if (orig !== undefined) {
        textNode.nodeValue = orig
        originalTextMap.delete(textNode)
        translatedTextSet.delete(textNode)
      }
    }
    curr = walker.nextNode()
  }

  // 清除父元素标记与残留 Tooltip
  if (container instanceof Element) {
    container.querySelectorAll('[data-transly-has-translation]').forEach((el) => {
      el.removeAttribute('data-transly-has-translation')
    })
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

  tooltipContainer.innerHTML = `<div class="badge">Original / 原文</div><div>${escapeHtml(originalText)}</div>`
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

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

// 监听鼠标在网页上的移动，当目标为已翻译节点且用户悬停（或按住 Alt）时显示原文 Tooltip
document.addEventListener('mouseover', (e) => {
  const target = e.target
  if (!(target instanceof Element)) return
  if (!target.hasAttribute('data-transly-has-translation')) return

  // 提取该元素或其子文本节点中缓存的原文
  let origText = ''
  for (const child of Array.from(target.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE && originalTextMap.has(child as Text)) {
      origText += (originalTextMap.get(child as Text) || '') + ' '
    }
  }

  if (origText.trim()) {
    showOriginalTooltip(origText.trim(), e.clientX, e.clientY)
  }
})

document.addEventListener('mouseout', (e) => {
  const target = e.target
  if (target instanceof Element && target.hasAttribute('data-transly-has-translation')) {
    hideOriginalTooltip()
  }
})

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    hideOriginalTooltip()
  }
})

interface TextNodeEntry {
  id: string
  node: Text
  text: string
}

// 维护失败文本节点映射，供局部重试与状态报告。
// 强引用 Map 会随 SPA 导航与多次还原无限增长，并钉住已脱离文档的 Text 节点，
// 因此容量必须封顶：超出时丢弃最旧项（其重试机会让位于近期失败）。
const failedItemsMap = new Map<string, TextNodeEntry & { errorKind?: string }>()
const MAX_FAILED_ITEMS = 2_000
let nodeSequenceId = 0

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
 * 递归收集容器内所有有效的正文文本节点
 */
export function collectTextNodes(root: Node = document.body): TextNodeEntry[] {
  const entries: TextNodeEntry[] = []

  // 如果 root 自身即是文本节点且尚未翻译
  if (root.nodeType === Node.TEXT_NODE) {
    const textNode = root as Text
    const parent = textNode.parentElement
    if (parent && shouldTranslateElement(parent) && !translatedTextSet.has(textNode)) {
      const text = textNode.nodeValue?.trim()
      if (text && text.length >= 2 && !/^[\d\s.,/#!$%^&*;:{}=\-_`~()]+$/.test(text)) {
        nodeSequenceId += 1
        return [{ id: `node_${nodeSequenceId}`, node: textNode, text: textNode.nodeValue || '' }]
      }
    }
    return []
  }

  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement
      if (!parent) return NodeFilter.FILTER_REJECT
      if (!shouldTranslateElement(parent)) return NodeFilter.FILTER_REJECT
      if (translatedTextSet.has(node as Text)) return NodeFilter.FILTER_REJECT

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
    entries.push({
      id: `node_${nodeSequenceId}`,
      node: currentNode as Text,
      text: currentNode.nodeValue || '',
    })
    currentNode = walker.nextNode()
  }

  return entries
}

let isObserving = false
let currentObserver: MutationObserver | null = null
let observerDebounceTimer: number | null = null
const observerPendingNodes = new Set<Node>()
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
      if (observerDebounceTimer) clearTimeout(observerDebounceTimer)
      observerDebounceTimer = window.setTimeout(async () => {
        const generation = currentGeneration
        const nodesToProcess = Array.from(observerPendingNodes)
        observerPendingNodes.clear()
        observerDebounceTimer = null

        // 增量批次是独立任务：重置计数基准，否则进度百分比与失败判定会被
        // 首屏任务的累计值污染（total 只增不减 → 永不收敛；
        // failedItemsMap 不清空 → 后续批次被误判为“部分失败”）。
        const batchEntries: TextNodeEntry[] = []
        for (const root of nodesToProcess) {
          if (generation !== currentGeneration) break
          if (document.body.contains(root)) {
            batchEntries.push(...collectTextNodes(root))
          }
        }
        if (batchEntries.length === 0 || generation !== currentGeneration) return

        // 增量批次是独立任务：用当前批次整体替换而不是追加。
        // 追加会让无限滚动页面持续增长该数组，并强引用已脱离文档的 Text 节点。
        currentTaskEntries = batchEntries
        currentTaskTotal = batchEntries.length
        currentTaskCompleted = 0
        failedItemsMap.clear()

        const failedBatches = await translateEntries(
          batchEntries,
          currentTargetLang,
          currentEngine,
          generation,
        )
        if (generation === currentGeneration) {
          broadcastTerminalState(generation, failedBatches)
        }
      }, 500)
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

import { planBatchRanges, type BatchLimits } from '../shared/batchPlan'

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
    chrome.runtime.sendMessage(
      {
        type: 'TRANSLATE_BATCH',
        items,
        source_lang: 'auto',
        target_lang: targetLang,
        engine,
      },
      resolve,
    )
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
async function sendBatchWithSplit(
  batch: TextNodeEntry[],
  targetLang: string,
  engine: string,
  expectedGen: number,
): Promise<{ failedBatches: number }> {
  if (batch.length === 0) return { failedBatches: 0 }

  const response = await sendBatch(
    batch.map((item) => ({ id: item.id, text: item.text })),
    targetLang,
    engine,
  )

  if (response.ok && response.data) {
    applyBatchResponse(response.data, batch, expectedGen)
    return { failedBatches: 0 }
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

  if (expectedGen === currentGeneration) markBatchFailed(batch, 'network')
  return { failedBatches: 1 }
}

/**
 * 翻译一批文本节点并原地写回 DOM。
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
  let failedBatches = 0
  const texts = entries.map((e) => e.text)
  const ranges = planBatchRanges(texts, currentLimits)

  for (const [rangeStart, rangeEnd] of ranges) {
    // 发送前检查：如果用户已经点击“取消”、“还原原文”或发起了新任务，直接阻断
    if (expectedGen !== currentGeneration) {
      console.log(
        `[Transly] 批次发送前拦截：任务版本已过期 (当前=${currentGeneration}, 批次=${expectedGen})`,
      )
      chrome.runtime
        .sendMessage({
          type: 'TRANSLATION_CANCELLED',
          generation: expectedGen,
        })
        .catch(() => {})
      return failedBatches
    }

    const batch = entries.slice(rangeStart, rangeEnd)

    try {
      const response = await sendBatchWithSplit(batch, targetLang, engine, expectedGen)
      failedBatches += response.failedBatches

      // 响应后强校验：如果请求在途期间用户触发了取消、还原或新任务，彻底丢弃响应，严禁写入 DOM
      if (expectedGen !== currentGeneration) {
        console.log(
          `[Transly] 响应写回前安全拦截：任务版本已过期，丢弃译文 (当前=${currentGeneration}, 批次=${expectedGen})`,
        )
        chrome.runtime
          .sendMessage({
            type: 'TRANSLATION_CANCELLED',
            generation: expectedGen,
          })
          .catch(() => {})
        return failedBatches
      }
    } catch (err) {
      failedBatches += 1
      if (expectedGen === currentGeneration) markBatchFailed(batch, 'network')
      console.warn('[Transly] 批量翻译出错:', err)
    }

    // 广播进度给 Popup 与扩展徽标
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
  }

  return failedBatches
}

/** 将一批节点的失败结果记入失败集，供“重试失败项”与部分失败判定使用 */
function markBatchFailed(batch: TextNodeEntry[], errorKind: string) {
  for (const entry of batch) {
    rememberFailed({ ...entry, errorKind })
  }
}

/** 按 ID 将服务端响应写回 DOM，逐项记录失败 */
function applyBatchResponse(
  resData: BatchResponsePayload | undefined,
  batch: TextNodeEntry[],
  expectedGen: number,
) {
  if (!resData) return

  // 代际闸门：还原原文 / 取消 / 新任务后到达的响应一律丢弃，严禁写回 DOM。
  // `items` 主契约分支此前没有这道判定（只有旧版 `translations` 分支有），
  // 导致「还原原文」后仍在途的批次把译文重新写回页面 —— 用户看到的是还原失败。
  if (expectedGen !== currentGeneration) return

  if (resData.items && resData.items.length > 0) {
    // 优先消费结构化 items 契约：精准映射 ID
    const itemMap = new Map(resData.items.map((it) => [it.id, it]))
    for (const entry of batch) {
      const itemRes = itemMap.get(entry.id)
      if (itemRes && itemRes.status === 'success' && itemRes.text) {
        replaceTextNode(entry.node, itemRes.text)
        failedItemsMap.delete(entry.id)
        currentTaskCompleted += 1
      } else {
        // 失败项：保留当前 TextNode 原文不变，记录至 failedItemsMap
        rememberFailed({
          ...entry,
          errorKind: itemRes?.error_kind || 'internal',
        })
      }
    }
  } else if (resData.translations) {
    // 兼容旧版 translations
    resData.translations.forEach((translated, idx) => {
      if (batch[idx]) {
        if (translated && translated !== batch[idx].text) {
          replaceTextNode(batch[idx].node, translated)
          failedItemsMap.delete(batch[idx].id)
          currentTaskCompleted += 1
        } else {
          rememberFailed({ ...batch[idx] })
        }
      }
    })
  }
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
  const entries = Array.from(failedItemsMap.values()).map(({ node, text, id }) => ({
    node,
    text,
    id,
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
export async function translateFullPage(targetLang: string = 'zh-CN', engine: string = 'google') {
  // 递增任务代际版本号，立即使所有历史在途异步批次响应失效
  currentGeneration += 1
  const taskGeneration = currentGeneration
  failedItemsMap.clear()
  currentTaskEntries = collectTextNodes(document.body)
  stopDynamicObserver()

  // 切批上限必须向服务端索取：本端写死项数而服务端按字符总量拒绝会造成整批 413
  await refreshBatchLimits()
  if (taskGeneration !== currentGeneration) return

  const entries = currentTaskEntries
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
    translateFullPage(message.target_lang || 'zh-CN', message.engine || 'google')
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
