const LOCAL_SERVER_URL = 'http://127.0.0.1:39281'

const CONTEXT_MENU_ID_TRANSLATE_SELECTION = 'transly-translate-selection'

/**
 * 服务端批量上限（由 `/health` 公开）。
 *
 * 扩展端原先按「项数 30」切批，而服务端按「字符总量 50000」拒绝：
 * 两者量纲不同，30 个平均 1667 字的节点即触发整批 413，
 * 该批所有节点被标为失败，页面静默无译文。因此上限必须运行时向服务端索取，
 * 不得在本端另写一份常量。
 */
export interface BatchLimits {
  maxBatchItems: number
  maxBatchTotalChars: number
  maxItemChars: number
}

/** 保守默认值：仅在服务端为旧版本（`/health` 未返回上限）时使用 */
const FALLBACK_LIMITS: BatchLimits = {
  maxBatchItems: 30,
  maxBatchTotalChars: 50_000,
  maxItemChars: 10_000,
}

let cachedLimits: BatchLimits | null = null

async function getBatchLimits(): Promise<BatchLimits> {
  if (cachedLimits) return cachedLimits
  try {
    const res = await fetch(`${LOCAL_SERVER_URL}/health`)
    const data = (await res.json()) as Partial<{
      max_batch_items: number
      max_batch_total_chars: number
      max_item_chars: number
    }>
    cachedLimits = {
      maxBatchItems: data.max_batch_items ?? FALLBACK_LIMITS.maxBatchItems,
      maxBatchTotalChars: data.max_batch_total_chars ?? FALLBACK_LIMITS.maxBatchTotalChars,
      maxItemChars: data.max_item_chars ?? FALLBACK_LIMITS.maxItemChars,
    }
  } catch {
    // 服务端未就绪时用保守值；下一次调用会重试，不永久缓存失败结果
    return FALLBACK_LIMITS
  }
  return cachedLimits
}

/**
 * 注入整页翻译脚本。
 *
 * 采用**主动注入**而非 `content_scripts` 常驻声明：
 * - 仅在用户点击“翻译此网页”时注入，符合 `PRIVACY_POLICY.md` 中
 *   “仅在主动触发时提取 DOM 文本”的承诺；
 * - 不再需要 `<all_urls>` 宿主权限，减少商店审核阻力与权限面。
 */
async function ensureContentScriptInjected(tabId: number): Promise<void> {
  try {
    // 重复注入会重置 content script 状态，因此先探测是否已就绪
    await chrome.tabs.sendMessage(tabId, { type: 'PING_CONTENT_SCRIPT' })
    return
  } catch {
    // 未就绪，执行注入
  }
  await chrome.scripting.executeScript({
    target: { tabId },
    files: ['content.js'],
  })
}

/**
 * 注入划词翻译气泡脚本。
 *
 * 与整页翻译同样采用**主动注入**：仅在用户点击右键菜单时注入，
 * 不声明 `content_scripts` 常驻，也不监听 `mouseup`，
 * 维持「仅在主动触发时处理选中文本」的隐私承诺。
 */
async function ensureSelectionScriptInjected(tabId: number): Promise<void> {
  try {
    await chrome.tabs.sendMessage(tabId, { type: 'PING_SELECTION_SCRIPT' })
    return
  } catch {
    // 未就绪，执行注入
  }
  await chrome.scripting.executeScript({
    target: { tabId },
    files: ['selection.js'],
  })
}

// 获取存储的通信 Token
async function getServerToken(): Promise<string | null> {
  const result = await chrome.storage.local.get(['transly_server_token'])
  return result.transly_server_token || null
}

chrome.runtime.onInstalled.addListener(() => {
  console.log('[Transly Extension] 插件已安装并就绪')

  // 菜单标题走 i18n（Chrome 原生），不硬编码文案。
  // 仅注册 `selection` 上下文：菜单只在用户选中文本后出现，
  // 未选中文本时不会出现无效入口。
  //
  // 先 removeAll 再 create：菜单跨浏览器会话持久化，扩展更新时
  // 重复 create 同一 id 会写入 runtime.lastError。
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create(
      {
        id: CONTEXT_MENU_ID_TRANSLATE_SELECTION,
        title: chrome.i18n.getMessage('ctxMenuTranslateSelection'),
        contexts: ['selection'],
      },
      () => void chrome.runtime.lastError,
    )
  })
})

// 右键菜单点击 → 向页面注入气泡脚本并投递待翻译文本。
// 目标语言与引擎沿用 Popup 中保存的配置，与整页翻译保持同一套偏好。
chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId !== CONTEXT_MENU_ID_TRANSLATE_SELECTION) return
  const tabId = tab?.id
  if (typeof tabId !== 'number') return

  void (async () => {
    try {
      await ensureSelectionScriptInjected(tabId)
      const stored = await chrome.storage.local.get(['transly_engine', 'transly_target_lang'])
      await chrome.tabs.sendMessage(tabId, {
        type: 'SHOW_SELECTION_TRANSLATION',
        text: info.selectionText || '',
        target_lang: stored.transly_target_lang || 'zh-CN',
        engine: stored.transly_engine || 'google',
      })
    } catch (err) {
      // 受限页面（chrome://、应用商店等）无法注入，属正常路径；
      // 但不能静默吞掉（Lessons Learned 第 8 条）。
      console.warn('[Transly Extension] 划词翻译气泡注入失败:', err)
    }
  })()
})

interface TaskProgressState {
  generation: number
  status:
    'idle' | 'translating' | 'finished' | 'partially_failed' | 'cancelled' | 'restored' | 'failed'
  completed: number
  total: number
  failedBatches: number
  failedCount?: number
}

// 每个 Tab 独立维护翻译进度快照。
// 标签页关闭时必须清理：Service Worker 生命周期很长，不清理会随使用时长累积。
const tabProgressMap = new Map<number, TaskProgressState>()

chrome.tabs.onRemoved.addListener((tabId) => {
  tabProgressMap.delete(tabId)
})

// 导航到新页面时旧页面的进度已无意义，一并丢弃。
// 注意：已申请 `tabs` 权限（本版本起），`tab.url` 在此可用；
// 该权限仅用于读取 URL 以匹配「为此站点记住自动翻译」规则，不读取页面内容。
chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.status === 'loading') {
    tabProgressMap.delete(tabId)
  }
})

/**
 * 写入徽标。
 *
 * 必须带 tabId：不传 tabId 会写全局徽标，多个标签页同时翻译时互相覆盖。
 * sender.tab 缺失（理论上不会发生）时退化为全局徽标。
 */
function setBadge(tabId: number | undefined, text: string, color?: string) {
  const target = tabId === undefined ? {} : { tabId }
  chrome.action.setBadgeText({ text, ...target })
  if (color) {
    chrome.action.setBadgeBackgroundColor({ color, ...target })
  }
}

// 监听翻译进度事件更新浏览器扩展徽标 Badge 并持久化在内存供 Popup 回显
chrome.runtime.onMessage.addListener((message, sender) => {
  const tabId = sender.tab?.id

  if (message.type === 'TRANSLATION_STARTED') {
    setBadge(tabId, '0%', '#4f46e5')
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'translating',
        completed: 0,
        total: message.total,
        failedBatches: 0,
      })
    }
  } else if (message.type === 'TRANSLATION_PROGRESS') {
    const pct = message.total > 0 ? Math.round((message.completed / message.total) * 100) : 0
    setBadge(tabId, `${pct}%`, message.failedBatches > 0 ? '#f59e0b' : '#4f46e5')
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'translating',
        completed: message.completed,
        total: message.total,
        failedBatches: message.failedBatches,
      })
    }
  } else if (
    message.type === 'TRANSLATION_FINISHED' ||
    message.type === 'TRANSLATION_PARTIAL_FAILED'
  ) {
    const partial = message.type === 'TRANSLATION_PARTIAL_FAILED'
    setBadge(tabId, partial ? 'WARN' : 'DONE', partial ? '#f59e0b' : '#10b981')
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: partial ? 'partially_failed' : 'finished',
        completed: message.completed,
        total: message.total,
        failedBatches: message.failedBatches,
        failedCount: message.failedCount || 0,
      })
    }
    setTimeout(() => {
      setBadge(tabId, '')
    }, 3000)
  } else if (message.type === 'TRANSLATION_CANCELLED') {
    setBadge(tabId, '')
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'cancelled',
        completed: message.completed || 0,
        total: message.total || 0,
        failedBatches: message.failedBatches || 0,
        failedCount: message.failedCount || 0,
      })
    }
  } else if (message.type === 'TRANSLATION_RESTORED') {
    setBadge(tabId, '')
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'restored',
        completed: 0,
        total: 0,
        failedBatches: 0,
        failedCount: 0,
      })
    }
  } else if (message.type === 'TRANSLATION_FAILED') {
    setBadge(tabId, 'ERR', '#ef4444')
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'failed',
        completed: message.completed || 0,
        total: message.total || 0,
        failedBatches: message.failedBatches || 1,
      })
    }
    setTimeout(() => {
      setBadge(tabId, '')
    }, 4000)
  }
})

// 获取或生成持久客户端标识用于配对
async function getOrCreateClientId(): Promise<string> {
  const stored = await chrome.storage.local.get(['transly_client_id'])
  if (stored.transly_client_id) {
    return stored.transly_client_id
  }
  const clientId =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? `ext-${crypto.randomUUID()}`
      : `ext-${Math.random().toString(36).substring(2, 15)}${Date.now().toString(36)}`
  await chrome.storage.local.set({ transly_client_id: clientId })
  return clientId
}

async function performAutoPairing(): Promise<{ ok: boolean; error?: string }> {
  try {
    const clientId = await getOrCreateClientId()
    const reqRes = await fetch(`${LOCAL_SERVER_URL}/pair/request`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        client_id: clientId,
        browser_hint: 'Chrome Extension',
      }),
    })

    if (!reqRes.ok) {
      return { ok: false, error: `发起配对失败 (HTTP ${reqRes.status})` }
    }

    const { session_id, expires_in } = (await reqRes.json()) as {
      session_id: string
      expires_in: number
    }
    const maxPolls = Math.min(expires_in || 60, 60)

    for (let i = 0; i < maxPolls; i++) {
      await new Promise((r) => setTimeout(r, 1000))
      const pollRes = await fetch(`${LOCAL_SERVER_URL}/pair/poll`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          session_id,
          client_id: clientId,
        }),
      })

      if (!pollRes.ok) {
        continue
      }

      const pollData = (await pollRes.json()) as {
        status: 'pending' | 'approved' | 'rejected' | 'expired'
        token?: string
      }

      if (pollData.status === 'approved' && pollData.token) {
        await chrome.storage.local.set({ transly_server_token: pollData.token })
        return { ok: true }
      }

      if (pollData.status === 'rejected') {
        return { ok: false, error: 'rejected' }
      }

      if (pollData.status === 'expired') {
        return { ok: false, error: 'expired' }
      }
    }

    return { ok: false, error: 'timeout' }
  } catch (err) {
    return { ok: false, error: (err as Error).message }
  }
}

// 后台转发请求至本地 127.0.0.1 桌面端翻译服务与状态查询
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === 'REQUEST_AUTO_PAIR') {
    performAutoPairing()
      .then((res) => sendResponse(res))
      .catch((err) => sendResponse({ ok: false, error: err.message }))
    return true
  }

  if (message.type === 'GET_TASK_PROGRESS') {
    const queryTabId = message.tabId
    const state = (queryTabId && tabProgressMap.get(queryTabId)) || null
    sendResponse({ ok: true, state })
    return true
  }

  if (message.type === 'CHECK_DESKTOP_SERVER') {
    fetch(`${LOCAL_SERVER_URL}/health`)
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        return res.json()
      })
      .then((data) => {
        // 如果服务运行中，进一步检验 Token 配对有效性
        getServerToken().then((token) => {
          if (!token) {
            sendResponse({ ok: true, serverRunning: true, paired: false, data })
            return
          }
          // 发起无副作用的 Token 鉴权探针请求
          fetch(`${LOCAL_SERVER_URL}/verify_token`, {
            method: 'GET',
            headers: {
              Authorization: `Bearer ${token}`,
            },
          })
            .then((authRes) => {
              // 只有 2xx 才代表鉴权探针成功。
              // 旧实现把「非 401」一律当作已配对，于是桌面端内部故障（500）、
              // 旧版本没有该路由（404）、权限问题（403）都会被报成「已连接」，
              // Popup 还会解锁翻译按钮，用户点下去才收到失败。
              if (authRes.ok) {
                sendResponse({ ok: true, serverRunning: true, paired: true, data })
              } else if (authRes.status === 401) {
                sendResponse({ ok: true, serverRunning: true, paired: false, tokenExpired: true })
              } else {
                // 服务在跑但探针本身异常：不能报成已配对
                sendResponse({
                  ok: true,
                  serverRunning: true,
                  paired: false,
                  error: `verify_failed_${authRes.status}`,
                })
              }
            })
            .catch(() => {
              // 网络层失败：桌面端可能已退出，不能沿用旧状态报成「已连接」
              sendResponse({ ok: false, serverRunning: false, error: 'network_unreachable' })
            })
        })
      })
      .catch((err) => sendResponse({ ok: false, serverRunning: false, error: err.message }))
    return true
  }

  if (message.type === 'GET_BATCH_LIMITS') {
    getBatchLimits()
      .then((limits) => sendResponse({ ok: true, limits }))
      .catch((err) => sendResponse({ ok: false, error: err.message }))
    return true
  }

  if (message.type === 'ENSURE_CONTENT_SCRIPT') {
    const tabId = message.tabId
    if (typeof tabId !== 'number') {
      sendResponse({ ok: false, error: '缺少 tabId' })
      return true
    }
    ensureContentScriptInjected(tabId)
      .then(() => sendResponse({ ok: true }))
      .catch((err) => sendResponse({ ok: false, error: err.message }))
    return true
  }

  if (message.type === 'TRANSLATE_BATCH') {
    getServerToken().then((token) => {
      fetch(`${LOCAL_SERVER_URL}/translate`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({
          items: message.items,
          source_lang: message.source_lang,
          target_lang: message.target_lang,
          engine: message.engine,
        }),
      })
        .then((res) => {
          if (!res.ok) {
            const err = new Error(`HTTP ${res.status}: ${res.statusText}`)
            // 413 是「批次超限」，与网络故障不同：带上状态码供调用方区分处理
            ;(err as Error & { status?: number }).status = res.status
            throw err
          }
          return res.json()
        })
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err) =>
          sendResponse({
            ok: false,
            error: err.message,
            status: (err as Error & { status?: number }).status,
          }),
        )
    })
    return true
  }
})

// 监听标签页加载完成：若命中站点记忆规则中的 'auto' 且有 Token，则自动触发整页翻译
chrome.tabs.onUpdated.addListener(async (tabId, changeInfo, tab) => {
  if (changeInfo.status === 'complete' && tab.url && tab.url.startsWith('http')) {
    try {
      const url = new URL(tab.url)
      const host = url.hostname
      const cfg = await chrome.storage.local.get([
        'transly_site_rules',
        'transly_target_lang',
        'transly_engine',
        'transly_mode',
        'transly_server_token',
      ])

      const rules = cfg.transly_site_rules || {}
      if (rules[host] === 'auto' && cfg.transly_server_token) {
        await ensureContentScriptInjected(tabId)
        chrome.tabs.sendMessage(tabId, {
          type: 'START_PAGE_TRANSLATION',
          target_lang: cfg.transly_target_lang || 'zh-CN',
          engine: cfg.transly_engine || 'google',
          mode: cfg.transly_mode || 'replace',
        })
      }
    } catch (err) {
      console.warn('[Transly] 站点自动翻译触发异常:', err)
    }
  }
})
