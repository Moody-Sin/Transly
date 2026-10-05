const statusEl = document.getElementById('conn-status') as HTMLElement
const btnTranslate = document.getElementById('btn-translate') as HTMLButtonElement
const btnRestore = document.getElementById('btn-restore') as HTMLButtonElement
const inputToken = document.getElementById('input-token') as HTMLInputElement
const btnSaveToken = document.getElementById('btn-save-token') as HTMLButtonElement
const selectEngine = document.getElementById('select-engine') as HTMLSelectElement
const progressBox = document.getElementById('progress-box') as HTMLElement
const progressText = document.getElementById('progress-text') as HTMLElement
const progressCount = document.getElementById('progress-count') as HTMLElement
const progressBar = document.getElementById('progress-bar') as HTMLElement

// 回显已保存的配置
chrome.storage.local.get(['transly_server_token', 'transly_engine'], (res) => {
  if (res.transly_server_token) {
    inputToken.value = res.transly_server_token
  }
  if (res.transly_engine) {
    selectEngine.value = res.transly_engine
  }
})

selectEngine.addEventListener('change', () => {
  chrome.storage.local.set({ transly_engine: selectEngine.value })
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
    if (!response || !response.serverRunning) {
      statusEl.textContent = '○ 桌面端未运行'
      statusEl.style.color = '#ef4444'
      btnTranslate.disabled = true
      return
    }

    if (response.tokenExpired) {
      statusEl.textContent = '▲ Token 已失效，请重新配对'
      statusEl.style.color = '#f59e0b'
      btnTranslate.disabled = true
    } else if (!response.paired) {
      statusEl.textContent = '▲ 未配对 Token'
      statusEl.style.color = '#f59e0b'
      btnTranslate.disabled = true
    } else {
      statusEl.textContent = '● 已连接且配对有效'
      statusEl.style.color = '#10b981'
      btnTranslate.disabled = false
    }
  })
}

// 检查与桌面端 127.0.0.1 服务的连通性
checkConnection()

// 监听来自 content script 的翻译进度广播
chrome.runtime.onMessage.addListener((message) => {
  if (message.type === 'TRANSLATION_STARTED') {
    progressBox.style.display = 'block'
    progressText.textContent = '正在翻译...'
    progressCount.textContent = `0/${message.total}`
    progressBar.style.width = '0%'
  } else if (message.type === 'TRANSLATION_PROGRESS') {
    progressBox.style.display = 'block'
    const pct = message.total > 0 ? Math.round((message.completed / message.total) * 100) : 0
    progressBar.style.width = `${pct}%`
    progressCount.textContent = `${message.completed}/${message.total}`
    if (message.failedBatches > 0) {
      progressText.textContent = `翻译中 (${message.failedBatches} 批失败)`
    } else {
      progressText.textContent = '正在翻译...'
    }
  } else if (message.type === 'TRANSLATION_FINISHED') {
    progressBox.style.display = 'block'
    progressBar.style.width = '100%'
    if (message.failedBatches > 0) {
      progressText.textContent = `已完成 (含 ${message.failedBatches} 批失败)`
    } else {
      progressText.textContent = '翻译完成 ✓'
    }
    progressCount.textContent = `${message.completed}/${message.total}`
    setTimeout(() => {
      progressBox.style.display = 'none'
    }, 2500)
  } else if (message.type === 'TRANSLATION_CANCELLED' || message.type === 'TRANSLATION_RESTORED') {
    progressBox.style.display = 'block'
    progressText.textContent = '已还原 / 取消'
    progressBar.style.width = '0%'
    setTimeout(() => {
      progressBox.style.display = 'none'
    }, 1500)
  }
})

// 触发整页翻译
btnTranslate.addEventListener('click', async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (tab?.id) {
    progressBox.style.display = 'block'
    progressText.textContent = '分析页面...'
    progressBar.style.width = '10%'
    chrome.tabs.sendMessage(tab.id, {
      type: 'START_PAGE_TRANSLATION',
      target_lang: 'zh-CN',
      engine: selectEngine.value,
    })
  }
})

// 触发恢复原文
btnRestore.addEventListener('click', async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (tab?.id) {
    chrome.tabs.sendMessage(tab.id, {
      type: 'RESTORE_PAGE_TRANSLATION',
    })
  }
})
