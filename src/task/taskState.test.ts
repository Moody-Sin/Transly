import { test } from 'node:test'
import assert from 'node:assert/strict'
import { nextState, isTerminal, canRetry, type TaskError } from './taskState.ts'

test('submit 后进入 running 且 running 幂等', () => {
  assert.equal(nextState('idle', 'submit'), 'running')
  assert.equal(nextState('failed', 'submit'), 'running')
  assert.equal(nextState('running', 'submit'), 'running')
})

test('终态不被 cancel 覆盖', () => {
  assert.equal(nextState('succeeded', 'cancel'), 'succeeded')
  assert.equal(nextState('failed', 'cancel'), 'failed')
  assert.equal(nextState('running', 'cancel'), 'cancelled')
})

test('取消后新提交可重新运行', () => {
  assert.equal(nextState('cancelled', 'submit'), 'running')
  assert.equal(nextState('cancelled', 'reset'), 'idle')
})

test('终态判定', () => {
  assert.equal(isTerminal('succeeded'), true)
  assert.equal(isTerminal('failed'), true)
  assert.equal(isTerminal('cancelled'), true)
  assert.equal(isTerminal('running'), false)
  assert.equal(isTerminal('idle'), false)
})

test('仅可重试错误才显示重试入口', () => {
  const retryable: TaskError = { kind: 'timeout', message: 'x', retryable: true }
  const notRetryable: TaskError = { kind: 'auth', message: 'x', retryable: false }
  assert.equal(canRetry('failed', retryable), true)
  assert.equal(canRetry('failed', notRetryable), false, '缺密钥重试无意义')
  assert.equal(canRetry('failed', null), false)
  assert.equal(canRetry('succeeded', retryable), false, '成功态不提供重试')
})
