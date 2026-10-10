import { test } from 'node:test'
import assert from 'node:assert/strict'
import { normalizeLanguageCode } from './speech.ts'

test('normalizeLanguageCode: 标准有效语言代码直接转换', () => {
  assert.equal(normalizeLanguageCode('zh'), 'zh-CN')
  assert.equal(normalizeLanguageCode('zh-CN'), 'zh-CN')
  assert.equal(normalizeLanguageCode('zh-TW'), 'zh-TW')
  assert.equal(normalizeLanguageCode('en'), 'en-US')
  assert.equal(normalizeLanguageCode('ja'), 'ja-JP')
  assert.equal(normalizeLanguageCode('ko'), 'ko-KR')
  assert.equal(normalizeLanguageCode('fr'), 'fr-FR')
  assert.equal(normalizeLanguageCode('de'), 'de-DE')
  assert.equal(normalizeLanguageCode('es'), 'es-ES')
  assert.equal(normalizeLanguageCode('ru'), 'ru-RU')
})

test('normalizeLanguageCode: auto 或空语言根据文本特征推断', () => {
  assert.equal(normalizeLanguageCode('auto', '你好世界'), 'zh-CN')
  assert.equal(normalizeLanguageCode(null, 'こんにちは世界'), 'ja-JP')
  assert.equal(normalizeLanguageCode('', '안녕하세요'), 'ko-KR')
  assert.equal(normalizeLanguageCode('auto', 'Привет мир'), 'ru-RU')
  assert.equal(normalizeLanguageCode('auto', 'Hello world'), 'en-US')
})
