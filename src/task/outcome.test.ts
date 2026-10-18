import { test } from 'node:test'
import assert from 'node:assert/strict'
import { toFailureOutcome, sanitizeCancelledStreamText } from './outcome.ts'

test('IPC 异常回落为可重试的 internal 错误，绝不留在 running', () => {
  const out = toFailureOutcome(new Error('ipc channel closed'))
  assert.equal(out.status, 'error')
  assert.equal(out.error.kind, 'internal')
  assert.equal(out.error.retryable, true, 'IPC 异常属瞬时故障，应允许重试')
  assert.equal(out.error.message, 'ipc channel closed')
})

test('非 Error 异常也安全字符串化', () => {
  const out = toFailureOutcome('boom')
  assert.equal(out.error.message, 'boom')
})

test('sanitizeCancelledStreamText 清洗完整及残缺的内部代码占位符', () => {
  assert.equal(
    sanitizeCancelledStreamText('请看函数：⟦TRANSLY_CODE_0⟧ 说明完成'),
    '请看函数：[代码] 说明完成',
  )
  assert.equal(sanitizeCancelledStreamText('进行中 ⟦ TRANSLY_CODE_1 ⟧ 结尾'), '进行中 [代码] 结尾')
  assert.equal(sanitizeCancelledStreamText('残缺占位 ⟦TRANSLY_CODE_'), '残缺占位 ')
})
