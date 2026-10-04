/**
 * Transly 网页内容脚本 - 沉浸式整页原地替换（仅译文）
 */

console.log('[Transly Content Script] 沉浸式翻译引擎已就绪')

// 标记节点属性以支持一键还原原文
export const ORIGINAL_TEXT_ATTR = 'data-transly-original'

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
  return true
}

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

interface TextNodeEntry {
  node: Text
  text: string
}

/**
 * 递归收集容器内所有有效的正文文本节点
 */
export function collectTextNodes(root: Node = document.body): TextNodeEntry[] {
  const entries: TextNodeEntry[] = []
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement
      if (!parent) return NodeFilter.FILTER_REJECT
      if (!shouldTranslateElement(parent)) return NodeFilter.FILTER_REJECT
      if (parent.hasAttribute(ORIGINAL_TEXT_ATTR)) return NodeFilter.FILTER_REJECT

      const text = node.nodeValue?.trim()
      if (!text || text.length < 2) return NodeFilter.FILTER_SKIP
      // 纯数字或仅符号标点跳过
      if (/^[\d\s.,/#!$%^&*;:{}=\-_`~()]+$/.test(text)) return NodeFilter.FILTER_SKIP

      return NodeFilter.FILTER_ACCEPT
    },
  })

  let currentNode = walker.nextNode()
  while (currentNode) {
    entries.push({
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

/**
 * 启动动态增量监听（针对 SPA 单页或瀑布流懒加载内容）
 */
export function startDynamicObserver(targetLang: string = 'zh-CN', engine: string = 'google') {
  if (isObserving && currentObserver) return
  currentTargetLang = targetLang
  currentEngine = engine
  isObserving = true

  let pendingNodes: Node[] = []
  let debounceTimer: number | null = null

  currentObserver = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === 'childList') {
        mutation.addedNodes.forEach((node) => {
          if (node.nodeType === Node.ELEMENT_NODE || node.nodeType === Node.TEXT_NODE) {
            pendingNodes.push(node)
          }
        })
      }
    }

    if (pendingNodes.length > 0) {
      if (debounceTimer) clearTimeout(debounceTimer)
      debounceTimer = window.setTimeout(async () => {
        const nodesToProcess = [...pendingNodes]
        pendingNodes = []
        for (const root of nodesToProcess) {
          if (document.body.contains(root)) {
            const entries = collectTextNodes(root)
            if (entries.length > 0) {
              await translateEntries(entries, currentTargetLang, currentEngine)
            }
          }
        }
      }, 500)
    }
  })

  currentObserver.observe(document.body, {
    childList: true,
    subtree: true,
  })
}

export function stopDynamicObserver() {
  if (currentObserver) {
    currentObserver.disconnect()
    currentObserver = null
  }
  isObserving = false
}

async function translateEntries(
  entries: TextNodeEntry[],
  targetLang: string,
  engine: string = 'google',
  batchSize: number = 30,
) {
  for (let i = 0; i < entries.length; i += batchSize) {
    const batch = entries.slice(i, i + batchSize)
    const texts = batch.map((item) => item.text)

    try {
      const response = await new Promise<{ ok: boolean; data?: { translations: string[] }; error?: string }>(
        (resolve) => {
          chrome.runtime.sendMessage(
            {
              type: 'TRANSLATE_BATCH',
              texts,
              source_lang: 'auto',
              target_lang: targetLang,
              engine,
            },
            resolve,
          )
        },
      )

      if (response && response.ok && response.data?.translations) {
        response.data.translations.forEach((translated, idx) => {
          if (translated && batch[idx]) {
            replaceTextNode(batch[idx].node, translated)
          }
        })
      }
    } catch (err) {
      console.warn('[Transly] 动态增量翻译出错:', err)
    }
  }
}

/**
 * 执行整页沉浸式翻译（分批次打包请求，原地替换）
 */
export async function translateFullPage(
  targetLang: string = 'zh-CN',
  engine: string = 'google',
  batchSize: number = 30,
) {
  const entries = collectTextNodes(document.body)
  if (entries.length === 0) {
    console.log('[Transly] 未检测到可翻译的文本节点')
    startDynamicObserver(targetLang, engine)
    return
  }

  console.log(`[Transly] 收集到 ${entries.length} 个文本节点，开始批量翻译...`)
  await translateEntries(entries, targetLang, engine, batchSize)
  console.log('[Transly] 首屏整页翻译完成，已开启动态增量监听')
  startDynamicObserver(targetLang, engine)
}

// 监听来自扩展 Popup 或快捷指令的消息
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === 'START_PAGE_TRANSLATION') {
    translateFullPage(message.target_lang || 'zh-CN', message.engine || 'google')
      .then(() => sendResponse({ ok: true }))
      .catch((err) => sendResponse({ ok: false, error: String(err) }))
    return true
  }

  if (message.type === 'RESTORE_PAGE_TRANSLATION') {
    stopDynamicObserver()
    restoreOriginalText()
    sendResponse({ ok: true })
    return true
  }
})

