import test from 'node:test'
import assert from 'node:assert/strict'
import { normalizeSiteRule } from './siteRules.ts'

test('normalizeSiteRule: 兼容历史纯字符串 auto 配置', () => {
  const rule = normalizeSiteRule('auto')
  assert.equal(rule.auto, true)
  assert.equal(rule.targetLang, undefined)
  assert.equal(rule.excludeSelectors, undefined)
  assert.equal(rule.disableInputAssistant, false)
})

test('normalizeSiteRule: 结构化配置正确解析与缺省规范化', () => {
  const rule = normalizeSiteRule({
    auto: true,
    targetLang: 'en',
    engine: 'deepseek',
    excludeSelectors: '.code, pre',
    disableInputAssistant: true,
  })
  assert.equal(rule.auto, true)
  assert.equal(rule.targetLang, 'en')
  assert.equal(rule.engine, 'deepseek')
  assert.equal(rule.excludeSelectors, '.code, pre')
  assert.equal(rule.disableInputAssistant, true)
})

test('normalizeSiteRule: 空值与非法输入回退默认空配置', () => {
  assert.deepEqual(normalizeSiteRule(null), { auto: false, disableInputAssistant: false })
  assert.deepEqual(normalizeSiteRule(undefined), { auto: false, disableInputAssistant: false })
})
