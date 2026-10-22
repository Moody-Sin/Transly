import { test } from 'node:test'
import assert from 'node:assert/strict'
import { normalizeLanguageCode, stopSpeaking, isSpeaking, speakText } from './speech.ts'

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

test('speech utils: 无 window 环境下防崩溃', () => {
  assert.doesNotThrow(() => stopSpeaking())
  assert.equal(isSpeaking(), false)
  assert.doesNotThrow(() => speakText('test'))
})

test('speech utils: mock 环境下回调与生命周期正常流转', () => {
  let started = false
  let ended = false
  let spokenUtterance: { onstart?: () => void; onend?: () => void; lang?: string } | null = null

  // 模拟全局 window 与 speechSynthesis
  const originalWindow = (globalThis as unknown as { window: unknown }).window
  const mockSynthesis = {
    speaking: true,
    cancel: () => {},
    speak: (u: unknown) => {
      spokenUtterance = u as typeof spokenUtterance
    },
  }
  class MockSpeechSynthesisUtterance {
    lang = ''
    text = ''
    onstart?: () => void
    onend?: () => void
    onerror?: (e: unknown) => void
    constructor(text: string) {
      this.text = text
    }
  }

  ;(globalThis as unknown as { window: unknown }).window = {
    speechSynthesis: mockSynthesis,
  }
  ;(globalThis as unknown as { SpeechSynthesisUtterance: unknown }).SpeechSynthesisUtterance =
    MockSpeechSynthesisUtterance

  try {
    assert.equal(isSpeaking(), true)
    speakText('Hello Transly', 'en', null, {
      onStart: () => {
        started = true
      },
      onEnd: () => {
        ended = true
      },
    })

    assert.ok(spokenUtterance)
    const utt = spokenUtterance as { onstart?: () => void; onend?: () => void }
    utt.onstart?.()
    assert.equal(started, true)
    utt.onend?.()
    assert.equal(ended, true)

    assert.doesNotThrow(() => stopSpeaking())
  } finally {
    ;(globalThis as unknown as { window: unknown }).window = originalWindow
    delete (globalThis as unknown as { SpeechSynthesisUtterance?: unknown })
      .SpeechSynthesisUtterance
  }
})
