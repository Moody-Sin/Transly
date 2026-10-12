import assert from 'node:assert/strict'
import test from 'node:test'
import { planBatchRanges, splitByViewport, countChars, type BatchLimits } from './batchPlan.ts'

const LIMITS: BatchLimits = {
  maxBatchItems: 30,
  maxBatchTotalChars: 50_000,
  maxItemChars: 10_000,
}

/** 复算每个区间的字符总量，供断言复用（口径须与服务端一致：码点数） */
function totalOf(texts: string[], start: number, end: number): number {
  let sum = 0
  for (let i = start; i < end; i += 1) sum += countChars(texts[i])
  return sum
}

test('countChars 按码点计数，与 Rust 的 chars().count() 口径一致', () => {
  // ASCII：一个 code point
  assert.equal(countChars('hello'), 5)
  // 中文：一个 code point = 一个 UTF-16 code unit
  assert.equal(countChars('机器学习'), 4)
  // emoji：占 2 个 UTF-16 code unit，但只有 1 个码点
  assert.equal('😀'.length, 2)
  assert.equal(countChars('😀'), 1)
  assert.equal(countChars('😀😀😀'), 3)
  // 星平面字符（如 𠀀）同样只算 1
  assert.equal('𠀀'.length, 2)
  assert.equal(countChars('𠀀'), 1)
  assert.equal(countChars(''), 0)
})

test('码点口径下长 emoji 文本不会被误判超长（回归：6000 个 emoji 曾被前端拒绝）', () => {
  const emoji = '😀'.repeat(6000)
  // UTF-16 长度 12000 > 10000，但码点数 6000 < 10000，服务端可以正常翻译
  assert.ok(emoji.length > LIMITS.maxItemChars)
  assert.ok(countChars(emoji) < LIMITS.maxItemChars)

  const ranges = planBatchRanges([emoji], LIMITS)
  assert.deepEqual(ranges, [[0, 1]], '合法长选区不得被切批逻辑误处理')
})

test('字符总量超限时切成多批（回归：30 项 × 2000 字曾被整批 413 拒绝）', () => {
  const texts = Array.from({ length: 30 }, () => 'a'.repeat(2000))
  const ranges = planBatchRanges(texts, LIMITS)

  assert.ok(ranges.length > 1, '超出字符总量上限必须切成多批')
  for (const [start, end] of ranges) {
    assert.ok(totalOf(texts, start, end) <= LIMITS.maxBatchTotalChars, '每批字符总量不得超限')
    assert.ok(end - start <= LIMITS.maxBatchItems, '每批项数不得超限')
  }
})

test('短文本时由项数上限主导', () => {
  const texts = Array.from({ length: 100 }, () => 'hi')
  assert.deepEqual(planBatchRanges(texts, LIMITS), [
    [0, 30],
    [30, 60],
    [60, 90],
    [90, 100],
  ])
})

test('单条自身超限时独占一批，不静默丢弃', () => {
  const texts = ['a'.repeat(60_000), 'b', 'c']
  const ranges = planBatchRanges(texts, LIMITS)
  assert.deepEqual(ranges[0], [0, 1], '超长项必须独占一批交由服务端报错')
  assert.deepEqual(ranges[1], [1, 3], '其余项正常成批')
})

test('区间连续且完整覆盖，不丢项不重复', () => {
  const texts = Array.from({ length: 77 }, (_, i) => 'x'.repeat(100 + i))
  const ranges = planBatchRanges(texts, LIMITS)

  assert.equal(ranges[0][0], 0)
  assert.equal(ranges[ranges.length - 1][1], texts.length)
  for (let i = 1; i < ranges.length; i += 1) {
    assert.equal(ranges[i - 1][1], ranges[i][0], '区间必须连续')
  }
})

test('边界：空输入、单项、恰好等于上限', () => {
  assert.deepEqual(planBatchRanges([], LIMITS), [])
  assert.deepEqual(planBatchRanges(['x'], LIMITS), [[0, 1]])
  // 恰好等于字符上限时不分批
  assert.deepEqual(planBatchRanges(['a'.repeat(25_000), 'a'.repeat(25_000)], LIMITS), [[0, 2]])
})

test('上限为 0 或负数时退化为每项一批，不产出空区间', () => {
  const texts = ['a', 'b', 'c']
  assert.deepEqual(
    planBatchRanges(texts, { maxBatchItems: 0, maxBatchTotalChars: 0, maxItemChars: 0 }),
    [
      [0, 1],
      [1, 2],
      [2, 3],
    ],
  )
})

test('分流视口批次：首屏优先置顶，离屏保序延后，总集合不漏不重', () => {
  // 模拟待翻译节点项
  const items = [
    { id: '1', inViewport: false, text: 'header offscreen' },
    { id: '2', inViewport: true, text: 'hero title' },
    { id: '3', inViewport: true, text: 'hero subtitle' },
    { id: '4', inViewport: false, text: 'footer offscreen' },
  ]
  const { prioritized, inViewportCount, offscreenCount } = splitByViewport(items)

  assert.equal(inViewportCount, 2)
  assert.equal(offscreenCount, 2)
  assert.equal(prioritized.length, 4)
  assert.deepEqual(
    prioritized.map((i) => i.id),
    ['2', '3', '1', '4'],
    '视口内项目必须排在前部且各自保持原有相对顺序',
  )
})
