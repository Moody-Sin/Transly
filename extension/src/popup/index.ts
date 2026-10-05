const statusEl = document.getElementById('conn-status') as HTMLElement
const btnTranslate = document.getElementById('btn-translate') as HTMLButtonElement
const btnCancel = document.getElementById('btn-cancel') as HTMLButtonElement
const btnRetry = document.getElementById('btn-retry') as HTMLButtonElement
const btnRestore = document.getElementById('btn-restore') as HTMLButtonElement
const inputToken = document.getElementById('input-token') as HTMLInputElement
const btnSaveToken = document.getElementById('btn-save-token') as HTMLButtonElement
const selectEngine = document.getElementById('select-engine') as HTMLSelectElement
const selectLang = document.getElementById('select-lang') as HTMLSelectElement
const progressBox = document.getElementById('progress-box') as HTMLElement
const progressText = document.getElementById('progress-text') as HTMLElement
const progressCount = document.getElementById('progress-count') as HTMLElement
const progressBar = document.getElementById('progress-bar') as HTMLElement

// 回显已保存的配置
chrome.storage.local.get(['transly_server_token', 'transly_engine', 'transly_target_lang'], (res) => {
  if (res.transly_server_token) {
    inputToken.value = res.transly_server_token
  }
  if (res.transly_engine) {
    selectEngine.value = res.transly_engine
  }
  if (res.transly_target_lang && selectLang) {
    selectLang.value = res.transly_target_lang
  }
})

selectEngine.addEventListener('change', () => {
  chrome.storage.local.set({ transly_engine: selectEngine.value })
})

if (selectLang) {
  selectLang.addEventListener('change', () => {
    chrome.storage.local.set({ transly_target_lang: selectLang.value })
  })
}

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

function renderProgressState(state: {
  status: string
  completed: number
  total: number
  failedBatches: number
  failedCount?: number
}) {
  progressBox.style.display = 'block'
  const failedCount = state.failedCount || 0
  const pct = state.total > 0 ? Math.round((state.completed / state.total) * 100) : 0
  progressBar.style.width = `${pct}%`
  progressCount.textContent = `${state.completed}/${state.total}`
  btnCancel.style.display = state.status === 'translating' ? 'block' : 'none'
  btnRetry.style.display = failedCount > 0 && state.status !== 'translating' ? 'block' : 'none'

  if (state.status === 'cancelled') {
    progressText.textContent = `已取消${failedCount > 0 ? `，${failedCount} 项可重试` : ''}`
  } else if (state.status === 'partially_failed') {
    progressText.textContent = `部分完成，${failedCount} 项失败`
  } else if (state.status === 'finished') {
    progressText.textContent = '翻译完成 ✓'
  } else if (state.status === 'failed') {
    progressText.textContent = `翻译失败，${failedCount || state.failedBatches} 项可重试`
  } else {
    progressText.textContent = '正在翻译...'
  }
}


async function restoreProgressState() {
  // 兼容直接作为 popup 打开或独立页面调试
  let tabId: number | undefined
  const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (activeTab?.id && !activeTab.url?.startsWith('chrome-extension://')) {
    tabId = activeTab.id
  } else {
    // 降级查询最近的正常网页标签
    const allTabs = await chrome.tabs.query({})
    const webTab = allTabs.find((t) => t.id && t.url && !t.url.startsWith('chrome-extension://'))
    tabId = webTab?.id
  }

  if (!tabId) return

  chrome.runtime.sendMessage({ type: 'GET_TASK_PROGRESS', tabId }, (response) => {
    if (response?.ok && response.state) {
      renderProgressState(response.state)
    }
  })
}
restoreProgressState()

async function sendToActiveTab(message: object) {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (tab?.id) chrome.tabs.sendMessage(tab.id, message)
}

btnCancel.addEventListener('click', () => {
  sendToActiveTab({ type: 'CANCEL_PAGE_TRANSLATION' })
})

btnRetry.addEventListener('click', () => {
  btnRetry.style.display = 'none'
  sendToActiveTab({ type: 'RETRY_FAILED_TRANSLATION' })
})

// 监听来自 content script 的翻译进度广播
chrome.runtime.onMessage.addListener((message) => {
  if (message.type === 'TRANSLATION_STARTED') {
    renderProgressState({ status: 'translating', completed: 0, total: message.total, failedBatches: 0 })
  } else if (message.type === 'TRANSLATION_PROGRESS') {
    renderProgressState({
      status: 'translating',
      completed: message.completed,
      total: message.total,
      failedBatches: message.failedBatches,
      failedCount: message.failedCount,
    })
  } else if (message.type === 'TRANSLATION_FINISHED' || message.type === 'TRANSLATION_PARTIAL_FAILED') {
    renderProgressState({
      status: message.type === 'TRANSLATION_PARTIAL_FAILED' ? 'partially_failed' : 'finished',
      completed: message.completed,
      total: message.total,
      failedBatches: message.failedBatches,
      failedCount: message.failedCount,
    })
    if (message.type === 'TRANSLATION_FINISHED') {
      setTimeout(() => {
        progressBox.style.display = 'none'
      }, 2500)
    }
  } else if (message.type === 'TRANSLATION_FAILED') {
    renderProgressState({
      status: 'failed',
      completed: message.completed || 0,
      total: message.total || 0,
      failedBatches: message.failedBatches || 1,
      failedCount: message.failedCount,
    })
  } else if (message.type === 'TRANSLATION_CANCELLED' || message.type === 'TRANSLATION_RESTORED') {
    renderProgressState({
      status: message.type === 'TRANSLATION_CANCELLED' ? 'cancelled' : 'restored',
      completed: message.completed || 0,
      total: message.total || 0,
      failedBatches: message.failedBatches || 0,
      failedCount: message.failedCount,
    })
    if (message.type === 'TRANSLATION_RESTORED') {
      setTimeout(() => {
        progressBox.style.display = 'none'
      }, 1500)
    }
  }
})

// 触发整页翻译
btnTranslate.addEventListener('click', async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (tab?.id) {
    progressBox.style.display = 'block'
    progressText.textContent = '分析页面...'
    progressBar.style.width = '10%'
    const targetLang = selectLang ? selectLang.value : 'zh-CN'
    chrome.tabs.sendMessage(tab.id, {
      type: 'START_PAGE_TRANSLATION',
      target_lang: targetLang,
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
