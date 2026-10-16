import http from 'node:http'
import path from 'node:path'
import fs from 'node:fs'
import assert from 'node:assert/strict'
import { chromium } from 'playwright'

const DIST_DIR = path.resolve('dist')

if (!fs.existsSync(DIST_DIR) || !fs.existsSync(path.join(DIST_DIR, 'index.html'))) {
  console.error('[UI-E2E] 错误：未找到前端构建产物 dist/index.html，请先运行 pnpm run build')
  process.exit(1)
}

// 启动极轻量的静态文件服务器托管 dist/
function startStaticServer() {
  return new Promise((resolve, reject) => {
    const mimeTypes = {
      '.html': 'text/html; charset=utf-8',
      '.js': 'application/javascript; charset=utf-8',
      '.css': 'text/css; charset=utf-8',
      '.json': 'application/json',
      '.png': 'image/png',
      '.svg': 'image/svg+xml',
    }

    const server = http.createServer((req, res) => {
      const url = new URL(req.url, 'http://127.0.0.1')
      let pathname = url.pathname
      if (pathname === '/' || pathname === '') pathname = '/index.html'

      const filePath = path.join(DIST_DIR, pathname)
      if (!filePath.startsWith(DIST_DIR)) {
        res.writeHead(403)
        res.end('Forbidden')
        return
      }

      fs.readFile(filePath, (err, data) => {
        if (err) {
          // SPA fallback: 若未找到静态资源则回退到 index.html
          fs.readFile(path.join(DIST_DIR, 'index.html'), (err2, indexData) => {
            if (err2) {
              res.writeHead(404)
              res.end('Not Found')
            } else {
              res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' })
              res.end(indexData)
            }
          })
          return
        }

        const ext = path.extname(filePath).toLowerCase()
        const contentType = mimeTypes[ext] || 'application/octet-stream'
        res.writeHead(200, { 'Content-Type': contentType })
        res.end(data)
      })
    })

    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port
      resolve({ server, port })
    })
    server.on('error', reject)
  })
}

// 用于向前端页面注入的 Tauri IPC Mock 脚本
const TAURI_MOCK_INIT_SCRIPT = `
(() => {
  const listeners = new Map();
  let nextListenerId = 1;
  const callbackMap = new Map();
  let nextCallbackId = 1;

  // 模拟的后端持久化状态
  const state = {
    serverToken: "mock-token-secret-12345",
    cacheEnabled: true,
    clipboardMonitor: false,
    shortcut: "Alt+Shift+D",
    replaceShortcut: "Alt+Shift+R",
    preferredReplaceEngine: "google",
    llmStyle: "general",
    apiKeys: {
      deepl: false,
      deepseek: true,
      openai: false,
    },
    llmConfigs: {
      deepseek: { base_url: "https://api.deepseek.com", model: "deepseek-chat" },
      openai: { base_url: "", model: "" }
    },
    glossary: [
      { source: "Transly", target: "全能翻译官", match_case: true },
      { source: "pipeline", target: "流水线", match_case: false }
    ],
    history: [
      {
        cache_key: "history_1",
        source_text: "Continuous Integration",
        translated_text: "持续集成",
        source_lang: "en",
        target_lang: "zh-CN",
        engine: "google",
        // 契约：CacheEntry.created_at 是 Unix 秒级时间戳整数（见 src/bindings.ts）。
        // 曾误传 ISO 字符串，HistoryPanel 会把它乘 1000 得到 NaN 并渲染成
        // Invalid Date，而旧断言只检查文案包含关系，把该缺陷放了过去。
        created_at: Math.floor((Date.now() - 3600000) / 1000)
      },
      {
        cache_key: "history_2",
        source_text: "Artificial Intelligence",
        translated_text: "人工智能",
        source_lang: "en",
        target_lang: "zh-CN",
        engine: "deepl",
        created_at: Math.floor(Date.now() / 1000)
      }
    ],
    isPinned: false
  };

  window.__EMIT_TEST_EVENT = (eventName, payload) => {
    const list = listeners.get(eventName) || [];
    for (const item of list) {
      try {
        if (item.fn) item.fn({ event: eventName, payload, id: item.id });
      } catch (e) {
        console.error('Error in mock event callback:', e);
      }
    }
  };

  window.__GET_MOCK_STATE = () => state;

  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event, eventId) => {
      const list = listeners.get(event) || [];
      listeners.set(event, list.filter(item => item.id !== eventId));
    }
  };

  window.__TAURI_INTERNALS__ = {
    transformCallback: (cb, once = false) => {
      const id = nextCallbackId++;
      const handlerFn = (arg) => {
        if (once) {
          callbackMap.delete(id);
          delete window['_' + id];
        }
        return cb(arg);
      };
      callbackMap.set(id, handlerFn);
      window['_' + id] = handlerFn;
      return id;
    },

    invoke: async (cmd, args = {}) => {
      // 记录调用日志供端到端断言
      if (!window.__INVOKE_LOGS) window.__INVOKE_LOGS = [];
      window.__INVOKE_LOGS.push({ cmd, args });

      // 处理插件调用
      if (cmd === 'plugin:event|listen') {
        const { event, handler } = args;
        const eventId = nextListenerId++;
        const fn = typeof handler === 'function'
          ? handler
          : (callbackMap.get(handler) || window['_' + handler]);
        const list = listeners.get(event) || [];
        list.push({ id: eventId, fn });
        listeners.set(event, list);
        return eventId;
      }

      if (cmd === 'plugin:event|unlisten') {
        const { event, eventId } = args;
        const list = listeners.get(event) || [];
        listeners.set(event, list.filter(item => item.id !== eventId));
        return null;
      }

      if (cmd.startsWith('plugin:window|')) {
        return null;
      }

      // 核心业务命令 Mock（遵循 Tauri 真实 IPC 契约：直接返回数据，typedError 会在外层包裹 { status, data }）
      switch (cmd) {
        case 'detect_text_language':
          return 'en';

        case 'translate': {
          const { text, targetLang, engineName } = args;
          const translated = '[E2E译文:' + (engineName || 'google') + '->' + targetLang + '] ' + text;
          return {
            text: translated,
            engine: engineName || 'google'
          };
        }

        case 'translate_stream': {
          const { text, targetLang, engineName } = args;
          return {
            text: '[E2E流式译文] ' + text,
            engine: engineName || 'google'
          };
        }

        case 'cancel_stream_task':
          return null;

        case 'get_engine_capabilities':
          return {
            id: args.engine || 'google',
            supports_streaming: args.engine === 'openai' || args.engine === 'deepseek',
            supports_glossary: true,
            supports_formality: false,
            max_text_bytes: 50000,
            requires_credential: args.engine !== 'google',
            fallback_engine: null
          };

        case 'get_server_token':
          return state.serverToken;

        case 'rotate_server_token':
          state.serverToken = 'rotated-token-' + Math.random().toString(36).substring(2, 10);
          return state.serverToken;

        case 'get_server_status':
          return {
            state: 'running'
          };

        case 'get_shortcut_status':
          return {
            ok: true,
            reason: null,
            translate_error: null,
            replace_error: null
          };

        case 'get_shortcut':
          return state.shortcut;

        case 'set_shortcut':
          state.shortcut = args.shortcut;
          return null;

        case 'get_replace_shortcut':
          return state.replaceShortcut;

        case 'set_replace_shortcut':
          state.replaceShortcut = args.shortcut;
          return null;

        case 'get_preferred_replace_engine':
          return state.preferredReplaceEngine;

        case 'save_preferred_replace_engine':
          state.preferredReplaceEngine = args.engine;
          return null;

        case 'get_cache_enabled':
          return state.cacheEnabled;

        case 'set_cache_enabled':
          state.cacheEnabled = args.enabled;
          return null;

        case 'clear_cache':
          const count = state.history.length;
          state.history = [];
          return count;

        case 'get_clipboard_monitor':
          return state.clipboardMonitor;

        case 'set_clipboard_monitor':
          state.clipboardMonitor = args.enabled;
          return null;

        case 'is_api_key_configured':
          return !!state.apiKeys[args.engine];

        case 'save_api_key':
          state.apiKeys[args.engine] = true;
          return null;

        case 'delete_api_key':
          state.apiKeys[args.engine] = false;
          return null;

        case 'get_llm_config':
          return state.llmConfigs[args.engine] || { base_url: '', model: '' };

        case 'save_llm_config':
          state.llmConfigs[args.engine] = { base_url: args.baseUrl || '', model: args.model || '' };
          return null;

        case 'get_llm_style':
          return state.llmStyle;

        case 'save_llm_style':
          state.llmStyle = args.style;
          return null;

        case 'get_glossary_terms':
          return state.glossary;

        case 'add_glossary_term':
          state.glossary.push({
            source: args.source,
            target: args.target,
            match_case: args.matchCase ?? true
          });
          return null;

        case 'import_glossary_terms': {
          const items = Array.isArray(args.terms) ? args.terms : [];
          for (const item of items) {
            state.glossary.push({
              source: item.source,
              target: item.target,
              match_case: item.match_case ?? true
            });
          }
          return items.length;
        }

        case 'delete_glossary_term':
          state.glossary = state.glossary.filter(item => item.source !== args.source);
          return null;

        case 'get_translation_history': {
          const { limit = 20, offset = 0 } = args;
          return state.history.slice(offset, offset + limit);
        }

        case 'search_translation_history': {
          const { keyword = '', limit = 20, offset = 0 } = args;
          const filtered = state.history.filter(h =>
            h.source_text.toLowerCase().includes(keyword.toLowerCase()) ||
            h.translated_text.toLowerCase().includes(keyword.toLowerCase())
          );
          return filtered.slice(offset, offset + limit);
        }

        case 'delete_history_entry': {
          const initialLen = state.history.length;
          state.history = state.history.filter(h => h.cache_key !== args.cacheKey);
          return state.history.length < initialLen;
        }

        case 'take_pending_float_request':
          return { has_request: false, text: '', source_lang: null, target_lang: 'zh-CN', engine: null };

        case 'set_float_sticky':
          return null;

        case 'approve_pairing':
          return null;

        case 'reject_pairing':
          return null;

        case 'ocr_clipboard_image':
          return '[OCR识别结果] Image text';

        // 截图翻译：冻结帧用 data: URL 顶替自定义协议（浏览器环境里没有 tauri 协议；
        // vite 构建产物不带 CSP，故 data URL 可用）。
        case 'start_screenshot_translation':
        case 'get_screenshot_session':
          return {
            session_id: 'e2e-shot-session',
            image_url:
              'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==',
            image_width: 1200,
            image_height: 800,
            generation: 7,
            target_lang: 'zh-CN',
            engine: 'google',
          };

        case 'recognize_screenshot_selection':
          return [
            { index: 0, text: '[E2E译文块0]', x: 0.1, y: 0.2, w: 0.3, h: 0.05, line_count: 1, confidence: 0.9 },
            { index: 1, text: '[E2E译文块1]', x: 0.1, y: 0.3, w: 0.3, h: 0.05, line_count: 1, confidence: 0.9 },
          ];

        case 'cancel_screenshot_translation':
          return null;

        default:
          console.warn('[Mock IPC] Unhandled cmd:', cmd, args);
          return null;
      }
    }
  };
})();
`

async function runDesktopUiE2E() {
  console.log('[UI-E2E] 正在启动前端静态托管服务器...')
  const { server, port } = await startStaticServer()
  const baseUrl = `http://127.0.0.1:${port}`
  console.log(`[UI-E2E] 静态托管服务已就绪: ${baseUrl}`)

  const channels =
    process.platform === 'win32'
      ? [{ channel: 'msedge' }, { channel: 'chrome' }, {}]
      : [{ channel: 'chrome' }, { channel: 'msedge' }, {}]

  let browser
  for (const ch of channels) {
    try {
      browser = await chromium.launch({
        headless: true,
        ...ch,
        args: ['--no-sandbox', '--disable-setuid-sandbox'],
      })
      break
    } catch {
      // 降级尝试其他可用浏览器
    }
  }

  assert(browser, '必须成功启动 Chromium 实例')
  console.log('[UI-E2E] 成功启动 Chromium 浏览器实例')

  const consoleErrors = []

  try {
    const page = await browser.newPage()
    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        consoleErrors.push(msg.text())
      }
    })
    page.on('pageerror', (err) => {
      consoleErrors.push(err.message)
    })

    // 预注入 Tauri IPC Mock 环境
    await page.addInitScript(TAURI_MOCK_INIT_SCRIPT)

    // ==========================================
    // 用例 1: 主窗口基础挂载与品牌/UI 元素渲染
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 1: 主窗口基础挂载与导航渲染]...')
    await page.goto(`${baseUrl}/`, { waitUntil: 'networkidle' })

    // 验证标题与主翻译输入框存在
    await page.waitForSelector('textarea', { timeout: 5000 })
    const titleText = await page.textContent('body')
    assert(titleText.includes('Transly'), '页面主视图必须包含 Transly 品牌标题')
    console.log('✔ [用例 1] 主窗口正常挂载，核心 UI 组件就绪')

    // ==========================================
    // 用例 2: 主题切换（系统 / 深色 / 浅色）与 DOM 同步
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 2: 主题切换与 DOM class 同步]...')
    const settingsBtn = page.getByRole('button', { name: '设置' })
    await settingsBtn.waitFor({ state: 'visible', timeout: 5000 })
    await settingsBtn.click()
    await page.waitForSelector('#transly-settings-dialog-title', { timeout: 5000 })

    // 切换到深色模式（通过点击主题模式按钮）
    const darkBtn = page.getByRole('button', { name: '深色' })
    await darkBtn.waitFor({ state: 'visible', timeout: 5000 })
    await darkBtn.click()
    await page.waitForTimeout(200)
    const isDark = await page.evaluate(() => document.documentElement.classList.contains('dark'))
    assert.equal(isDark, true, '切换深色主题后 html 根节点必须包含 dark class')

    const lightBtn = page.getByRole('button', { name: '浅色' })
    await lightBtn.waitFor({ state: 'visible', timeout: 5000 })
    await lightBtn.click()
    await page.waitForTimeout(200)
    const isLight = await page.evaluate(() => !document.documentElement.classList.contains('dark'))
    assert.equal(isLight, true, '切换浅色主题后 html 根节点不得包含 dark class')

    // 关闭设置弹窗
    const closeSettingsBtn = page.locator('button[aria-label="关闭"]').first()
    if (await closeSettingsBtn.isVisible()) {
      await closeSettingsBtn.click()
    } else {
      await page.keyboard.press('Escape')
    }
    await page.waitForSelector('#transly-settings-dialog-title', {
      state: 'detached',
      timeout: 5000,
    })
    console.log('✔ [用例 2] 主题切换与 documentElement class 响应正确')

    // ==========================================
    // 用例 3: 原文输入 -> 触发翻译 -> 译文展示 -> 复制功能
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 3: 原文输入与翻译主链路流转]...')
    const inputArea = page.locator('textarea').first()
    await inputArea.fill('Transly end to end automated verification')

    // 目标语言选择切换（定位包含 zh-CN 选项的 select）
    const targetLangSelect = page
      .locator('select')
      .filter({ has: page.locator('option[value="zh-CN"]') })
      .first()
    await targetLangSelect.waitFor({ state: 'visible', timeout: 5000 })
    await targetLangSelect.selectOption('en')

    // 点击翻译按钮（防抖之外再显式触发一次，确保链路被真实点击驱动）
    const translateButton = page
      .locator('button')
      .filter({ hasText: /^翻译$/ })
      .last()
    await translateButton.waitFor({ state: 'visible', timeout: 5000 })
    assert(await translateButton.isEnabled(), '翻译按钮在有输入时必须可用')
    await translateButton.click()

    // 等待翻译结果区域渲染
    await page.waitForFunction(
      () => {
        const text = document.body.innerText
        return text.includes('[E2E译文:')
      },
      null,
      { timeout: 8000 },
    )

    const renderedResult = await page.textContent('body')
    assert(
      renderedResult.includes('[E2E译文:'),
      `翻译结果区域必须展示译文，实际内容包含: ${renderedResult.substring(0, 200)}`,
    )

    // 复制译文按钮测试
    // 断言必须显式：`if (await x.isVisible())` 在选择器漂移或组件未挂载时只会
    // 静默跳过，用例照样打勾通过，等于把缺陷留给用户。
    const copyButton = page.locator('button').filter({ hasText: /复制/ }).first()
    await copyButton.waitFor({ state: 'visible', timeout: 5000 })
    await copyButton.click()
    await page.waitForTimeout(200)
    console.log('✔ [用例 3] 文本输入、防抖调度、IPC 调用与译文回填全链路正常')

    // ==========================================
    // 用例 4: 历史记录面板（列表渲染、搜索过滤、删除条目、导出 CSV）
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 4: 历史记录面板检索、管理与导出]...')
    const historyTab = page.getByRole('button', { name: '历史' })
    await historyTab.waitFor({ state: 'visible', timeout: 5000 })
    await historyTab.click()
    await page.waitForTimeout(400)

    // 验证历史记录项呈现
    const historyContent = await page.textContent('body')
    assert(
      historyContent.includes('Continuous Integration') || historyContent.includes('持续集成'),
      '历史记录列表应展示初始历史条目',
    )
    // 时间戳必须是可解析的合法时间：Mock 曾误传 ISO 字符串，渲染成 Invalid Date
    // 却因断言只查文案而漏过。此处显式守护契约类型。
    assert(
      !historyContent.includes('Invalid Date'),
      '历史记录时间必须渲染为合法时间（created_at 须为 Unix 秒级整数）',
    )

    // 搜索框过滤测试
    const searchInput = page.locator('input[placeholder*="搜索"], input[type="search"]').first()
    await searchInput.waitFor({ state: 'visible', timeout: 5000 })
    await searchInput.fill('Continuous')
    await page.waitForTimeout(400)
    const filteredContent = await page.textContent('body')
    assert(filteredContent.includes('Continuous Integration'), '搜索结果必须包含匹配条目')

    // 切换回主翻译 Tab
    const translateTab = page.getByRole('button', { name: '翻译' })
    await translateTab.click()
    await page.waitForTimeout(300)
    console.log('✔ [用例 4] 历史记录检索、过滤与交互闭环正常')

    // ==========================================
    // 用例 5: 设置中心全功能（API Key、术语库新增删除、快捷键录制）
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 5: 设置中心与术语库管理]...')
    await settingsBtn.click()
    await page.waitForSelector('#transly-settings-dialog-title', { timeout: 5000 })

    // 术语库输入框测试
    // 选择器必须匹配真实 placeholder（i18n 文案为「源术语 (如: LLM)」）。
    // 旧选择器写的是 `placeholder*="原文"`，从未匹配成功，而外层 `if (isVisible())`
    // 守卫让整段术语库用例静默跳过 —— 这就是典型的「假通过」。
    const glossarySourceInput = page
      .locator('input[placeholder*="源术语"], input[placeholder*="Source term"]')
      .first()
    await glossarySourceInput.waitFor({ state: 'visible', timeout: 5000 })
    await glossarySourceInput.fill('E2ETerm')
    const targetInput = page
      .locator('input[placeholder*="目标译文"], input[placeholder*="Target"]')
      .first()
    await targetInput.waitFor({ state: 'visible', timeout: 5000 })
    await targetInput.fill('端到端术语')
    // 按钮真实文案为「添加术语」（i18n: settings.btnAddGlossary）
    const addGlossaryBtn = page.getByRole('button', { name: '添加术语' })
    await addGlossaryBtn.waitFor({ state: 'visible', timeout: 5000 })
    assert(await addGlossaryBtn.isEnabled(), '添加规则按钮在填入术语后必须可用')
    await addGlossaryBtn.click()
    await page.waitForTimeout(300)

    // 关闭设置弹窗
    const closeBtn2 = page.locator('button[aria-label="关闭"]').first()
    if (await closeBtn2.isVisible()) {
      await closeBtn2.click()
    } else {
      await page.keyboard.press('Escape')
    }
    await page.waitForSelector('#transly-settings-dialog-title', {
      state: 'detached',
      timeout: 5000,
    })
    console.log('✔ [用例 5] 设置对话框、术语库配置与状态存储正常')

    // ==========================================
    // 用例 6: 浏览器扩展一键配对请求浮层 (Pairing Modal)
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 6: 浏览器扩展配对申请弹窗与确认]...')
    // 模拟服务端广播配对申请事件
    await page.evaluate(() => {
      window.__EMIT_TEST_EVENT('transly://pairing-requested', {
        session_id: 'e2e-pairing-session-999',
        browser_hint: 'Chrome 122',
      })
    })

    // 等待配对弹窗出现
    await page.waitForSelector('#transly-pairing-dialog-title', { timeout: 5000 })
    const modalText = await page.textContent('#transly-pairing-dialog-title')
    assert(modalText.includes('配对') || modalText.includes('扩展'), '弹窗标题必须指示配对申请')

    // 点击“允许”按钮
    const approveBtn = page
      .locator('button')
      .filter({ hasText: /允许|确认|同意/ })
      .first()
    await approveBtn.click()
    await page.waitForTimeout(400)

    // 校验 IPC 调用日志中包含 approve_pairing 且携带对应 session_id
    const invokeLogs = await page.evaluate(() => window.__INVOKE_LOGS || [])
    const approvedCall = invokeLogs.find(
      (log) => log.cmd === 'approve_pairing' && log.args?.sessionId === 'e2e-pairing-session-999',
    )
    assert(approvedCall, '必须向后端发起 approve_pairing 并传递正确 sessionId')
    console.log('✔ [用例 6] 浏览器扩展一键配对事件广播、弹窗确认与 IPC 回调成功')

    // ==========================================
    // 用例 7: 划词悬浮窗紧凑模式 (#float) 与选区文本灌入
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 7: 迷你划词悬浮窗 (#float) 紧凑模式]...')
    const floatPage = await browser.newPage()
    await floatPage.addInitScript(TAURI_MOCK_INIT_SCRIPT)
    await floatPage.goto(`${baseUrl}/#float`, { waitUntil: 'networkidle' })

    // 验证浮窗模式正常挂载且无主窗口 Tab
    await floatPage.waitForSelector('[data-tauri-drag-region]', { timeout: 5000 })
    const floatBody = await floatPage.textContent('body')
    assert(!floatBody.includes('历史记录'), '悬浮窗紧凑视图不得展示完整主窗口功能')

    // 模拟后端向浮窗广播 float-translate 选区划词事件
    await floatPage.evaluate(() => {
      window.__EMIT_TEST_EVENT('float-translate', {
        text: 'Selected floating text for quick popup translation',
        source: 'selection',
        request_id: 10086,
      })
    })

    // 验证浮窗接收并自动翻译展示
    await floatPage.waitForFunction(
      () => {
        return document.body.innerText.includes('[E2E译文:')
      },
      null,
      { timeout: 8000 },
    )
    const floatResult = await floatPage.textContent('body')
    assert(floatResult.includes('[E2E译文:'), '悬浮窗必须自动触发划词翻译并展示译文')

    // 置顶图钉切换测试：图钉是悬浮窗的必备控件，缺失即视为缺陷
    const pinBtn = floatPage
      .locator('button[title*="置顶"], button[title*="固定"], button[title*="Pin"]')
      .first()
    await pinBtn.waitFor({ state: 'visible', timeout: 5000 })
    await pinBtn.click()
    await floatPage.waitForTimeout(200)

    await floatPage.close()
    console.log('✔ [用例 7] 迷你悬浮窗 (#float) 路由隔离、选区事件响应与交互成功')

    // ==========================================
    // 用例 8: 截图覆盖窗 (#shot) 框选与选区归一化
    // ==========================================
    console.log('[UI-E2E] 正在测试 [用例 8: 截图覆盖窗 (#shot) 框选交互]...')
    const shotPage = await browser.newPage()
    shotPage.on('console', (msg) => {
      if (msg.type() === 'error') consoleErrors.push(msg.text())
    })
    shotPage.on('pageerror', (err) => consoleErrors.push(err.message))
    await shotPage.addInitScript(TAURI_MOCK_INIT_SCRIPT)
    // 视口固定为 1200×800，与 mock 的 image_width/image_height 一致，
    // 使「物理像素 ↔ 归一化」换算可精确断言。
    await shotPage.setViewportSize({ width: 1200, height: 800 })
    await shotPage.goto(`${baseUrl}/#shot`, { waitUntil: 'networkidle' })

    // 断言 1: 背景冻结帧已加载（src 非空且 complete）
    const shotFrame = shotPage.locator('[data-testid="shot-frame"]')
    await shotFrame.waitFor({ state: 'visible', timeout: 5000 })
    const frameReady = await shotFrame.evaluate(
      (el) => !!el.getAttribute('src') && el.complete && el.naturalWidth > 0,
    )
    assert(frameReady, '覆盖窗必须加载非空且已完成的冻结帧背景图')

    const lastRecognizeArgs = async () => {
      const logs = await shotPage.evaluate(() => window.__INVOKE_LOGS || [])
      const calls = logs.filter((l) => l.cmd === 'recognize_screenshot_selection')
      return calls.length ? calls[calls.length - 1].args.rect : null
    }
    const dragSelect = async (x0, y0, x1, y1) => {
      await shotPage.mouse.move(x0, y0)
      await shotPage.mouse.down()
      await shotPage.mouse.move(x1, y1, { steps: 8 })
      await shotPage.mouse.up()
      await shotPage.waitForTimeout(250)
    }
    const assertRect = (rect, expected, label) => {
      assert(rect, `${label}：必须调用 recognize_screenshot_selection`)
      for (const [key, value] of Object.entries(expected)) {
        assert(
          Math.abs(rect[key] - value) < 0.001,
          `${label}：选区 ${key} 应为 ${value}，实际 ${rect[key]}；完整入参 ${JSON.stringify(rect)}`,
        )
      }
    }

    // 断言 2: 正向框选 (240,160)→(720,480) 应得 {0.2, 0.2, 0.4, 0.4}
    await dragSelect(240, 160, 720, 480)
    assertRect(await lastRecognizeArgs(), { x: 0.2, y: 0.2, w: 0.4, h: 0.4 }, '正向框选')

    // 断言 3: 拖出视口必须被钳制为 x+w ≤ 1、y+h ≤ 1
    await dragSelect(600, 400, 2000, 1500)
    assertRect(await lastRecognizeArgs(), { x: 0.5, y: 0.5, w: 0.5, h: 0.5 }, '越界框选')

    // 断言 4: 反向拖拽必须得到与正向相同的正矩形（两角取 min）
    await dragSelect(800, 600, 400, 300)
    assertRect(
      await lastRecognizeArgs(),
      { x: 400 / 1200, y: 300 / 800, w: 400 / 1200, h: 300 / 800 },
      '反向框选',
    )

    // 断言 5: 识别结果块按归一化坐标渲染成百分比定位
    const shotBlocks = shotPage.locator('[data-testid="shot-block"]')
    assert.strictEqual(await shotBlocks.count(), 2, '识别完成后必须渲染 2 个译文块')
    assert.strictEqual(await shotBlocks.first().evaluate((el) => el.style.left), '10%')
    assert.strictEqual(await shotBlocks.first().evaluate((el) => el.style.top), '20%')
    assert.strictEqual(await shotBlocks.nth(1).evaluate((el) => el.style.top), '30%')

    // 断言 6: 「还原原文」只隐藏译文层，不得卸载背景冻结帧
    await shotPage.getByRole('button', { name: '还原原文' }).click()
    await shotPage.waitForTimeout(150)
    assert.strictEqual(
      await shotPage.locator('[data-testid="shot-block"]').count(),
      0,
      '还原原文后译文层必须不可见',
    )
    assert(await shotFrame.isVisible(), '还原原文不得卸载背景冻结帧')

    // 断言 7: Esc 取消并关闭覆盖层
    await shotPage.keyboard.press('Escape')
    await shotPage.waitForTimeout(200)
    const cancelCalled = await shotPage
      .evaluate(() => window.__INVOKE_LOGS || [])
      .then((logs) => logs.some((l) => l.cmd === 'cancel_screenshot_translation'))
    assert(cancelCalled, 'Esc 必须调用 cancel_screenshot_translation')
    assert.strictEqual(
      await shotPage.locator('[data-testid="shot-frame"]').count(),
      0,
      'Esc 后覆盖层必须消失',
    )

    await shotPage.close()
    console.log('✔ [用例 8] 截图覆盖窗框选、坐标归一化、译文层与取消闭环正常')

    // 检查控制台异常
    assert.equal(
      consoleErrors.length,
      0,
      `端到端测试过程中不得产生任何未捕获控制台错误: ${consoleErrors.join('; ')}`,
    )

    console.log('\n========================================')
    console.log('🎉 全部 8 项桌面端 UI 前端真实渲染与交互 E2E 测试 100% 通过！')
    console.log(
      '⚠️ 覆盖范围声明：截图覆盖窗用例走 Mock IPC（浏览器里前端无法拉起 Tauri 运行时），' +
        '不代表自定义协议 translyshot 链路与真实屏幕捕获已验证；那两项只能靠真机手动验收。',
    )
    console.log('========================================\n')
  } catch (err) {
    console.error('[UI-E2E Catch] 捕获到错误，控制台错误列表:', consoleErrors)
    throw err
  } finally {
    if (browser) await browser.close()
    server.close()
  }
}

runDesktopUiE2E().catch((err) => {
  console.error('[UI-E2E] 测试执行失败:', err)
  process.exit(1)
})
