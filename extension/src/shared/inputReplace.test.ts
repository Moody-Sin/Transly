import test from 'node:test'
import assert from 'node:assert/strict'
import { resolveInputTargetLang } from './inputReplace.ts'

test('resolveInputTargetLang: 输入中文自动对转为英文', () => {
  assert.equal(resolveInputTargetLang('你好世界'), 'en')
  assert.equal(resolveInputTargetLang('Fix: 修复了这个问题'), 'en')
})

test('resolveInputTargetLang: 输入英文或其他外文自动对转为中文', () => {
  assert.equal(resolveInputTargetLang('Hello world'), 'zh-CN')
  assert.equal(resolveInputTargetLang('こんにちは'), 'zh-CN')
  assert.equal(resolveInputTargetLang('12345'), 'zh-CN')
})

test('resolveInputTargetLang: 显式指定非 auto 偏好时尊重偏好', () => {
  assert.equal(resolveInputTargetLang('Hello', 'ja'), 'ja')
  assert.equal(resolveInputTargetLang('你好', 'de'), 'de')
  assert.equal(resolveInputTargetLang('你好', 'auto'), 'en')
})
