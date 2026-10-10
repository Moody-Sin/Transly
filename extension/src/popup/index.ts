import { applyStaticTranslations, t } from '../i18n'

/**
 * 把后端返回的错误标识映射为本地化文案。
 *
 * 后端在桌面端未启动时会返回原始 fetch 错误串（`Failed to fetch`），
 * 直接显示会让中文界面弹出英文网络错误；因此这里对已知标识做映射，
 * 未知的一律回落到本地化的兜底文案，绝不透传原始英文。
 */
function describeError(errType?: string): string {
  switch (errType) {
    case 'rejected':
      return t('pairingRejected')
    case 'timeout':
      return t('pairingTimeout')
    case 'network_unreachable':
    case 'desktop_unreachable':
      return t('pairingDesktopUnreachable')
    default:
      if (errType && errType.startsWith('verify_failed_')) {
        return t('pairingVerifyFailed')
      }
      return t('pairingFailedGeneric')
  }
}

const statusEl = document.getElementById('conn-status') as HTMLElement
const btnTranslate = document.getElementById('btn-translate') as HTMLButtonElement
const btnCancel = document.getElementById('btn-cancel') as HTMLButtonElement
const btnRetry = document.getElementById('btn-retry') as HTMLButtonElement
const btnRestore = document.getElementById('btn-restore') as HTMLButtonElement
const inputToken = document.getElementById('input-token') as HTMLInputElement
const btnSaveToken = document.getElementById('btn-save-token') as HTMLButtonElement
const btnAutoPair = document.getElementById('btn-auto-pair') as HTMLButtonElement
const pairingMessage = document.getElementById('pairing-message') as HTMLElement
const selectEngine = document.getElementById('select-engine') as HTMLSelectElement
const selectLang = document.getElementById('select-lang') as HTMLSelectElement
const progressBox = document.getElementById('progress-box') as HTMLElement
const progressText = document.getElementById('progress-text') as HTMLElement
const progressCount = document.getElementById('progress-count') as HTMLElement
const progressBar = document.getElementById('progress-bar') as HTMLElement

// 静态文案（按钮、占位符、状态位）统一由 _locales 注入，代码内不硬编码文案
applyStaticTranslations()

// 回显已保存的配置
chrome.storage.local.get(
  ['transly_server_token', 'transly_engine', 'transly_target_lang', 'transly_mode'],
  (res) => {
    if (res.transly_server_token) {
      inputToken.value = res.transly_server_token
    }
    if (res.transly_engine) {
      selectEngine.value = res.transly_engine
    }
    if (res.transly_target_lang && selectLang) {
      selectLang.value = res.transly_target_lang
    }
    const mode = res.transly_mode || 'replace'
    const radio = document.querySelector<HTMLInputElement>(
      `input[name="transly_mode"][value="${mode}"]`,
    )
    if (radio) radio.checked = true
  },
)

document.querySelectorAll<HTMLInputElement>('input[name="transly_mode"]').forEach((el) => {
  el.addEventListener('change', () => {
    if (el.checked) {
      chrome.storage.local.set({ transly_mode: el.value })
    }
  })
})

const checkSiteAuto = document.getElementById('check-site-auto') as HTMLInputElement

// 回显站点自动翻译配置
chrome.tabs.query({ active: true, currentWindow: true }).then(([tab]) => {
  if (tab?.url) {
    try {
      const url = new URL(tab.url)
      const host = url.hostname
      chrome.storage.local.get(['transly_site_rules'], (res) => {
        const rules = res.transly_site_rules || {}
        if (checkSiteAuto) {
          checkSiteAuto.checked = rules[host] === 'auto'
        }
      })
    } catch (err) {
      console.warn('[Transly] 读取站点规则异常:', err)
    }
  }
})

if (checkSiteAuto) {
  checkSiteAuto.addEventListener('change', async () => {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
    if (!tab?.url) return
    try {
      const url = new URL(tab.url)
      const host = url.hostname
      chrome.storage.local.get(['transly_site_rules'], (res) => {
        const rules = res.transly_site_rules || {}
        if (checkSiteAuto.checked) {
          rules[host] = 'auto'
        } else {
          delete rules[host]
        }
        chrome.storage.local.set({ transly_site_rules: rules })
      })
    } catch (err) {
      console.warn('[Transly] 保存站点规则异常:', err)
    }
  })
}

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
    btnSaveToken.textContent = t('btnSavedToken')
    setTimeout(() => {
      btnSaveToken.textContent = t('btnSaveToken')
    }, 1500)
    checkConnection()
  })
})

if (btnAutoPair) {
  btnAutoPair.addEventListener('click', () => {
    btnAutoPair.disabled = true
    btnAutoPair.textContent = t('pairingWaitingDesktop')
    if (pairingMessage) {
      pairingMessage.style.display = 'block'
      pairingMessage.style.color = '#4f46e5'
      pairingMessage.textContent = t('pairingWaitingDesktop')
    }

    chrome.runtime.sendMessage({ type: 'REQUEST_AUTO_PAIR' }, (res) => {
      btnAutoPair.disabled = false
      btnAutoPair.textContent = t('btnAutoPair')

      if (res && res.ok) {
        if (pairingMessage) {
          pairingMessage.style.color = '#10b981'
          pairingMessage.textContent = t('pairingSuccess')
          setTimeout(() => {
            pairingMessage.style.display = 'none'
          }, 3000)
        }
        chrome.storage.local.get(['transly_server_token'], (data) => {
          if (data.transly_server_token) {
            inputToken.value = data.transly_server_token
          }
        })
        checkConnection()
      } else {
        const errType = res?.error
        if (pairingMessage) {
          pairingMessage.style.color = '#ef4444'
          if (errType === 'rejected') {
            pairingMessage.textContent = t('pairingRejected')
          } else if (errType === 'timeout') {
            pairingMessage.textContent = t('pairingTimeout')
          } else {
            // 后端可能返回原始 fetch 错误串（如 "Failed to fetch"），
            // 直接显示会让中文界面弹出英文网络错误，且 _locales 里没有对应 key。
            pairingMessage.textContent = describeError(errType)
          }
        }
      }
    })
  })
}

function checkConnection() {
  chrome.runtime.sendMessage({ type: 'CHECK_DESKTOP_SERVER' }, (response) => {
    if (!response || !response.serverRunning) {
      statusEl.textContent = t('connServerDown')
      statusEl.style.color = '#ef4444'
      btnTranslate.disabled = true
      if (btnAutoPair) btnAutoPair.style.display = 'none'
      return
    }

    if (response.tokenExpired) {
      statusEl.textContent = t('connTokenExpired')
      statusEl.style.color = '#f59e0b'
      btnTranslate.disabled = true
      if (btnAutoPair) btnAutoPair.style.display = 'block'
    } else if (!response.paired) {
      statusEl.textContent = t('connUnpaired')
      statusEl.style.color = '#f59e0b'
      btnTranslate.disabled = true
      if (btnAutoPair) btnAutoPair.style.display = 'block'
    } else {
      statusEl.textContent = t('connPaired')
      statusEl.style.color = '#10b981'
      btnTranslate.disabled = false
      if (btnAutoPair) btnAutoPair.style.display = 'none'
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

  // 无节点可译时 content script 只广播终态、不发 STARTED（total=0）。
  // 若按常规渲染，进度条会从点击时的 10% 回落为 0% 并显示“完成”，
  // 用户观感是“点了没反应”；此处显式区分该终态。
  if (state.status === 'finished' && state.total === 0) {
    progressText.textContent = t('progressNothingToTranslate')
    progressCount.textContent = ''
    progressBar.style.width = '0%'
    return
  }

  if (state.status === 'cancelled') {
    progressText.textContent =
      failedCount > 0
        ? t('progressCancelledRetryable', [String(failedCount)])
        : t('progressCancelled')
  } else if (state.status === 'partially_failed') {
    progressText.textContent = t('progressPartiallyFailed', [String(failedCount)])
  } else if (state.status === 'finished') {
    progressText.textContent = t('progressFinished')
  } else if (state.status === 'failed') {
    const isGoogle =
      !selectEngine.value || selectEngine.value === 'google' || selectEngine.value === 'google_web'
    progressText.textContent = isGoogle
      ? t('progressFailedGoogleNetwork')
      : t('progressFailed', [String(failedCount || state.failedBatches)])
  } else {
    progressText.textContent = t('progressTranslating')
  }
}

async function restoreProgressState() {
  // 进度快照按 tabId 索引，Popup 始终关心当前活动标签页。
  // 仅声明了 activeTab / storage 权限（未申请 tabs），`chrome.tabs.query`
  // 不会返回 url 字段，因此不做 url 过滤，也不做全量标签页回退
  // （那种回退在无 tabs 权限下永远拿不到 url，是死代码）。
  const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true })
  const tabId = activeTab?.id
  if (!tabId) return

  chrome.runtime.sendMessage({ type: 'GET_TASK_PROGRESS', tabId }, (response) => {
    if (response?.ok && response.state) {
      renderProgressState(response.state)
    }
  })
}
restoreProgressState()

/**
 * 确保目标标签页已注入 content script。
 *
 * content script 不再常驻声明（避免 `<all_urls>` 全站注入），
 * 因此所有发给页面的消息都必须先经过这一步。
 */
async function ensureInjected(tabId: number): Promise<boolean> {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: 'ENSURE_CONTENT_SCRIPT', tabId }, (res) => {
      resolve(Boolean(res?.ok))
    })
  })
}

async function sendToActiveTab(message: object) {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (!tab?.id) return
  if (!(await ensureInjected(tab.id))) return
  chrome.tabs.sendMessage(tab.id, message)
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
    renderProgressState({
      status: 'translating',
      completed: 0,
      total: message.total,
      failedBatches: 0,
    })
  } else if (message.type === 'TRANSLATION_PROGRESS') {
    renderProgressState({
      status: 'translating',
      completed: message.completed,
      total: message.total,
      failedBatches: message.failedBatches,
      failedCount: message.failedCount,
    })
  } else if (
    message.type === 'TRANSLATION_FINISHED' ||
    message.type === 'TRANSLATION_PARTIAL_FAILED'
  ) {
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
  if (!tab?.id) return

  progressBox.style.display = 'block'
  progressText.textContent = t('progressAnalyzing')
  progressBar.style.width = '10%'

  // 首次使用该页面时先注入 content script（主动注入，非常驻）
  if (!(await ensureInjected(tab.id))) {
    progressText.textContent = t('cannotAccessPage')
    return
  }

  const targetLang = selectLang ? selectLang.value : 'zh-CN'
  const modeRadio = document.querySelector<HTMLInputElement>('input[name="transly_mode"]:checked')
  const mode = modeRadio ? modeRadio.value : 'replace'

  chrome.tabs.sendMessage(tab.id, {
    type: 'START_PAGE_TRANSLATION',
    target_lang: targetLang,
    engine: selectEngine.value,
    mode,
  })
})

// 触发恢复原文
btnRestore.addEventListener('click', async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true })
  if (tab?.id) {
    // 未注入过则无需恢复
    if (!(await ensureInjected(tab.id))) return
    chrome.tabs.sendMessage(tab.id, {
      type: 'RESTORE_PAGE_TRANSLATION',
    })
  }
})
