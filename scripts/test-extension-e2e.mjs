import http from 'node:http'
import net from 'node:net'
import path from 'node:path'
import fs from 'node:fs'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { chromium } from 'playwright'

const DEFAULT_SERVER_PORT = 39281
const EXTENSION_DIST = path.resolve('extension/dist')
const USER_DATA_DIR = path.resolve('target/playwright-user-data')
const TEST_TOKEN = 'e2e-valid-token-39281'

async function getAvailablePort(defaultPort = DEFAULT_SERVER_PORT) {
  const isPortAvailable = (port) =>
    new Promise((resolve) => {
      const server = net.createServer()
      server.once('error', () => resolve(false))
      server.once('listening', () => {
        server.close(() => resolve(true))
      })
      server.listen(port, '127.0.0.1')
    })

  if (await isPortAvailable(defaultPort)) {
    return defaultPort
  }
  // 端口冲突时（例如本地已有 Transly 桌面端实例运行），动态申请一个空闲端口
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port
      server.close(() => resolve(port))
    })
    server.on('error', reject)
  })
}

if (!fs.existsSync(EXTENSION_DIST)) {
  console.error(`[E2E] 错误：未找到扩展构建产物目录: ${EXTENSION_DIST}`)
  process.exit(1)
}

const TEST_PAGE_HTML = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Transly E2E Test Target Page</title>
</head>
<body>
  <h1>Transly In-Place Translation Test Document</h1>

  <!-- 场景 1: 普通文本段落 -->
  <p id="simple-para">Hello World, this is a plain text test for full page in-place replacement.</p>

  <!-- 场景 2: 包含行内标签的富文本段落 -->
  <p id="rich-para">Please visit <a id="rich-link" href="https://example.com/target">Example Official Site</a> and check <strong>Crucial Document</strong> before proceeding.</p>

  <!-- 场景 3: 排除与代码块标签 -->
  <div id="code-container">
    <code id="ignored-code">const apiKey = "do-not-translate-code";</code>
    <div class="notranslate" id="ignored-notranslate">This content must never be translated.</div>
  </div>

  <!-- 场景 4: 开放模式 Web Components Shadow DOM -->
  <custom-card id="shadow-host"></custom-card>

  <!-- 场景 5: 输入框与自定义排除区域 -->
  <textarea id="test-textarea">这是一个测试输入框</textarea>
  <div id="custom-exclude" class="user-excluded">Custom excluded text that should not be translated.</div>

  <script>
    class CustomCard extends HTMLElement {
      connectedCallback() {
        const shadow = this.attachShadow({ mode: 'open' });
        shadow.innerHTML = '<div class="shadow-box"><p id="shadow-para">Shadow component paragraph text inside open web component root.</p></div>';
      }
    }
    customElements.define('custom-card', CustomCard);
  </script>
</body>
</html>`

// 尝试启动真实 Rust 本地服务（target/debug/transly_test_server）
function startRustServer(serverPort) {
  // Windows 产物带 .exe 后缀，Linux/macOS 无后缀；
  // 同时兼容工作区根 target/ 与 src-tauri/target/ 两种产物目录布局。
  const binName = process.platform === 'win32' ? 'transly_test_server.exe' : 'transly_test_server'
  const candidates = [
    path.resolve('target/debug', binName),
    path.resolve('src-tauri/target/debug', binName),
  ]
  const binaryPath = candidates.find((p) => fs.existsSync(p))
  if (!binaryPath) {
    return Promise.resolve(null)
  }

  return new Promise((resolve) => {
    const child = spawn(binaryPath, [], {
      env: {
        ...process.env,
        TRANSLY_SERVER_PORT: String(serverPort),
        TRANSLY_SERVER_TOKEN: TEST_TOKEN,
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    })

    let started = false
    child.stdout.on('data', (chunk) => {
      const text = chunk.toString()
      if (text.includes(`READY on port ${serverPort}`)) {
        started = true
        resolve(child)
      }
    })

    child.on('error', (err) => {
      console.warn('[E2E] 启动 Rust 真实测试服务失败，降级使用 Mock 桩:', err.message)
      if (!started) resolve(null)
    })

    child.on('exit', (code) => {
      if (!started) {
        console.warn(`[E2E] Rust 真实测试服务提前退出 (exit: ${code})，降级使用 Mock 桩`)
        resolve(null)
      }
    })

    setTimeout(() => {
      if (!started) {
        console.warn('[E2E] Rust 真实服务启动等待超时，降级使用 Mock 桩')
        resolve(null)
      }
    }, 5000)
  })
}

// 降级用 Mock 服务器（当未编译 Rust 测试二进制时启用）
function startMockServer(serverPort) {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*')
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
      res.setHeader('Access-Control-Allow-Headers', 'Authorization, Content-Type')

      if (req.method === 'OPTIONS') {
        res.writeHead(204)
        res.end()
        return
      }

      const url = new URL(req.url, `http://127.0.0.1:${serverPort}`)

      if (url.pathname === '/test-page.html') {
        res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' })
        res.end(TEST_PAGE_HTML)
        return
      }

      if (url.pathname === '/health') {
        res.writeHead(200, { 'Content-Type': 'application/json' })
        res.end(
          JSON.stringify({
            status: 'ok',
            version: '0.4.1',
            max_batch_items: 30,
            max_batch_total_chars: 50000,
            max_item_chars: 10000,
          }),
        )
        return
      }

      if (url.pathname === '/verify_token') {
        const auth = req.headers['authorization']
        if (auth && auth.startsWith('Bearer ')) {
          res.writeHead(200, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ status: 'ok', version: '0.4.1' }))
        } else {
          res.writeHead(401, { 'Content-Type': 'application/json' })
          res.end(JSON.stringify({ status: 'unauthorized' }))
        }
        return
      }

      if (url.pathname === '/translate' && req.method === 'POST') {
        let body = ''
        req.on('data', (chunk) => {
          body += chunk
        })
        req.on('end', () => {
          try {
            const data = JSON.parse(body)
            const items = (data.items || []).map((item) => {
              let translated = item.text
              if (item.text.includes('⟦T_')) {
                translated = item.text
                  .replace('Please visit ', '请访问 ')
                  .replace(' and check ', ' 并查阅 ')
                  .replace(' before proceeding.', ' 后继续操作。')
                  .replace('Example Official Site', '示例官方站点')
                  .replace('Crucial Document', '重要文档')
              } else {
                translated = `[E2E译文] ${item.text}`
              }

              return {
                status: 'success',
                id: item.id,
                text: translated,
                engine: data.engine || 'google',
              }
            })

            res.writeHead(200, { 'Content-Type': 'application/json' })
            res.end(
              JSON.stringify({
                translations: items.map((i) => i.text),
                items,
              }),
            )
          } catch (e) {
            res.writeHead(400, { 'Content-Type': 'application/json' })
            res.end(JSON.stringify({ error: e.message }))
          }
        })
        return
      }

      res.writeHead(404)
      res.end('Not Found')
    })

    server.listen(serverPort, '127.0.0.1', () => {
      resolve(server)
    })
    server.on('error', reject)
  })
}

async function runTests() {
  console.log('[E2E] 正在初始化测试服务环境...')
  const targetPort = await getAvailablePort(
    process.env.TRANSLY_TEST_PORT ? Number(process.env.TRANSLY_TEST_PORT) : DEFAULT_SERVER_PORT,
  )
  console.log(`[E2E] 测试使用本地服务端口: 127.0.0.1:${targetPort}`)

  let rustProcess = await startRustServer(targetPort)
  let mockServer = null

  // 降级到 Mock 桩时，用例断言与真实服务一致，但覆盖不到真实 Rust 服务的
  // 鉴权、CORS 与切批校验；此时结尾横幅必须如实标注，否则「真实环境 E2E 通过」
  // 会在 Rust 服务启动失败时变成假绿。CI 在跑本脚本前已先编译 transly_test_server，
  // 故 CI 中若落到 Mock 分支即意味着真实服务未能拉起，应引起注意。
  const backendLabel = rustProcess ? '真实环境' : 'Mock 桩降级'

  if (rustProcess) {
    console.log(`[E2E] 成功启动【真实跨进程 Rust 本地 HTTP 服务】127.0.0.1:${targetPort}`)
  } else {
    console.warn(`[E2E] ⚠ 未能启动真实 Rust 服务，已降级为本地 Mock 桩 127.0.0.1:${targetPort}...`)
    mockServer = await startMockServer(targetPort)
    console.warn(`[E2E] ⚠ Mock 桩服务已就绪，本次运行未覆盖真实服务鉴权/CORS/切批校验`)
  }

  fs.mkdirSync(USER_DATA_DIR, { recursive: true })

  // 尝试启动 Chromium 浏览器并挂载已解压的 Transly MV3 插件。
  // 关键：Chrome 137+ 起品牌版 Google Chrome 彻底禁用 --load-extension 开关，
  // 而 Linux runner 预装了 /usr/bin/google-chrome，若优先命中它会导致扩展
  // 根本未加载、waitForEvent('serviceworker') 必然超时。
  // 因此必须优先使用 Playwright 自带的 Chromium（Chrome for Testing），
  // 它 100% 支持 --load-extension；系统浏览器仅作为最后兜底。
  const channels =
    process.platform === 'win32'
      ? [{ channel: 'chromium' }, {}, { channel: 'msedge' }, { channel: 'chrome' }]
      : [{ channel: 'chromium' }, {}, { channel: 'chrome' }, { channel: 'msedge' }]

  let context
  let sw
  for (const ch of channels) {
    let candidateContext
    try {
      candidateContext = await chromium.launchPersistentContext(USER_DATA_DIR, {
        headless: false,
        ...ch,
        args: [
          '--headless=new',
          `--disable-extensions-except=${EXTENSION_DIST}`,
          `--load-extension=${EXTENSION_DIST}`,
          '--no-sandbox',
          '--disable-setuid-sandbox',
        ],
      })
      // 立即验证扩展 Service Worker 是否真的起来了：
      // 未起来的信道（如品牌版 Chrome 忽略 --load-extension）直接关掉换下一个。
      sw =
        candidateContext.serviceWorkers()[0] ||
        (await candidateContext.waitForEvent('serviceworker', { timeout: 10000 }))
      if (sw) {
        context = candidateContext
        break
      }
      await candidateContext.close()
    } catch {
      if (candidateContext) {
        await candidateContext.close().catch(() => {})
      }
      // 尝试下一个可用浏览器信道
    }
  }

  assert(context, '必须成功启动 Chromium 实例')
  assert(sw, '必须成功加载扩展 Service Worker（当前信道可能不支持 --load-extension）')
  console.log('[E2E] 成功启动包含 Transly 浏览器扩展的无头 Chromium 实例')

  try {
    // 1. 等待 Background Service Worker 启动并预置测试 Token（握手阶段已完成就绪判定）

    // 在扩展存储中预先配置已配对有效的 Bearer Token 与默认偏好及测试端口
    await sw.evaluate(
      async ({ token, port }) => {
        await chrome.storage.local.set({
          transly_server_token: token,
          transly_server_port: port,
          transly_engine: 'google',
          transly_target_lang: 'zh-CN',
          transly_mode: 'replace',
        })
      },
      { token: TEST_TOKEN, port: targetPort },
    )
    console.log('✔ [用例 0] 浏览器扩展后台 Service Worker 就绪并预设配对凭据与端口')

    // 2. 打开被测页面（由真实的 Rust 本地服务托管）
    const page = await context.newPage()
    page.on('console', (msg) => console.log('[PAGE CONSOLE]', msg.text()))
    page.on('pageerror', (err) => console.log('[PAGE ERROR]', err.message))
    await page.goto(`http://127.0.0.1:${targetPort}/test-page.html`, { waitUntil: 'networkidle' })

    // 注入嵌套行内标签段落：<p> 的一级子元素是 <strong>/<em>，深层才是 <a>/<span>。
    // 回归点：旧实现只检查一级子元素、只遍历一级文本节点，该段落的深层文本会被整段漏翻。
    await page.evaluate(() => {
      const p = document.createElement('p')
      p.id = 'nested-inline-para'
      p.innerHTML =
        'Please visit <strong><a id="nested-inline-link" href="https://example.com/nested">Example Official Site</a></strong> and check <em><span>Crucial Document</span></em> before proceeding.'
      document.body.appendChild(p)
    })

    // 验证初始原文状态
    const initialSimple = await page.locator('#simple-para').innerText()
    assert.match(initialSimple, /^Hello World/)
    const initialRich = await page.locator('#rich-para').innerText()
    assert.match(initialRich, /^Please visit/)

    // 3. 执行【整页翻译 (仅译文模式)】
    console.log('[E2E] 正在注入 Content Script 并触发整页沉浸式仅译文翻译...')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
      if (tab?.id) {
        await chrome.scripting.executeScript({
          target: { tabId: tab.id },
          files: ['content.js'],
        })
        await chrome.tabs.sendMessage(tab.id, {
          type: 'START_PAGE_TRANSLATION',
          target_lang: 'zh-CN',
          engine: 'google',
          mode: 'replace',
        })
      }
    })

    // 等待翻译完成（DOM 内容被原地替换）
    await page.waitForFunction(
      () => {
        const el = document.getElementById('simple-para')
        return el && el.textContent && el.textContent.includes('[E2E译文]')
      },
      null,
      { timeout: 8000 },
    )

    // 断言 1: 普通段落被原地替换为译文
    const translatedSimple = await page.locator('#simple-para').innerText()
    assert(
      translatedSimple.includes('[E2E译文]'),
      `普通段落必须包含译文，实际内容: ${translatedSimple}`,
    )
    console.log('✔ [用例 1] 真实 DOM 文本节点仅译文原地替换成功')

    // 断言 2: 行内标签富文本段落保真性
    const linkHref = await page.locator('#rich-link').getAttribute('href')
    assert.equal(linkHref, 'https://example.com/target', '<a> 标签 href 属性必须完全保真')
    const richHtml = await page.locator('#rich-para').innerHTML()
    assert(richHtml.includes('<a'), '<a> 嵌套标签必须保留在段落内')
    assert(richHtml.includes('<strong'), '<strong> 嵌套标签必须保留在段落内')
    assert(
      richHtml.includes('请访问') || richHtml.includes('示例官方站点'),
      `富文本段落译文结构正确回填: ${richHtml}`,
    )
    console.log('✔ [用例 2] 包含 <a>、<strong> 的富文本段落占位与标签保真回填成功')

    // 断言 3: 代码块与 notranslate 严格排除未被翻译
    const codeText = await page.locator('#ignored-code').innerText()
    assert.equal(codeText, 'const apiKey = "do-not-translate-code";', 'code 标签内容不得被翻译')
    const noTransText = await page.locator('#ignored-notranslate').innerText()
    assert.equal(
      noTransText,
      'This content must never be translated.',
      '.notranslate 容器不得被翻译',
    )
    console.log('✔ [用例 3] code 与 notranslate 区域严格排除未受污染')

    // 断言 3.5: 嵌套行内标签段落（<strong><a>、<em><span>）深层文本必须被聚合翻译
    const nestedHtml = await page.locator('#nested-inline-para').innerHTML()
    assert(
      nestedHtml.includes('请访问') && nestedHtml.includes('示例官方站点'),
      `嵌套行内标签深层文本必须参与整句翻译，实际 HTML: ${nestedHtml}`,
    )
    assert(
      nestedHtml.includes('重要文档'),
      `<em><span> 二级嵌套文本不得漏翻，实际 HTML: ${nestedHtml}`,
    )
    assert(
      nestedHtml.includes('<a') && nestedHtml.includes('<span') && nestedHtml.includes('<em'),
      `嵌套行内标签结构必须完整保留，实际 HTML: ${nestedHtml}`,
    )
    const nestedHref = await page.locator('#nested-inline-link').getAttribute('href')
    assert.equal(nestedHref, 'https://example.com/nested', '嵌套 <a> 的 href 必须完全保真')
    console.log('✔ [用例 3.5] 嵌套行内标签（<strong><a> / <em><span>）深层文本聚合翻译成功')

    // 断言 4: 开放模式 Shadow DOM 穿透翻译
    const shadowTranslated = await page.evaluate(() => {
      const host = document.getElementById('shadow-host')
      const inner = host?.shadowRoot?.getElementById('shadow-para')
      return inner?.textContent || ''
    })
    assert(
      shadowTranslated.includes('[E2E译文]'),
      `Shadow DOM 内部文本节点必须成功翻译，实际内容: ${shadowTranslated}`,
    )
    console.log('✔ [用例 4] 现代 Web Components 开放 Shadow DOM 穿透翻译成功')

    // 4. 执行【还原原文 (RESTORE)】
    console.log('[E2E] 正在触发还原仅译文...')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
      if (tab?.id) {
        await chrome.tabs.sendMessage(tab.id, {
          type: 'RESTORE_PAGE_TRANSLATION',
        })
      }
    })

    // 等待原文恢复
    await page.waitForFunction(() => {
      const el = document.getElementById('simple-para')
      return el && el.textContent && el.textContent.startsWith('Hello World')
    })
    console.log('✔ [用例 5] 仅译文模式一键还原成功：文本 100% 恢复为原文')

    // 5. 执行【双语对照模式】
    console.log('[E2E] 正在触发整页双语对照模式翻译...')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
      if (tab?.id) {
        await chrome.tabs.sendMessage(tab.id, {
          type: 'START_PAGE_TRANSLATION',
          target_lang: 'zh-CN',
          engine: 'google',
          mode: 'bilingual',
        })
      }
    })

    // 等待双语译文节点生成
    await page.waitForSelector('.transly-bilingual-trans', { timeout: 8000 })
    const bilingualCount = await page.locator('.transly-bilingual-trans').count()
    assert(bilingualCount > 0, '双语模式下必须生成带样式隔离的兄弟译文节点')
    console.log(`✔ [用例 6] 双语对照模式成功生成 ${bilingualCount} 个兄弟译文节点`)

    // 6. 执行【双语模式还原原文】
    console.log('[E2E] 正在触发还原双语对照页面...')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
      if (tab?.id) {
        await chrome.tabs.sendMessage(tab.id, {
          type: 'RESTORE_PAGE_TRANSLATION',
        })
      }
    })

    // 等待双语节点被彻底清除且原文恢复
    await page.waitForFunction(() => {
      return document.querySelectorAll('.transly-bilingual-trans').length === 0
    })
    const restoredSimple = await page.locator('#simple-para').innerText()
    assert(
      restoredSimple.startsWith('Hello World'),
      `还原后普通文本必须完全恢复为原文，实际内容: ${restoredSimple}`,
    )
    console.log('✔ [用例 7] 双语模式一键还原成功：兄弟译文节点彻底清除且原文保持完整')

    // 7. 测试【输入框原地翻译替换】
    console.log('[E2E] 正在触发输入框原地翻译替换...')
    await page.focus('#test-textarea')
    await sw.evaluate(async () => {
      const tabs = await chrome.tabs.query({ url: '*://127.0.0.1/*' })
      const tab = tabs[0]
      if (tab?.id) {
        let pingOk = false
        try {
          const pingRes = await chrome.tabs.sendMessage(tab.id, { type: 'PING_SELECTION_SCRIPT' })
          if (pingRes?.ok) pingOk = true
        } catch {
          pingOk = false
        }

        if (!pingOk) {
          await chrome.scripting.executeScript({
            target: { tabId: tab.id },
            files: ['selection.js'],
          })
        }

        await chrome.tabs.sendMessage(tab.id, {
          type: 'TRIGGER_INPUT_REPLACEMENT',
        })
      }
    })

    await page.waitForFunction(
      () => {
        const el = document.getElementById('test-textarea')
        return el && el.value && el.value !== '这是一个测试输入框'
      },
      { timeout: 8000 },
    )
    const replacedVal = await page.locator('#test-textarea').inputValue()
    assert(
      replacedVal.length > 0 && replacedVal !== '这是一个测试输入框',
      '输入框内容应被成功原地替换为译文',
    )
    console.log('✔ [用例 8] 网页 textarea 输入框原地翻译替换成功')

    // 8. 测试【自定义站点排除选择器】
    console.log('[E2E] 正在测试站点级自定义排除选择器...')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
      if (tab?.id && tab.url) {
        const host = new URL(tab.url).hostname
        await chrome.storage.local.set({
          transly_site_rules: {
            [host]: {
              excludeSelectors: '#custom-exclude, .user-excluded',
            },
          },
        })
        await chrome.tabs.sendMessage(tab.id, {
          type: 'START_PAGE_TRANSLATION',
          target_lang: 'zh-CN',
          engine: 'google',
          mode: 'replace',
        })
      }
    })

    await page.waitForTimeout(1000)
    const customExcludeText = await page.locator('#custom-exclude').innerText()
    assert.equal(
      customExcludeText,
      'Custom excluded text that should not be translated.',
      '命中站点自定义排除选择器的节点绝不得被翻译',
    )
    console.log('✔ [用例 9] 站点自定义 CSS 排除选择器生效，指定区域免翻保护正常')

    console.log('\n========================================')
    console.log(`🎉 全部 11 项浏览器插件${backendLabel}端到端测试 100% 通过！`)
    console.log('========================================\n')
  } finally {
    if (context) await context.close()
    if (mockServer) mockServer.close()
    if (rustProcess) {
      rustProcess.kill()
    }
    try {
      fs.rmSync(USER_DATA_DIR, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 })
    } catch {
      // 容忍 Windows 系统对临时目录锁释放的微小延迟
    }
  }
}

runTests().catch((err) => {
  console.error('[E2E] 测试执行失败:', err)
  process.exit(1)
})
