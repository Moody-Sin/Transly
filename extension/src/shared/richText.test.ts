import assert from 'node:assert/strict'
import test from 'node:test'
import {
  extractPlaceholderTokens,
  buildRichTextComposite,
  restoreRichTextComposite,
  type TextPart,
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

test('富文本闭合占位聚合与行内译文完整还原', () => {
  const parts = [
    { id: 't0', text: 'Please visit ' },
    { id: 't1', text: 'Transly official website' },
    { id: 't2', text: ' to download.' },
  ]
  const composite = buildRichTextComposite(parts, true)
  assert.equal(
    composite.compositeText,
    'Please visit ⟦T_1⟧Transly official website⟦/T_1⟧ to download.',
  )

  const translated = '请访问⟦T_1⟧Transly 官方网站⟦/T_1⟧进行下载。'
  const mapping = new Map([['⟦T_1⟧', 't1']])
  const restored = restoreRichTextComposite(translated, ['t0', 't2'], mapping)

  assert.ok(restored)
  assert.equal(restored.get('t0'), '请访问')
  assert.equal(restored.get('t1'), 'Transly 官方网站', '行内标签内部译文应被准确还原')
  assert.equal(restored.get('t2'), '进行下载。')
})

test('富文本占位缺失降级：若引擎丢失占位符，返回 null 触发安全回退', () => {
  const translated = '请点击立即开始，没有占位符。'
  const mapping = new Map([['⟦T_1⟧', 't1']])
  const restored = restoreRichTextComposite(translated, ['t0', 't2'], mapping)
  assert.equal(restored, null, '丢失占位符时必须返回 null 允许安全降级')
})

test('富文本段落以行内标签开头时准确生成与还原（isInline 显式支持）', () => {
  // <p><a href="...">Google</a> is a powerful search engine.</p>
  const parts = [
    { id: 't0', text: 'Google', isInline: true },
    { id: 't1', text: ' is a powerful search engine.', isInline: false },
  ]
  const composite = buildRichTextComposite(parts, true)
  assert.equal(composite.compositeText, '⟦T_0⟧Google⟦/T_0⟧ is a powerful search engine.')
  assert.deepEqual(composite.baseSlots, [null, 't1'])

  const translated = '⟦T_0⟧谷歌⟦/T_0⟧是一款强大的搜索引擎。'
  const mapping = new Map([['⟦T_0⟧', 't0']])
  const restored = restoreRichTextComposite(translated, composite.baseSlots!, mapping)

  assert.ok(restored)
  assert.equal(restored.get('t0'), '谷歌')
  assert.equal(restored.get('t1'), '是一款强大的搜索引擎。')
})

test('富文本段落以行内标签结尾且包含连续行内标签时准确还原', () => {
  // <p>Visit <b>Site</b><i>Link</i></p>
  const parts = [
    { id: 't0', text: 'Visit ', isInline: false },
    { id: 't1', text: 'Site', isInline: true },
    { id: 't2', text: 'Link', isInline: true },
  ]
  const composite = buildRichTextComposite(parts, true)
  assert.equal(composite.compositeText, 'Visit ⟦T_1⟧Site⟦/T_1⟧⟦T_2⟧Link⟦/T_2⟧')
  assert.deepEqual(composite.baseSlots, ['t0', null, null])

  const translated = '访问⟦T_1⟧网站⟦/T_1⟧⟦T_2⟧链接⟦/T_2⟧'
  const mapping = new Map([
    ['⟦T_1⟧', 't1'],
    ['⟦T_2⟧', 't2'],
  ])
  const restored = restoreRichTextComposite(translated, composite.baseSlots!, mapping)

  assert.ok(restored)
  assert.equal(restored.get('t0'), '访问')
  assert.equal(restored.get('t1'), '网站')
  assert.equal(restored.get('t2'), '链接')
})

test('富文本段落包含多层嵌套行内标签时准确聚合与回填', () => {
  const parts: TextPart[] = [
    { id: 'p0', text: 'Please check ', isInline: false },
    { id: 'p1', text: 'Important Doc', isInline: true },
    { id: 'p2', text: ' and ', isInline: false },
    { id: 'p3', text: 'Notice', isInline: true },
    { id: 'p4', text: '.', isInline: false },
  ]
  const comp = buildRichTextComposite(parts, true)
  assert.equal(comp.compositeText, 'Please check ⟦T_1⟧Important Doc⟦/T_1⟧ and ⟦T_3⟧Notice⟦/T_3⟧.')
  const translated = '请查阅⟦T_1⟧重要文档⟦/T_1⟧与⟦T_3⟧须知⟦/T_3⟧。'
  const restored = restoreRichTextComposite(
    translated,
    comp.baseSlots || [],
    new Map([
      ['⟦T_1⟧', 'p1'],
      ['⟦T_3⟧', 'p3'],
    ]),
  )
  assert.ok(restored)
  assert.equal(restored.get('p0'), '请查阅')
  assert.equal(restored.get('p1'), '重要文档')
  assert.equal(restored.get('p2'), '与')
  assert.equal(restored.get('p3'), '须知')
  assert.equal(restored.get('p4'), '。')
})

test('富文本容错：大模型在占位符括线内插入空格时仍可准确提取与还原', () => {
  const parts: TextPart[] = [
    { id: 't0', text: 'Visit ', isInline: false },
    { id: 't1', text: 'Transly', isInline: true },
    { id: 't2', text: ' now.', isInline: false },
  ]
  const comp = buildRichTextComposite(parts, true)
  assert.equal(comp.compositeText, 'Visit ⟦T_1⟧Transly⟦/T_1⟧ now.')

  // 模拟大模型在特殊括号内插入空格的典型变形
  const translatedWithSpaces = '立即访问 ⟦ T_1 ⟧ 译思 ⟦ /T_1 ⟧ 。'
  const restored = restoreRichTextComposite(
    translatedWithSpaces,
    comp.baseSlots || [],
    new Map([['⟦T_1⟧', 't1']]),
  )
  assert.ok(restored, '带空格占位符必须被成功还原')
  assert.equal(restored.get('t0'), '立即访问 ')
  assert.equal(restored.get('t1'), ' 译思 ')
  assert.equal(restored.get('t2'), ' 。')
})
