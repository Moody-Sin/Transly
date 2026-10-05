const LOCAL_SERVER_URL = 'http://127.0.0.1:39281'

// 获取存储的通信 Token
async function getServerToken(): Promise<string | null> {
  const result = await chrome.storage.local.get(['transly_server_token'])
  return result.transly_server_token || null
}

chrome.runtime.onInstalled.addListener(() => {
  console.log('[Transly Extension] 插件已安装并就绪')
})

interface TaskProgressState {
  generation: number
  status: 'idle' | 'translating' | 'finished' | 'restored' | 'failed'
  completed: number
  total: number
  failedBatches: number
}

// 每个 Tab 独立维护翻译进度快照
const tabProgressMap = new Map<number, TaskProgressState>()

// 监听翻译进度事件更新浏览器扩展徽标 Badge 并持久化在内存供 Popup 回显
chrome.runtime.onMessage.addListener((message, sender) => {
  const tabId = sender.tab?.id

  if (message.type === 'GET_TASK_PROGRESS') {
    return false
  }

  if (message.type === 'TRANSLATION_STARTED') {
    chrome.action.setBadgeText({ text: '0%' })
    chrome.action.setBadgeBackgroundColor({ color: '#4f46e5' })
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
    chrome.action.setBadgeText({ text: `${pct}%` })
    chrome.action.setBadgeBackgroundColor({ color: message.failedBatches > 0 ? '#f59e0b' : '#4f46e5' })
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'translating',
        completed: message.completed,
        total: message.total,
        failedBatches: message.failedBatches,
      })
    }
  } else if (message.type === 'TRANSLATION_FINISHED') {
    chrome.action.setBadgeText({ text: message.failedBatches > 0 ? 'WARN' : 'DONE' })
    chrome.action.setBadgeBackgroundColor({ color: message.failedBatches > 0 ? '#f59e0b' : '#10b981' })
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'finished',
        completed: message.completed,
        total: message.total,
        failedBatches: message.failedBatches,
      })
    }
    setTimeout(() => {
      chrome.action.setBadgeText({ text: '' })
    }, 3000)
  } else if (message.type === 'TRANSLATION_CANCELLED' || message.type === 'TRANSLATION_RESTORED') {
    chrome.action.setBadgeText({ text: '' })
    if (tabId) {
      tabProgressMap.set(tabId, {
        generation: message.generation,
        status: 'restored',
        completed: 0,
        total: 0,
        failedBatches: 0,
      })
    }
  } else if (message.type === 'TRANSLATION_FAILED') {
    chrome.action.setBadgeText({ text: 'ERR' })
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' })
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
      chrome.action.setBadgeText({ text: '' })
    }, 4000)
  }
})

// 后台转发请求至本地 127.0.0.1 桌面端翻译服务与状态查询
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
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
          // 发起试探鉴权请求
          fetch(`${LOCAL_SERVER_URL}/translate`, {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              Authorization: `Bearer ${token}`,
            },
            body: JSON.stringify({
              texts: ['test'],
              target_lang: 'zh-CN',
            }),
          })
            .then((authRes) => {
              if (authRes.status === 401) {
                sendResponse({ ok: true, serverRunning: true, paired: false, tokenExpired: true })
              } else {
                sendResponse({ ok: true, serverRunning: true, paired: true, data })
              }
            })
            .catch(() => {
              sendResponse({ ok: true, serverRunning: true, paired: true, data })
            })
        })
      })
      .catch((err) => sendResponse({ ok: false, serverRunning: false, error: err.message }))
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
          texts: message.texts,
          source_lang: message.source_lang,
          target_lang: message.target_lang,
          engine: message.engine,
        }),
      })
        .then((res) => {
          if (!res.ok) {
            throw new Error(`HTTP ${res.status}: ${res.statusText}`)
          }
          return res.json()
        })
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err) => sendResponse({ ok: false, error: err.message }))
    })
    return true
  }

})

