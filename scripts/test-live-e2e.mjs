import http from 'node:http'
import net from 'node:net'
import path from 'node:path'
import fs from 'node:fs'
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { chromium } from 'playwright'

const EXTENSION_DIST = path.resolve('extension/dist')
const USER_DATA_DIR = path.resolve('target/playwright-live-user-data')
const TEST_TOKEN = '{{SECRET_live_test_token_8899}}'

async function checkGoogleConnectivity() {
  return new Promise((resolve) => {
    const req = http.get(
      'http://translate.googleapis.com/translate_a/single?client=dict-chrome-ex&sl=en&tl=zh-CN&dt=t&q=hello',
      { timeout: 4000 },
      (res) => {
        resolve(res.statusCode === 200 || res.statusCode === 301 || res.statusCode === 302)
      },
    )
    req.on('error', () => resolve(false))
    req.on('timeout', () => {
      req.destroy()
      resolve(false)
    })
  })
}

async function getAvailablePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port
      server.close(() => resolve(port))
    })
    server.on('error', reject)
  })
}

function startRustRealServer(serverPort) {
  const binName = process.platform === 'win32' ? 'transly_test_server.exe' : 'transly_test_server'
  const candidates = [
    path.resolve('target/debug', binName),
    path.resolve('src-tauri/target/debug', binName),
  ]
  const binaryPath = candidates.find((p) => fs.existsSync(p))
  if (!binaryPath) {
    throw new Error(`未找到 transly_test_server 二进制: ${binaryPath}`)
  }

  return new Promise((resolve, reject) => {
    const child = spawn(binaryPath, [], {
      env: {
        ...process.env,
        TRANSLY_SERVER_PORT: String(serverPort),
        TRANSLY_SERVER_TOKEN: TEST_TOKEN,
        TRANSLY_TEST_REAL_ENGINE: '1',
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
      if (!started) reject(err)
    })

    child.on('exit', (code) => {
      if (!started) reject(new Error(`Rust 服务异常退出 (code: ${code})`))
    })

    setTimeout(() => {
      if (!started) {
        child.kill()
        reject(new Error('Rust 服务启动等待超时 (5s)'))
      }
    }, 5000)
  })
}

const LIVE_PAGE_HTML = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Transly Live E2E Verification</title>
</head>
<body>
  <h1>Transly Real Engine Verification Document</h1>
  <p id="live-p1">The deployment pipeline completed successfully without any interruption.</p>
  <p id="live-p2">Visit our <a id="live-a" href="https://example.com/docs">Official Documentation</a> to learn more.</p>
  <pre><code id="live-code">const token = "live-secret-do-not-translate";</code></pre>
  <textarea id="live-textarea">Good afternoon, welcome to the real machine test.</textarea>
</body>
</html>`

async function testHttpEndpoints(port) {
  console.log('[Live-E2E] 正在验证真实 Rust HTTP 服务鉴权与协议契约...')

  // 1. 测试未授权访问拦截
  const unauthRes = await fetch(`http://127.0.0.1:${port}/translate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      text: 'hello',
      target_lang: 'zh-CN',
      engine: 'google',
    }),
  })
  assert.equal(unauthRes.status, 401, '未携带 Bearer Token 的请求必须被 401 拦截')
  console.log('✔ [Live-E2E] 真实 Rust 服务安全鉴权拦截有效 (401 Unauthorized)')

  // 2. 测试真实 Google 引擎与术语库联动
  const authRes = await fetch(`http://127.0.0.1:${port}/translate`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${TEST_TOKEN}`,
    },
    body: JSON.stringify({
      target_lang: 'zh-CN',
      engine: 'google',
      items: [
        { id: '1', text: 'The continuous integration pipeline is fast.' },
        { id: '2', text: 'Hello, welcome to Transly!' },
      ],
    }),
  })
  assert.equal(authRes.status, 200, '携带有效 Bearer Token 的请求必须返回 200')
  const data = await authRes.json()
  assert(Array.isArray(data.items), '返回必须包含 items 数组')
  assert.equal(data.items.length, 2, '返回项数必须与请求项数一致')

  const item1Text = data.items[0].text
  const item2Text = data.items[1].text
  console.log(`[Live-E2E] 真实网络翻译返回 项1: "${item1Text}", 项2: "${item2Text}"`)

  // 校验术语库: "pipeline" 在 transly_test_server 中配置了替换为 "流水线"
  assert(item1Text.includes('流水线'), `术语库规则必须生效，实际译文: ${item1Text}`)
  // 校验项2包含中文
  assert(
    /[\u4e00-\u9fa5]/.test(item2Text),
    `真实 Google 翻译译文必须包含中文字符，实际: ${item2Text}`,
  )
  console.log('✔ [Live-E2E] 真实 Google Web 翻译引擎与术语库跨网络翻译验证通过')
}

async function runLiveE2E() {
  console.log('====================================================')
  console.log('🚀 开始 Transly 全链路实机端到端 (Live E2E) 综合验证')
  console.log('====================================================')

  const hasNet = await checkGoogleConnectivity()
  if (!hasNet) {
    console.warn('[Live-E2E] ⚠️ 当前主机无法连通 Google 翻译服务，跳过外部网络翻译用例。')
    return
  }
  console.log('✔ 当前实机网络连通正常，支持真实 Google 翻译 API 跨公网交互')

  const targetPort = await getAvailablePort()
  console.log(`[Live-E2E] 启动真实 Rust 测试服务于端口 127.0.0.1:${targetPort}...`)
  const rustProcess = await startRustRealServer(targetPort)
  console.log('✔ 真实 Rust 测试服务就绪 (启用真实 Google 翻译引擎池)')

  let context = null
  let mockPageServer = null

  try {
    // 验证 HTTP 接口与安全层
    await testHttpEndpoints(targetPort)

    // 启动静态页面服务器承载 LIVE_PAGE_HTML
    mockPageServer = http.createServer((req, res) => {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' })
      res.end(LIVE_PAGE_HTML)
    })
    const pagePort = await getAvailablePort()
    await new Promise((resolve) => mockPageServer.listen(pagePort, '127.0.0.1', resolve))
    console.log(`[Live-E2E] 测试目标网页已就绪: http://127.0.0.1:${pagePort}/`)

    // 启动包含真实扩展的 Chromium
    fs.mkdirSync(USER_DATA_DIR, { recursive: true })
    context = await chromium.launchPersistentContext(USER_DATA_DIR, {
      headless: false,
      args: [
        '--headless=new',
        `--disable-extensions-except=${EXTENSION_DIST}`,
        `--load-extension=${EXTENSION_DIST}`,
        '--no-sandbox',
        '--disable-setuid-sandbox',
      ],
    })

    const sw =
      context.serviceWorkers()[0] ||
      (await context.waitForEvent('serviceworker', { timeout: 10000 }))
    assert(sw, '浏览器扩展 Service Worker 必须正常启动')

    // 注入真实后端端口与令牌
    await sw.evaluate(
      async ({ port, token }) => {
        await chrome.storage.local.set({
          transly_server_port: port,
          transly_server_token: token,
          transly_default_engine: 'google',
          transly_default_target_lang: 'zh-CN',
          transly_paired_at: Date.now(),
        })
      },
      { port: targetPort, token: TEST_TOKEN },
    )
    console.log('✔ 浏览器扩展配对参数与端口注入成功')

    const page = await context.newPage()
    await page.goto(`http://127.0.0.1:${pagePort}/`, { waitUntil: 'networkidle' })

    // 触发整页真实翻译
    console.log('[Live-E2E] 正在触发整页沉浸式仅译文翻译（真实公网 API）...')
    await sw.evaluate(async () => {
      const tabs = await chrome.tabs.query({ url: '*://127.0.0.1/*' })
      const tab = tabs[0]
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

    // 等待真实翻译完成并在 DOM 中呈现中文
    await page.waitForFunction(
      () => {
        const p1 = document.getElementById('live-p1')
        return p1 && /[\u4e00-\u9fa5]/.test(p1.innerText)
      },
      { timeout: 15000 },
    )

    const translatedP1 = await page.locator('#live-p1').innerText()
    const translatedA = await page.locator('#live-a').innerText()
    const codeText = await page.locator('#live-code').innerText()

    console.log(`[Live-E2E] 页面段落 1 真实译文: "${translatedP1}"`)
    console.log(`[Live-E2E] 页面超链接真实译文: "${translatedA}"`)

    assert(/[\u4e00-\u9fa5]/.test(translatedP1), '段落 1 必须被真实翻译为中文')
    assert(/[\u4e00-\u9fa5]/.test(translatedA), '超链接文字必须被真实翻译为中文')
    assert.equal(
      codeText,
      'const token = "live-secret-do-not-translate";',
      '代码块必须受免翻保护，不得被翻译',
    )
    console.log('✔ [Live-E2E] 浏览器扩展原地 DOM 翻译、真实网络回填与代码免翻保护通过')

    // 触发一键还原
    console.log('[Live-E2E] 正在触发一键还原为原文...')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
      if (tab?.id) {
        await chrome.tabs.sendMessage(tab.id, {
          type: 'RESTORE_PAGE_TRANSLATION',
        })
      }
    })

    await page.waitForFunction(() => {
      const p1 = document.getElementById('live-p1')
      return p1 && p1.innerText.includes('The deployment pipeline completed')
    })
    console.log('✔ [Live-E2E] 真实网络翻译后一键还原 100% 恢复原文')

    // 触发双语对照模式真实翻译
    console.log('[Live-E2E] 正在触发整页双语对照模式真实翻译...')
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

    await page.waitForSelector('.transly-bilingual-trans', { timeout: 15000 })
    const bilingualNodes = await page.locator('.transly-bilingual-trans').allInnerTexts()
    assert(bilingualNodes.length > 0, '双语模式必须生成至少一个译文节点')
    assert(
      bilingualNodes.some((text) => /[\u4e00-\u9fa5]/.test(text)),
      '双语模式生成的译文节点必须包含中文',
    )
    console.log(`✔ [Live-E2E] 真实双语对照翻译生成 ${bilingualNodes.length} 个译文节点并呈现`)

    // 还原双语对照
    console.log('[Live-E2E] 正在触发双语对照一键还原...')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
      if (tab?.id) {
        await chrome.tabs.sendMessage(tab.id, {
          type: 'RESTORE_PAGE_TRANSLATION',
        })
      }
    })
    await page.waitForTimeout(500)
    const remainingBilingual = await page.locator('.transly-bilingual-trans').count()
    assert.equal(remainingBilingual, 0, '双语模式还原后不得残留任何译文节点')
    console.log('✔ [Live-E2E] 双语对照一键还原成功，兄弟节点彻底清理')

    // 触发输入框原地替换真实测试
    console.log('[Live-E2E] 正在测试网页 textarea 输入框真实原地翻译替换...')
    await page.focus('#live-textarea')
    await sw.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ active: true })
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
        const el = document.getElementById('live-textarea')
        return el && el.value && /[\u4e00-\u9fa5]/.test(el.value)
      },
      { timeout: 15000 },
    )
    const replacedTextarea = await page.locator('#live-textarea').inputValue()
    console.log(`[Live-E2E] 输入框真实替换后内容: "${replacedTextarea}"`)
    assert(/[\u4e00-\u9fa5]/.test(replacedTextarea), '输入框内容必须被真实替换为中文')
    console.log('✔ [Live-E2E] 网页 textarea 输入框真实网络翻译替换成功')

    console.log('\n====================================================')
    console.log('🎉 Transly 全链路实机真实网络端到端验证 100% 通过！')
    console.log('====================================================\n')
  } finally {
    if (context) await context.close()
    if (mockPageServer) mockPageServer.close()
    if (rustProcess) rustProcess.kill()
    try {
      fs.rmSync(USER_DATA_DIR, { recursive: true, force: true })
    } catch {}
  }
}

runLiveE2E().catch((err) => {
  console.error('[Live-E2E] 执行失败:', err)
  process.exit(1)
})
