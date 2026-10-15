import test from 'node:test'
import assert from 'node:assert/strict'
import { normalizeLoopbackServerUrl, normalizeServerPort } from './serverUrl.ts'

test('normalizeLoopbackServerUrl: 允许合法回环地址', () => {
  assert.equal(normalizeLoopbackServerUrl('http://127.0.0.1:39281'), 'http://127.0.0.1:39281')
  assert.equal(normalizeLoopbackServerUrl('http://localhost:18273'), 'http://localhost:18273')
  assert.equal(normalizeLoopbackServerUrl('http://[::1]:39281'), 'http://[::1]:39281')
  assert.equal(
    normalizeLoopbackServerUrl('http://127.0.0.1:39281/translate'),
    'http://127.0.0.1:39281',
  )
})

test('normalizeLoopbackServerUrl: 拦截非回环与恶意地址', () => {
  // 局域网地址
  assert.equal(normalizeLoopbackServerUrl('http://192.168.1.100:39281'), null)
  // 伪装域名与外网域名
  assert.equal(normalizeLoopbackServerUrl('http://127.0.0.1.evil.com:39281'), null)
  assert.equal(normalizeLoopbackServerUrl('http://localhost.evil.com'), null)
  assert.equal(normalizeLoopbackServerUrl('http://evil.com'), null)
  // 非 http 协议
  assert.equal(normalizeLoopbackServerUrl('https://127.0.0.1:39281'), null)
  assert.equal(normalizeLoopbackServerUrl('javascript:alert(1)'), null)
  // 空值或非字符串
  assert.equal(normalizeLoopbackServerUrl(''), null)
  assert.equal(normalizeLoopbackServerUrl('   '), null)
  assert.equal(normalizeLoopbackServerUrl(null), null)
  assert.equal(normalizeLoopbackServerUrl(undefined), null)
})

test('normalizeServerPort: 校验端口合法性', () => {
  assert.equal(normalizeServerPort(39281), 39281)
  assert.equal(normalizeServerPort('39281'), 39281)
  assert.equal(normalizeServerPort(' 8080 '), 8080)
  assert.equal(normalizeServerPort(0), null)
  assert.equal(normalizeServerPort(70000), null)
  assert.equal(normalizeServerPort(-1), null)
  assert.equal(normalizeServerPort('abc'), null)
  assert.equal(normalizeServerPort('12a'), null)
})
