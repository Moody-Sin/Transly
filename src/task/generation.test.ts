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
