const LOCAL_SERVER_URL = 'http://127.0.0.1:39281'

// 获取存储的通信 Token
async function getServerToken(): Promise<string | null> {
  const result = await chrome.storage.local.get(['transly_server_token'])
  return result.transly_server_token || null
}

chrome.runtime.onInstalled.addListener(() => {
  console.log('[Transly Extension] 插件已安装并就绪')
})

// 监听翻译进度事件更新浏览器扩展徽标 Badge
chrome.runtime.onMessage.addListener((message) => {
  if (message.type === 'TRANSLATION_STARTED') {
    chrome.action.setBadgeText({ text: '0%' })
    chrome.action.setBadgeBackgroundColor({ color: '#4f46e5' })
  } else if (message.type === 'TRANSLATION_PROGRESS') {
    const pct = message.total > 0 ? Math.round((message.completed / message.total) * 100) : 0
    chrome.action.setBadgeText({ text: `${pct}%` })
    chrome.action.setBadgeBackgroundColor({ color: message.failedBatches > 0 ? '#f59e0b' : '#4f46e5' })
  } else if (message.type === 'TRANSLATION_FINISHED') {
    chrome.action.setBadgeText({ text: message.failedBatches > 0 ? 'WARN' : 'DONE' })
    chrome.action.setBadgeBackgroundColor({ color: message.failedBatches > 0 ? '#f59e0b' : '#10b981' })
    setTimeout(() => {
      chrome.action.setBadgeText({ text: '' })
    }, 3000)
  } else if (message.type === 'TRANSLATION_CANCELLED' || message.type === 'TRANSLATION_RESTORED') {
    chrome.action.setBadgeText({ text: '' })
  } else if (message.type === 'TRANSLATION_FAILED') {
    chrome.action.setBadgeText({ text: 'ERR' })
    chrome.action.setBadgeBackgroundColor({ color: '#ef4444' })
    setTimeout(() => {
      chrome.action.setBadgeText({ text: '' })
    }, 4000)
  }
})

// 后台转发请求至本地 127.0.0.1 桌面端翻译服务
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
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

