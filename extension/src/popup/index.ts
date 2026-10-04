const statusEl = document.getElementById('conn-status') as HTMLElement
const btnTranslate = document.getElementById('btn-translate') as HTMLButtonElement
const btnRestore = document.getElementById('btn-restore') as HTMLButtonElement
const inputToken = document.getElementById('input-token') as HTMLInputElement
const btnSaveToken = document.getElementById('btn-save-token') as HTMLButtonElement

// 回显已保存的 Token
chrome.storage.local.get(['transly_server_token'], (res) => {
  if (res.transly_server_token) {
    inputToken.value = res.transly_server_token
  }
})

// 保存 Token 并刷新连接
btnSaveToken.addEventListener('click', () => {
  const token = inputToken.value.trim()
  chrome.storage.local.set({ transly_server_token: token }, () => {
    btnSaveToken.textContent = '已保存 ✓'
    setTimeout(() => {
      btnSaveToken.textContent = '保存配对 Token'
    }, 1500)
    checkConnection()
  })
})

function checkConnection() {
  chrome.runtime.sendMessage({ type: 'CHECK_DESKTOP_SERVER' }, (response) => {
    if (response && response.ok) {
      statusEl.textContent = '● 已连接桌面端'
      statusEl.style.color = '#10b981'
    } else {
      statusEl.textContent = '○ 桌面端未运行'
      statusEl.style.color = '#ef4444'
    }
  })
}

// 检查与桌面端 127.0.0.1 服务的连通性
checkConnection()

// 触发整页翻译
btnTranslate.addEventListener('click', async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (tab?.id) {
    chrome.tabs.sendMessage(tab.id, {
      type: 'START_PAGE_TRANSLATION',
      target_lang: 'zh-CN',
    })
    window.close()
  }
})

// 触发恢复原文
btnRestore.addEventListener('click', async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (tab?.id) {
    chrome.tabs.sendMessage(tab.id, {
      type: 'RESTORE_PAGE_TRANSLATION',
    })
    window.close()
  }
})
