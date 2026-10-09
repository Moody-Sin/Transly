import test from 'node:test'
import assert from 'node:assert/strict'
import { planBatchRanges } from '../../extension/src/shared/batchPlan.ts'
import { nextState, canRetryExecution } from './taskState.ts'

test('E2E: 浏览器插件整页分块切批 -> 任务状态机完整闭环流程', () => {
  // 1. 模拟浏览器页面收集到了长短不一的大量正文节点（共 75 项，部分长段落）
  const mockNodes: string[] = []
  for (let i = 0; i < 75; i++) {
    if (i % 10 === 0) {
      mockNodes.push('Super long paragraph '.repeat(150)) // 约 3150 字符
    } else {
      mockNodes.push(`Short item #${i}`)
    }
  }

  // 2. 模拟切批算法规划区间 (planBatchRanges)
  const ranges = planBatchRanges(mockNodes, {
    maxBatchItems: 30,
    maxBatchTotalChars: 50_000,
    maxItemChars: 10_000,
  })

  assert.ok(ranges.length >= 3, '75 项节点在 30 项上限下应切分为至少 3 批')
  assert.equal(ranges[0][0], 0, '首批起始索引必须为 0')
  assert.equal(ranges[ranges.length - 1][1], 75, '末批结束索引必须覆盖全部 75 项')

  // 3. 模拟前端状态机流转 (submit -> running -> resolve)
  let state = nextState('idle', 'submit')
  assert.equal(state, 'running')

  state = nextState(state, 'resolve')
  assert.equal(state, 'succeeded')

  // 4. 模拟网络异常失败与重试保护
  let failState = nextState('idle', 'submit')
  failState = nextState(failState, 'reject')
  assert.equal(failState, 'failed')

  const retryAllowed = canRetryExecution(
    failState,
    {
      kind: 'network',
      message: 'connection reset',
      retryable: true,
    },
    true,
  )
  assert.equal(retryAllowed, true, '网络瞬态异常必须允许重试')
})
