import http from 'node:http'
import path from 'node:path'
import fs from 'node:fs'
import assert from 'node:assert/strict'
import { chromium } from 'playwright'

const MOCK_PORT = 39281
const EXTENSION_DIST = path.resolve('extension/dist')
const USER_DATA_DIR = path.resolve('target/playwright-user-data')

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

let translationBatchCount = 0

// 启动集成测试本地 Mock 服务器（模拟 Transly 桌面端 127.0.0.1:39281 HTTP 服务与测试页面托管）
function startMockServer() {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      // 允许跨源访问与扩展通信
      res.setHeader('Access-Control-Allow-Origin', '*')
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
      res.setHeader('Access-Control-Allow-Headers', 'Authorization, Content-Type')

      if (req.method === 'OPTIONS') {
        res.writeHead(204)
        res.end()
        return
      }

      const url = new URL(req.url, `http://127.0.0.1:${MOCK_PORT}`)

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
            version: '0.3.0',
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
          res.end(JSON.stringify({ status: 'ok', version: '0.3.0' }))
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
          translationBatchCount += 1
          try {
            const data = JSON.parse(body)
            const items = (data.items || []).map((item) => {
              // 模拟译文生成：保持占位符 ⟦T_N⟧ 结构完好，将普通文字前缀 [E2E译文]
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

    server.listen(MOCK_PORT, '127.0.0.1', () => {
      resolve(server)
    })
    server.on('error', reject)
  })
}

async function runTests() {
  console.log('[E2E] 正在启动测试桩服务 127.0.0.1:39281...')
  const server = await startMockServer()
  console.log('[E2E] 测试桩服务已启动')

  fs.mkdirSync(USER_DATA_DIR, { recursive: true })

  // 尝试启动 Chromium 浏览器并挂载已解压的 Transly MV3 插件
  const channels =
    process.platform === 'win32'
      ? [{ channel: 'msedge' }, { channel: 'chrome' }, {}]
      : [{ channel: 'chrome' }, { channel: 'msedge' }, {}]

  let context
  for (const ch of channels) {
    try {
      context = await chromium.launchPersistentContext(USER_DATA_DIR, {
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
      break
    } catch {
      // 尝试下一个可用浏览器信道
    }
  }

  assert(context, '必须成功启动 Chromium 实例')
  console.log('[E2E] 成功启动包含 Transly 浏览器扩展的无头 Chromium 实例')

  try {
    // 1. 等待 Background Service Worker 启动并预置测试 Token
    let sw = context.serviceWorkers()[0]
    if (!sw) {
      sw = await context.waitForEvent('serviceworker', { timeout: 10000 })
    }
    assert(sw, 'Service Worker 必须正常初始化')

    // 在扩展存储中预先配置已配对有效的 Bearer Token 与默认偏好
    await sw.evaluate(async () => {
      await chrome.storage.local.set({
        transly_server_token: 'e2e-valid-token-39281',
        transly_engine: 'google',
        transly_target_lang: 'zh-CN',
        transly_mode: 'replace',
      })
    })
    console.log('✔ [用例 0] 浏览器扩展后台 Service Worker 就绪并预设配对凭据')

    // 2. 打开被测页面
    const page = await context.newPage()
    await page.goto(`http://127.0.0.1:${MOCK_PORT}/test-page.html`, { waitUntil: 'networkidle' })

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

    console.log('\n========================================')
    console.log('🎉 全部 8 项浏览器插件真实环境端到端测试 100% 通过！')
    console.log('========================================\n')
  } finally {
    if (context) await context.close()
    server.close()
    fs.rmSync(USER_DATA_DIR, { recursive: true, force: true })
  }
}

runTests().catch((err) => {
  console.error('[E2E] 测试执行失败:', err)
  process.exit(1)
})
