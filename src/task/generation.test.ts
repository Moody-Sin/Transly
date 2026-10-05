import { test } from 'node:test'
import assert from 'node:assert/strict'
import { applyIfCurrent } from './generation.ts'

test('同代结果放行', () => {
  assert.equal(applyIfCurrent(3, 3, 'ok'), 'ok')
})

test('迟到结果被丢弃：旧代结果绝不覆盖新代', () => {
  // 用户取消或新输入后，第 1 代响应回来必须被丢弃
  assert.equal(applyIfCurrent(1, 2, 'stale'), null)
})

test('取消后第 1 代迟到错误同样被丢弃', () => {
  assert.equal(applyIfCurrent(4, 5, 'stale-error'), null)
})

test('新输入立即作废上一代：连续三次输入只允许最新一代落地', () => {
  // 模拟防抖期间的连续输入
  let current = 0
  current += 1; const g1 = current
  current += 1; const g2 = current
  current += 1; const g3 = current
  assert.equal(applyIfCurrent(g1, g3, 'first'), null)
  assert.equal(applyIfCurrent(g2, g3, 'second'), null)
  assert.equal(applyIfCurrent(g3, g3, 'third'), 'third')
})
