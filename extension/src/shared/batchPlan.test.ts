import assert from 'node:assert/strict'
import test from 'node:test'
import { planBatchRanges, type BatchLimits } from './batchPlan.ts'

const LIMITS: BatchLimits = {
  maxBatchItems: 30,
  maxBatchTotalChars: 50_000,
  maxItemChars: 10_000,
}

/** 复算每个区间的字符总量，供断言复用 */
function totalOf(texts: string[], start: number, end: number): number {
  let sum = 0
  for (let i = start; i < end; i += 1) sum += texts[i].length
  return sum
}

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
