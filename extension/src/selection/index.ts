/**
 * Transly 划词翻译气泡（按需注入）。
 *
 * 由 Service Worker 在用户点击右键菜单「Transly 翻译选中文本」后注入，
 * 收到 `SHOW_SELECTION_TRANSLATION` 消息才渲染气泡，不做常驻注入、
 * 不监听 `mouseup`，与 `PRIVACY_POLICY.md` 的「仅在主动触发时处理选中文本」一致。
 *
 * 气泡挂载在 **Shadow DOM** 中：
 * - 宿主页面的 CSS 无法污染气泡（`all: initial` + 封闭式 shadow root）；
 * - 宿主页面脚本拿不到 shadow root（`mode: 'closed'`）；
 * - 宿主节点带 `data-no-translate` 且挂在 `documentElement` 上，
 *   因此整页翻译的 TreeWalker 不会把气泡自身的文案当成待翻译正文。
 */
import { t } from '../i18n'
import { countChars } from '../shared/batchPlan'

const HOST_ID = 'transly-selection-bubble-host'

/** 服务端 `MAX_ITEM_CHARS` 的兜底值；真实上限优先从 `/health` 读取 */
const FALLBACK_MAX_ITEM_CHARS = 10_000

interface BatchItemResult {
  status: 'success' | 'failed'
  id: string
  text?: string
  error_kind?: string
  message?: string
}

interface BatchResponsePayload {
  items?: BatchItemResult[]
  translations?: string[]
}

interface AnchorSnapshot {
  top: number
  left: number
  width: number
  height: number
  scrollY: number
}

let activeHost: HTMLElement | null = null
let activeShadow: ShadowRoot | null = null
let anchor: AnchorSnapshot | null = null
let teardownFns: Array<() => void> = []

/**
 * 气泡实例代际。
 *
 * 翻译请求在途期间用户可能按 Esc、点击外部或重新划词（旧气泡被销毁并新建）。
 * 仅凭 `body.isConnected` 无法区分「旧气泡已被移除」与「新气泡已就绪」，
 * 因此每次创建/销毁都递增代际，响应回来时代际不符即丢弃（Lessons Learned 第 15 条）。
 */
let bubbleGeneration = 0

const BUBBLE_CSS = `
  :host {
    all: initial;
  }
  .bubble {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    width: 100%;
    max-height: 320px;
    font-family: system-ui, -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif;
    font-size: 13px;
    line-height: 1.6;
    color: #1e293b;
    background: #ffffff;
    border: 1px solid #cbd5e1;
    border-radius: 10px;
    box-shadow: 0 8px 28px rgba(15, 23, 42, 0.18);
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px 6px 10px;
    background: #f1f5f9;
    border-bottom: 1px solid #e2e8f0;
  }
  .brand {
    font-size: 11px;
    font-weight: 600;
    color: #4f46e5;
    letter-spacing: 0.02em;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .btn {
    padding: 3px 8px;
    font: inherit;
    font-size: 11px;
    color: #334155;
    background: #ffffff;
    border: 1px solid #cbd5e1;
    border-radius: 5px;
    cursor: pointer;
  }
  .btn:hover {
    background: #e2e8f0;
  }
  .btn-close {
    padding: 3px 7px;
    color: #64748b;
  }
  .body {
    padding: 10px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .body.muted {
    color: #64748b;
  }
  .body.error {
    color: #dc2626;
  }
`

/** 移除上一枚气泡（含事件监听），避免重复注入时残留多个宿主节点 */
function disposeBubble() {
  bubbleGeneration += 1
  for (const fn of teardownFns) fn()
  teardownFns = []
  if (activeHost) activeHost.remove()
  activeHost = null
  activeShadow = null
  anchor = null
}

function createBubble(): {
  body: HTMLElement
  copyBtn: HTMLButtonElement
  generation: number
} {
  disposeBubble()

  // 重复注入（Service Worker 探测失败后重注入）可能留下旧宿主，
  // 封闭式 shadow root 无法从新模块作用域取回，只能整体移除重建。
  document.getElementById(HOST_ID)?.remove()

  const host = document.createElement('div')
  host.id = HOST_ID
  host.setAttribute('data-no-translate', '')
  host.style.cssText =
    'all: initial; position: fixed; z-index: 2147483647; top: 0; left: 0; width: 360px;'

  const shadow = host.attachShadow({ mode: 'closed' })
  const sheet = new CSSStyleSheet()
  sheet.replaceSync(BUBBLE_CSS)
  shadow.adoptedStyleSheets = [sheet]

  const bubble = document.createElement('div')
  bubble.className = 'bubble'

  const head = document.createElement('div')
  head.className = 'head'

  const brand = document.createElement('span')
  brand.className = 'brand'
  brand.textContent = 'Transly'

  const actions = document.createElement('div')
  actions.className = 'actions'

  const copyBtn = document.createElement('button')
  copyBtn.className = 'btn'
  copyBtn.type = 'button'
  copyBtn.textContent = t('bubbleCopy')
  copyBtn.style.display = 'none'

  const closeBtn = document.createElement('button')
  closeBtn.className = 'btn btn-close'
  closeBtn.type = 'button'
  closeBtn.textContent = '✕'
  closeBtn.title = t('bubbleClose')

  actions.append(copyBtn, closeBtn)
  head.append(brand, actions)

  const body = document.createElement('div')
  body.className = 'body'

  bubble.append(head, body)
  shadow.append(bubble)

  // 挂在 documentElement 而非 body：整页翻译从 document.body 起遍历，
  // 从源头避开气泡文案（data-no-translate 是第二道防线）。
  document.documentElement.appendChild(host)

  activeHost = host
  activeShadow = shadow

  closeBtn.addEventListener('click', disposeBubble)

  // 捕获阶段监听：即使宿主页面 stopPropagation 也能收到
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') disposeBubble()
  }
  const onPointerDown = (e: MouseEvent) => {
    if (activeHost && !e.composedPath().includes(activeHost)) disposeBubble()
  }
  const onViewportChange = () => reposition()

  window.addEventListener('keydown', onKeyDown, true)
  document.addEventListener('mousedown', onPointerDown, true)
  window.addEventListener('scroll', onViewportChange, true)
  window.addEventListener('resize', onViewportChange)
  teardownFns.push(
    () => window.removeEventListener('keydown', onKeyDown, true),
    () => document.removeEventListener('mousedown', onPointerDown, true),
    () => window.removeEventListener('scroll', onViewportChange, true),
    () => window.removeEventListener('resize', onViewportChange),
  )

  return { body, copyBtn, generation: bubbleGeneration }
}

/**
 * 按锚点（选区）摆放气泡。
 *
 * 锚点记录的是**视口坐标 + 记录时的滚动偏移**：滚动时页面坐标不变，
 * 视口坐标需要按滚动差值回推，否则气泡会跟着页面一起滚走。
 */
function reposition() {
  if (!activeHost || !anchor || !activeShadow) return

  const bubble = activeShadow.querySelector('.bubble') as HTMLElement | null
  if (!bubble) return

  const margin = 8
  const vw = document.documentElement.clientWidth
  const vh = document.documentElement.clientHeight
  const width = Math.min(360, Math.max(200, vw - margin * 2))
  const height = bubble.offsetHeight || 0

  const left = Math.min(Math.max(anchor.left, margin), Math.max(margin, vw - margin - width))
  const topInViewport = anchor.top - (window.scrollY - anchor.scrollY)
  const anchorHeight = anchor.height || 0

  // 正常朝下放置：位于选区文字下方，避开正在阅读的选区，绝不遮盖选中文本
  let top = topInViewport + anchorHeight + margin
  // 选区下方放不下时翻到选区上方，上下都放不下则贴住可视区顶部
  if (top + height > vh - margin) {
    const above = topInViewport - margin - height
    top = above >= margin ? above : Math.max(margin, vh - margin - height)
  }

  activeHost.style.width = `${width}px`
  activeHost.style.left = `${left}px`
  activeHost.style.top = `${Math.max(margin, top)}px`
}

function setBody(body: HTMLElement, text: string, variant?: 'muted' | 'error') {
  body.textContent = text
  body.className = variant ? `body ${variant}` : 'body'
  reposition()
}

async function getMaxItemChars(): Promise<number> {
  try {
    const res = await chrome.runtime.sendMessage({ type: 'GET_BATCH_LIMITS' })
    if (res?.ok && typeof res.limits?.maxItemChars === 'number') {
      return res.limits.maxItemChars
    }
  } catch {
    // 服务端未就绪时退回兜底值，不阻断划词翻译
  }
  return FALLBACK_MAX_ITEM_CHARS
}

async function requestTranslation(
  text: string,
  targetLang: string,
  engine: string,
): Promise<{ ok: boolean; data?: BatchResponsePayload; error?: string; status?: number }> {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage(
      {
        type: 'TRANSLATE_BATCH',
        items: [{ id: 'selection', text }],
        source_lang: 'auto',
        target_lang: targetLang,
        engine,
      },
      (res) => {
        // Service Worker 被回收或扩展被重载时回调拿到 undefined，
        // 不处理会留下永久「翻译中...」的气泡。
        if (chrome.runtime.lastError || !res) {
          resolve({ ok: false, error: chrome.runtime.lastError?.message || 'no response' })
          return
        }
        resolve(res)
      },
    )
  })
}

/**
 * 复制译文。
 *
 * 注入发生在右键菜单回调之后，用户手势通常已不在当前帧，
 * `navigator.clipboard.writeText` 可能以 `NotAllowedError` 拒绝；
 * 因此回落到 `execCommand('copy')` 临时文本域方案（两条路径都失败才提示失败）。
 */
async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    // 回落到 execCommand
  }

  const textarea = document.createElement('textarea')
  textarea.value = text
  textarea.setAttribute('readonly', '')
  textarea.style.cssText = 'position: fixed; top: -1000px; left: -1000px; opacity: 0;'
  document.documentElement.appendChild(textarea)
  textarea.select()
  let ok: boolean
  try {
    ok = document.execCommand('copy')
  } catch {
    ok = false
  }
  textarea.remove()
  return ok
}

/** 从当前视口选区取得锚点；无选区时返回 null */
function captureSelectionAnchor(): AnchorSnapshot | null {
  const selection = window.getSelection()
  if (!selection || selection.rangeCount === 0) return null
  const rect = selection.getRangeAt(0).getBoundingClientRect()
  if (rect.width === 0 && rect.height === 0) return null
  return {
    top: rect.top,
    left: rect.left,
    width: rect.width,
    height: rect.height,
    scrollY: window.scrollY,
  }
}

async function showSelectionTranslation(payload: {
  text?: string
  target_lang?: string
  engine?: string
}) {
  // 优先用页面自身选区：右键菜单回调里 selectionText 可能被浏览器截断
  const liveSelection = window.getSelection()?.toString().trim() || ''
  const text = liveSelection || (payload.text || '').trim()

  const { body, copyBtn, generation } = createBubble()
  // 锚点必须在 createBubble 之后采集：disposeBubble 会把 anchor 清空
  anchor = captureSelectionAnchor() ?? {
    // 取不到选区矩形（如跨帧选区）时退化为可视区右上角固定位置
    top: 0,
    left: document.documentElement.clientWidth - 380,
    width: 0,
    height: 0,
    scrollY: window.scrollY,
  }

  if (!text) {
    setBody(body, t('bubbleEmptySelection'), 'muted')
    return
  }

  const maxItemChars = await getMaxItemChars()
  // 必须按码点数比较（与服务端一致）：String.length 是 UTF-16 code unit 数，
  // emoji/生僻字占 2 个 unit，会把服务端本可正常翻译的长选区误判为超长。
  if (countChars(text) > maxItemChars) {
    setBody(body, t('bubbleTextTooLong'), 'error')
    return
  }

  setBody(body, t('bubbleTranslating'), 'muted')

  const res = await requestTranslation(
    text,
    payload.target_lang || 'zh-CN',
    payload.engine || 'google',
  )

  // 请求在途期间用户可能已关闭气泡（Esc / 点击外部 / 新一次划词）
  if (generation !== bubbleGeneration || !body.isConnected) return

  if (!res.ok) {
    const isGoogle =
      !payload.engine || payload.engine === 'google' || payload.engine === 'google_web'
    const errorText = isGoogle ? t('bubbleFailedGoogleNetwork') : t('bubbleFailed')
    setBody(body, errorText, 'error')
    return
  }

  const item = res.data?.items?.[0]
  if (item?.status === 'failed') {
    const isGoogle =
      !payload.engine || payload.engine === 'google' || payload.engine === 'google_web'
    const isNetwork = item.error_kind === 'network'
    const errorText =
      isGoogle && isNetwork ? t('bubbleFailedGoogleNetwork') : item.message || t('bubbleFailed')
    setBody(body, errorText, 'error')
    return
  }

  const translated =
    item?.status === 'success' && item.text
      ? item.text
      : (res.data?.translations?.[0] as string | undefined)

  if (!translated) {
    const isGoogle =
      !payload.engine || payload.engine === 'google' || payload.engine === 'google_web'
    const errorText = isGoogle ? t('bubbleFailedGoogleNetwork') : t('bubbleFailed')
    setBody(body, errorText, 'error')
    return
  }

  setBody(body, translated)
  copyBtn.style.display = 'block'
  copyBtn.addEventListener('click', async () => {
    const ok = await copyText(translated)
    copyBtn.textContent = ok ? t('bubbleCopied') : t('bubbleFailed')
    setTimeout(() => {
      copyBtn.textContent = t('bubbleCopy')
    }, 1500)
  })
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === 'PING_SELECTION_SCRIPT') {
    sendResponse({ ok: true })
    return
  }

  if (message.type === 'SHOW_SELECTION_TRANSLATION') {
    showSelectionTranslation(message)
      .then(() => sendResponse({ ok: true }))
      .catch((err) => {
        console.warn('[Transly] 划词翻译气泡渲染失败:', err)
        sendResponse({ ok: false, error: String(err) })
      })
    return true
  }
})

// 选区动态划词悬浮图标逻辑（封闭式 Shadow DOM 隔离）
let iconHost: HTMLElement | null = null

function removeHoverIcon() {
  if (iconHost) {
    iconHost.remove()
    iconHost = null
  }
}

document.addEventListener('mouseup', (e) => {
  // 如果点击发生在气泡内或图标内，不干扰
  if (activeHost && activeHost.contains(e.target as Node)) return
  if (iconHost && iconHost.contains(e.target as Node)) return

  const sel = window.getSelection()
  if (!sel || sel.isCollapsed) {
    removeHoverIcon()
    return
  }

  const text = sel.toString().trim()
  if (text.length < 2) {
    removeHoverIcon()
    return
  }

  // 检查是否发生在可编辑区域
  const anchorNode = sel.anchorNode
  if (anchorNode) {
    const parent = anchorNode instanceof Element ? anchorNode : anchorNode.parentElement
    if (parent?.closest('input, textarea, [contenteditable]:not([contenteditable="false"])')) {
      removeHoverIcon()
      return
    }
  }

  const range = sel.getRangeAt(0)
  const rect = range.getBoundingClientRect()
  if (rect.width === 0 && rect.height === 0) {
    removeHoverIcon()
    return
  }

  removeHoverIcon()

  iconHost = document.createElement('div')
  iconHost.id = 'transly-selection-hover-icon'
  iconHost.setAttribute('data-no-translate', 'true')
  iconHost.style.position = 'absolute'
  iconHost.style.zIndex = '2147483646'
  iconHost.style.left = `${window.scrollX + rect.right + 4}px`
  iconHost.style.top = `${Math.max(window.scrollY + 4, window.scrollY + rect.top - 20)}px`

  const iconShadow = iconHost.attachShadow({ mode: 'closed' })
  const btn = document.createElement('div')
  btn.style.width = '22px'
  btn.style.height = '22px'
  btn.style.borderRadius = '50%'
  btn.style.background = '#4f46e5'
  btn.style.color = '#ffffff'
  btn.style.display = 'flex'
  btn.style.alignItems = 'center'
  btn.style.justifyContent = 'center'
  btn.style.cursor = 'pointer'
  btn.style.boxShadow = '0 2px 8px rgba(79, 70, 229, 0.4)'
  btn.style.fontSize = '12px'
  btn.style.fontWeight = 'bold'
  btn.style.userSelect = 'none'
  btn.textContent = '译'

  btn.addEventListener('mousedown', (ev) => {
    ev.preventDefault()
    ev.stopPropagation()
  })

  btn.addEventListener('click', (ev) => {
    ev.preventDefault()
    ev.stopPropagation()
    removeHoverIcon()

    chrome.storage.local.get(['transly_target_lang', 'transly_engine'], (cfg) => {
      showSelectionTranslation({
        text,
        target_lang: cfg.transly_target_lang || 'zh-CN',
        engine: cfg.transly_engine || 'google',
      }).catch((err) => {
        console.warn('[Transly] 点击悬浮图标呼出气泡失败:', err)
      })
    })
  })

  iconShadow.appendChild(btn)
  document.documentElement.appendChild(iconHost)
})

document.addEventListener('selectionchange', () => {
  const sel = window.getSelection()
  if (!sel || sel.isCollapsed) {
    removeHoverIcon()
  }
})
