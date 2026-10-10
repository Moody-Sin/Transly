import assert from 'node:assert/strict'
import test from 'node:test'
import {
  extractPlaceholderTokens,
  buildRichTextComposite,
  restoreRichTextComposite,
} from './richText.ts'

test('提取占位符 Token 数组', () => {
  const tokens = extractPlaceholderTokens('hello ⟦T_1⟧ world ⟦T_2⟧')
  assert.deepEqual(tokens, ['⟦T_1⟧', '⟦T_2⟧'])
})

test('富文本占位聚合：识别多个行内标签并生成占位段落', () => {
  const parts = [
    { id: 't0', text: 'Please click ' },
    { id: 't1', text: 'here' },
    { id: 't2', text: ' to start now.' },
  ]
  const composite = buildRichTextComposite(parts)
  assert.equal(composite.compositeText, 'Please click ⟦T_1⟧ to start now.')
  assert.equal(composite.placeholders.size, 1)
  assert.equal(composite.placeholders.get('⟦T_1⟧'), 'here')
})

test('富文本译文回填拆分：占位符准确定位并在不同语序下还原结构', () => {
  // 原文: "Please click ⟦T_1⟧ to start." -> 翻译: "请点击⟦T_1⟧立即开始。"
  const translated = '请点击⟦T_1⟧立即开始。'
  const mapping = new Map([['⟦T_1⟧', 't1']])
  const restored = restoreRichTextComposite(translated, ['t0', 't2'], mapping)

  assert.ok(restored)
  assert.equal(restored.get('t0'), '请点击')
  assert.equal(restored.get('t1'), '') // 占位符位置
  assert.equal(restored.get('t2'), '立即开始。')
})

test('富文本占位缺失降级：若引擎丢失占位符，返回 null 触发安全回退', () => {
  const translated = '请点击立即开始，没有占位符。'
  const mapping = new Map([['⟦T_1⟧', 't1']])
  const restored = restoreRichTextComposite(translated, ['t0', 't2'], mapping)
  assert.equal(restored, null, '丢失占位符时必须返回 null 允许安全降级')
})
