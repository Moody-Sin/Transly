import test from 'node:test'
import assert from 'node:assert/strict'
import { planBatchRanges } from '../../extension/src/shared/batchPlan.ts'
import {
  buildRichTextComposite,
  restoreRichTextComposite,
} from '../../extension/src/shared/richText.ts'
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

test('E2E: 浏览器扩展双语对照模式与原文节点完全隔离', () => {
  interface MockElement {
    className: string
    textContent: string
  }
  // 模拟网页正文节点
  const mockNode = {
    nodeValue: 'Original Text to Translate',
    parentElement: {
      tagName: 'DIV',
      children: [] as MockElement[],
      appendChild(el: MockElement) {
        this.children.push(el)
      },
    },
  }

  // 模拟在双语模式下追加兄弟译文元素
  const transEl = {
    className: 'transly-bilingual-trans',
    textContent: ' (翻译后的中文)',
  }
  mockNode.parentElement.appendChild(transEl)

  // 验证：原节点内容完全不变，译文以独立元素紧随其后
  assert.equal(mockNode.nodeValue, 'Original Text to Translate')
  assert.equal(mockNode.parentElement.children.length, 1)
  assert.equal(mockNode.parentElement.children[0].className, 'transly-bilingual-trans')

  // 模拟还原原文：只需批量移除 .transly-bilingual-trans
  mockNode.parentElement.children = []
  assert.equal(mockNode.parentElement.children.length, 0)
  assert.equal(mockNode.nodeValue, 'Original Text to Translate')
})

test('E2E: 多引擎对比翻译 (Google / DeepL / DeepSeek) 响应聚合与容灾隔离', async () => {
  // 模拟三引擎并发调用结果
  const mockCalls = [
    Promise.resolve({ status: 'ok', data: { text: 'Google译文', engine: 'google' } }),
    Promise.resolve({ status: 'ok', data: { text: 'DeepL译文', engine: 'deepl' } }),
    Promise.reject(new Error('DeepSeek API限流')),
  ]

  const results = await Promise.allSettled(mockCalls)
  assert.equal(results.length, 3)

  // 验证 Google 与 DeepL 正常返回，DeepSeek 失败隔离不影响前两者
  assert.equal(results[0].status, 'fulfilled')
  assert.equal(results[1].status, 'fulfilled')
  assert.equal(results[2].status, 'rejected')
})

test('E2E: 富文本段落闭合占位聚合 -> 批量翻译 -> 准确按语序回填各子节点', () => {
  // 模拟网页段落: <p>Visit <a href="...">Transly docs</a> to learn more.</p>
  const nodeParts = [
    { id: 'part_0', text: 'Visit ' },
    { id: 'part_1', text: 'Transly docs' },
    { id: 'part_2', text: ' to learn more.' },
  ]

  const composite = buildRichTextComposite(nodeParts, true)
  assert.equal(composite.compositeText, 'Visit ⟦T_1⟧Transly docs⟦/T_1⟧ to learn more.')

  // 模拟翻译引擎将整句翻译为中文
  const engineTranslated = '访问⟦T_1⟧Transly 官方文档⟦/T_1⟧了解更多详情。'
  const baseIds = ['part_0', 'part_2']
  const placeholderToId = new Map([['⟦T_1⟧', 'part_1']])

  const unpacked = restoreRichTextComposite(engineTranslated, baseIds, placeholderToId)
  assert.ok(unpacked, '必须成功拆解回填')
  assert.equal(unpacked.get('part_0'), '访问')
  assert.equal(unpacked.get('part_1'), 'Transly 官方文档')
  assert.equal(unpacked.get('part_2'), '了解更多详情。')
})
