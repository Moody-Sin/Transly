/**
 * Transly 网页内容脚本 - 沉浸式整页原地替换（仅译文）
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
  if (IGNORED_TAGS.has(element.tagName)) return false
  if (element.hasAttribute('data-no-translate')) return false
  if (element.classList.contains('notranslate')) return false

  // 沿祖先链深度检查是否有屏蔽标签或属性，防止嵌套在屏蔽区域内的文本节点被误翻
  if (element.closest('[data-no-translate], .notranslate, pre, code, textarea, input, script, style')) {
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
}

interface TextNodeEntry {
  id: string
  node: Text
  text: string
}

// 维护失败文本节点映射，供局部重试与状态报告
const failedItemsMap = new Map<string, TextNodeEntry & { errorKind?: string }>()
let nodeSequenceId = 0

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
let currentTargetLang = 'zh-CN'
let currentEngine = 'google'

// 任务版本号协调机制：杜绝请求跨越“还原原文”或“重新翻译”边界时写回污染
let currentGeneration = 0
let currentTaskEntries: TextNodeEntry[] = []
let currentTaskTotal = 0
let currentTaskCompleted = 0

/**
 * 启动动态增量监听（针对 SPA 单页或瀑布流懒加载内容，监听 childList 与 characterData）
 */
export function startDynamicObserver(
  targetLang: string = 'zh-CN',
  engine: string = 'google',
  generation: number = currentGeneration,
) {
  currentTargetLang = targetLang
  currentEngine = engine

  // 若已在监听，更新语言和引擎即可
  if (isObserving && currentObserver) return
  isObserving = true

  const pendingNodes = new Set<Node>()
  let debounceTimer: number | null = null

  currentObserver = new MutationObserver((mutations) => {
    // 若当前任务版本已失效，停止收集
    if (generation !== currentGeneration) return

    for (const mutation of mutations) {
      if (mutation.type === 'childList') {
        mutation.addedNodes.forEach((node) => {
          if (node.nodeType === Node.ELEMENT_NODE || node.nodeType === Node.TEXT_NODE) {
            pendingNodes.add(node)
          }
        })
      } else if (mutation.type === 'characterData') {
        if (mutation.target.nodeType === Node.TEXT_NODE) {
          const textNode = mutation.target as Text
          if (!translatedTextSet.has(textNode)) {
            pendingNodes.add(textNode)
          }
        }
      }
    }

    if (pendingNodes.size > 0) {
      if (debounceTimer) clearTimeout(debounceTimer)
      debounceTimer = window.setTimeout(async () => {
        if (generation !== currentGeneration) {
          pendingNodes.clear()
          return
        }
        const nodesToProcess = Array.from(pendingNodes)
        pendingNodes.clear()
        for (const root of nodesToProcess) {
          if (generation !== currentGeneration) break
          if (document.body.contains(root)) {
            const entries = collectTextNodes(root)
            if (entries.length > 0) {
              currentTaskEntries.push(...entries)
              currentTaskTotal += entries.length
              await translateEntries(entries, currentTargetLang, currentEngine, 30, generation)
            }
          }
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

async function translateEntries(
  entries: TextNodeEntry[],
  targetLang: string,
  engine: string = 'google',
  batchSize: number = 30,
  expectedGen: number = currentGeneration,
) {
  const requestTotalNodes = entries.length
  const totalBatches = Math.ceil(requestTotalNodes / batchSize)
  let failedBatches = 0

  for (let i = 0; i < entries.length; i += batchSize) {
    // 发送前检查：如果用户已经点击“取消”、“还原原文”或发起了新任务，直接阻断
    if (expectedGen !== currentGeneration) {
      console.log(`[Transly] 批次发送前拦截：任务版本已过期 (当前=${currentGeneration}, 批次=${expectedGen})`)
      chrome.runtime.sendMessage({
        type: 'TRANSLATION_CANCELLED',
        generation: expectedGen,
      }).catch(() => {})
      return
    }

    const batch = entries.slice(i, i + batchSize)
    const items = batch.map((item) => ({ id: item.id, text: item.text }))
    const texts = batch.map((item) => item.text)

    try {
      const response = await new Promise<{ ok: boolean; data?: BatchResponsePayload; error?: string }>(
        (resolve) => {
          chrome.runtime.sendMessage(
            {
              type: 'TRANSLATE_BATCH',
              items,
              texts,
              source_lang: 'auto',
              target_lang: targetLang,
              engine,
            },
            resolve,
          )
        },
      )

      // 响应后强校验：如果请求在途期间用户触发了取消、还原或新任务，彻底丢弃响应，严禁写入 DOM
      if (expectedGen !== currentGeneration) {
        console.log(`[Transly] 响应写回前安全拦截：任务版本已过期，丢弃译文 (当前=${currentGeneration}, 批次=${expectedGen})`)
        chrome.runtime.sendMessage({
          type: 'TRANSLATION_CANCELLED',
          generation: expectedGen,
        }).catch(() => {})
        return
      }

      if (response && response.ok && response.data) {
        const resData = response.data
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
              failedItemsMap.set(entry.id, {
                ...entry,
                errorKind: itemRes?.error_kind || 'internal',
              })
            }
          }
        } else if (resData.translations) {
          // 兼容旧版 translations
          resData.translations.forEach((translated, idx) => {
            if (batch[idx] && expectedGen === currentGeneration) {
              if (translated && translated !== batch[idx].text) {
                replaceTextNode(batch[idx].node, translated)
                failedItemsMap.delete(batch[idx].id)
                currentTaskCompleted += 1
              } else {
                failedItemsMap.set(batch[idx].id, { ...batch[idx] })
              }
            }
          })
        }
      } else {
        failedBatches += 1
        for (const entry of batch) {
          failedItemsMap.set(entry.id, { ...entry, errorKind: 'network' })
        }
      }
    } catch (err) {
      failedBatches += 1
      for (const entry of batch) {
        failedItemsMap.set(entry.id, { ...entry, errorKind: 'network' })
      }
      console.warn('[Transly] 批量翻译出错:', err)
    }

    // 广播进度给 Popup 与扩展徽标
    if (expectedGen === currentGeneration) {
      chrome.runtime.sendMessage({
        type: 'TRANSLATION_PROGRESS',
        generation: expectedGen,
        completed: currentTaskCompleted,
        total: currentTaskTotal,
        failedBatches,
        totalBatches,
        failedCount: failedItemsMap.size,
      }).catch(() => {})
    }
  }

  if (expectedGen === currentGeneration) {
    const isTotalFailed = currentTaskCompleted === 0 && currentTaskTotal > 0
    const isPartialFailed = failedItemsMap.size > 0 && currentTaskCompleted > 0
    chrome.runtime.sendMessage({
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
    }).catch(() => {})
  }
}


async function retryFailedTranslations() {
  const entries = Array.from(failedItemsMap.values()).map(({ node, text, id }) => ({ node, text, id }))
  if (entries.length === 0) return

  currentGeneration += 1
  const retryGeneration = currentGeneration
  await translateEntries(entries, currentTargetLang, currentEngine, 30, retryGeneration)
}

function cancelTranslation() {
  // 先冻结当前任务中尚未成功应用的节点，再递增代际丢弃迟到响应
  for (const entry of currentTaskEntries) {
    if (!translatedTextSet.has(entry.node) && document.body.contains(entry.node)) {
      failedItemsMap.set(entry.id, { ...entry, errorKind: 'cancelled' })
    }
  }
  currentGeneration += 1
  stopDynamicObserver()
  chrome.runtime.sendMessage({
    type: 'TRANSLATION_CANCELLED',
    generation: currentGeneration,
    completed: currentTaskCompleted,
    total: currentTaskTotal,
    failedCount: failedItemsMap.size,
  }).catch(() => {})
}

/**
 * 执行整页沉浸式翻译（分批次打包请求，原地替换）
 */
export async function translateFullPage(
  targetLang: string = 'zh-CN',
  engine: string = 'google',
  batchSize: number = 30,
) {
  // 递增任务代际版本号，立即使所有历史在途异步批次响应失效
  currentGeneration += 1
  const taskGeneration = currentGeneration
  failedItemsMap.clear()
  currentTaskEntries = collectTextNodes(document.body)
  stopDynamicObserver()

  const entries = currentTaskEntries
  currentTaskTotal = entries.length
  currentTaskCompleted = 0

  if (entries.length === 0) {
    console.log('[Transly] 未检测到可翻译的文本节点')
    chrome.runtime.sendMessage({
      type: 'TRANSLATION_FINISHED',
      generation: taskGeneration,
      completed: 0,
      total: 0,
      failedBatches: 0,
    }).catch(() => {})
    startDynamicObserver(targetLang, engine, taskGeneration)
    return
  }

  chrome.runtime.sendMessage({
    type: 'TRANSLATION_STARTED',
    generation: taskGeneration,
    total: entries.length,
  }).catch(() => {})

  console.log(`[Transly] 收集到 ${entries.length} 个文本节点，任务版本=${taskGeneration}，开始批量翻译...`)
  await translateEntries(entries, targetLang, engine, batchSize, taskGeneration)

  // 翻译结束时再次校验代际版本，避免中途已被还原后误开启动态监听
  if (taskGeneration === currentGeneration) {
    console.log('[Transly] 首屏整页翻译完成，已开启动态增量监听')
    startDynamicObserver(targetLang, engine, taskGeneration)
  }
}

// 监听来自扩展 Popup 或快捷指令的消息
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
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
    chrome.runtime.sendMessage({
      type: 'TRANSLATION_RESTORED',
      generation: currentGeneration,
    }).catch(() => {})
    sendResponse({ ok: true })
    return true
  }
})

