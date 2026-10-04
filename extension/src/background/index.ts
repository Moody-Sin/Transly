const LOCAL_SERVER_URL = 'http://127.0.0.1:39281'

chrome.runtime.onInstalled.addListener(() => {
  console.log('[Transly Extension] 插件已安装并就绪')
})

// 后台转发请求至本地 127.0.0.1 桌面端翻译服务
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type === 'CHECK_DESKTOP_SERVER') {
    fetch(`${LOCAL_SERVER_URL}/health`)
      .then((res) => res.json())
      .then((data) => sendResponse({ ok: true, data }))
      .catch((err) => sendResponse({ ok: false, error: err.message }))
    return true
  }
})
