import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  parseThemePreference,
  parseLlmStylePreference,
  DEFAULT_THEME,
  DEFAULT_LLM_STYLE,
  THEME_VALUES,
  LLM_STYLE_VALUES,
} from './prefs.ts'

test('parseThemePreference: 合法值原样通过，非法值回退默认', () => {
  for (const value of THEME_VALUES) {
    assert.equal(parseThemePreference(value), value)
  }
  // 曾经被 localStorage 写坏的值：非空串、大小写漂移、JSON 串、null/undefined
  assert.equal(parseThemePreference('sepia'), DEFAULT_THEME)
  assert.equal(parseThemePreference('Dark'), DEFAULT_THEME)
  assert.equal(parseThemePreference('"dark"'), DEFAULT_THEME)
  assert.equal(parseThemePreference(''), DEFAULT_THEME)
  assert.equal(parseThemePreference(null), DEFAULT_THEME)
  assert.equal(parseThemePreference(undefined), DEFAULT_THEME)
})

test('parseLlmStylePreference: 后端返回非法风格时回退 general，绝不产出非法状态', () => {
  for (const value of LLM_STYLE_VALUES) {
    assert.equal(parseLlmStylePreference(value), value)
  }
  assert.equal(parseLlmStylePreference('poetic'), DEFAULT_LLM_STYLE)
  assert.equal(parseLlmStylePreference('GENERAL'), DEFAULT_LLM_STYLE)
  assert.equal(parseLlmStylePreference(''), DEFAULT_LLM_STYLE)
  assert.equal(parseLlmStylePreference(null), DEFAULT_LLM_STYLE)
  assert.equal(parseLlmStylePreference(undefined), DEFAULT_LLM_STYLE)
})

test('回退默认值本身必须属于合法集合，否则下拉框仍会失配', () => {
  assert.ok(THEME_VALUES.includes(DEFAULT_THEME))
  assert.ok(LLM_STYLE_VALUES.includes(DEFAULT_LLM_STYLE))
})
