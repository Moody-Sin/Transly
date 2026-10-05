import { test } from 'node:test'
import assert from 'node:assert/strict'
import { toFailureOutcome } from './outcome.ts'

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
