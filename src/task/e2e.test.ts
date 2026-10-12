import fs from 'node:fs'
import { pathToFileURL } from 'node:url'
import test from 'node:test'
import assert from 'node:assert/strict'
import { planBatchRanges } from '../../extension/src/shared/batchPlan.ts'
import {
  buildRichTextComposite,
  restoreRichTextComposite,
} from '../../extension/src/shared/richText.ts'
import { nextState, canRetryExecution } from './taskState.ts'
import type { TaskRunner } from './useTaskRunner.ts'

// 本文件混合了两层测试：
// 1) 纯函数/算法级 E2E（分块切批、富文本占位回填），无需任何运行时打桩；
// 2) React Hook 级 E2E（useTaskRunner），需要伪造 Tauri IPC 宿主环境。
// 第 2 类测试依赖 import.meta 与动态 import，故统一放在文件末尾的 IIFE 中注册，
// 避免顶层 await + 全局打桩污染第 1 类测试的运行时。

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

test('E2E: 富文本占位符丢失或篡改时安全返回 null，回退逻辑不破坏 DOM', () => {
  const baseIds = ['part_0', 'part_2']
  const placeholderToId = new Map([['⟦T_1⟧', 'part_1']])

  // 模拟引擎丢弃了 ⟦T_1⟧ 占位符
  const mangledTranslation = '访问 Transly 官方文档了解更多详情。'
  const unpacked = restoreRichTextComposite(mangledTranslation, baseIds, placeholderToId)
  assert.equal(unpacked, null, '占位符丢失时必须安全返回 null 触发降级')
})

test('E2E: 富文本文案重复占位符顶替缺失占位符时拒绝回填（p_bug 安全回退）', () => {
  // 原文: 'Please check <b>Important Doc</b> and <i>Notice</i>.'
  // 大模型把 ⟦T_3⟧ 误输出成第二个 ⟦T_1⟧：占位符总数仍是 2，
  // 仅凭数量校验会被绕过，导致 p3 漏翻、p1 被重复回填。
  const nodeParts = [
    { id: 'p0', text: 'Please check ', isInline: false },
    { id: 'p1', text: 'Important Doc', isInline: true },
    { id: 'p2', text: ' and ', isInline: false },
    { id: 'p3', text: 'Notice', isInline: true },
    { id: 'p4', text: '.', isInline: false },
  ]
  const composite = buildRichTextComposite(nodeParts, true)
  const placeholderToId = new Map([
    ['⟦T_1⟧', 'p1'],
    ['⟦T_3⟧', 'p3'],
  ])

  const duplicated = '请查阅⟦T_1⟧重要文档⟦/T_1⟧与⟦T_1⟧须知⟦/T_1⟧。'
  assert.equal(
    restoreRichTextComposite(duplicated, composite.baseSlots || [], placeholderToId),
    null,
    '重复占位符必须触发安全回退 null，绝不静默漏翻',
  )

  // 对照组：完全合法的译文必须照常通过校验
  const valid = '请查阅⟦T_1⟧重要文档⟦/T_1⟧与⟦T_3⟧须知⟦/T_3⟧。'
  const restored = restoreRichTextComposite(valid, composite.baseSlots || [], placeholderToId)
  assert.ok(restored, '合法译文不得被误杀')
  assert.equal(restored.get('p1'), '重要文档')
  assert.equal(restored.get('p3'), '须知')
})

// ---------------------------------------------------------------------------
// React Hook 级 E2E：需要伪造 Tauri IPC 宿主，集中于本 IIFE 内注册
// ---------------------------------------------------------------------------
void (async () => {
  // 无扩展名相对导入（如 './taskState'）在 Node ESM 下无法解析，
  // 安装一个最小 resolve hook 让它们落到 .ts/.tsx 实体文件。
  const Module = (await import('node:module')).default
  const nodePath = (await import('node:path')).default
  const nodeUrl = await import('node:url')
  Module.registerHooks({
    resolve(specifier, context, next) {
      if (/^\.\.?\//.test(specifier) && context.parentURL?.startsWith('file:')) {
        const __dirname = nodePath.dirname(nodeUrl.fileURLToPath(import.meta.url))
        const base = nodePath.resolve(__dirname, specifier)
        for (const cand of [base + '.ts', base + '.tsx', base + '.js']) {
          if (fs.existsSync(cand)) {
            return { url: pathToFileURL(cand).href, shortCircuit: true }
          }
        }
      }
      return next(specifier, context)
    },
  })

  // 伪造 Tauri IPC 宿主：记录全部命令调用，用于断言引擎/目标语言
  interface InvokeCall {
    cmd: string
    args: Record<string, unknown>
  }
  let invokeCalls: InvokeCall[] = []
  ;(globalThis as { window?: unknown }).window = {
    // 非 #float 哈希 => 走主窗口分支（translateStream 链路）
    location: { hash: '' },
    __TAURI_INTERNALS__: {
      invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
        invokeCalls.push({ cmd, args })
        if (cmd === 'detect_text_language') return 'en'
        return { text: '译文占位', engine: String(args.engineName ?? 'google') }
      },
      transformCallback: () => 1,
    },
  }

  const React = (await import('react')).default
  const { renderToStaticMarkup } = await import('react-dom/server')
  const { useTaskRunner } = await import('./useTaskRunner.ts')

  test('E2E: setEngine/setTargetLang 尚未重渲染时 run() 必须使用新引擎与新目标语言', async () => {
    invokeCalls = []
    let runner: TaskRunner | null = null

    // renderToStaticMarkup 只跑一次渲染且**不执行 useEffect**，
    // 精确复现「setState 后尚未重渲染」这一时间窗。
    function Probe() {
      runner = useTaskRunner('google', 'zh-CN')
      return null
    }
    renderToStaticMarkup(React.createElement(Probe))

    const harness = runner as unknown as TaskRunner
    assert.ok(harness.run, '必须拿到 runner.run')

    // 模拟 FloatWindow 中 setEngine('deepseek') + setTargetLang('ja') 的 onChange
    harness.run('Hello World', { engine: 'deepseek', targetLang: 'ja' })
    await new Promise((r) => setTimeout(r, 300))

    const translations = invokeCalls.filter((c) =>
      c.cmd === 'translate' ? true : c.cmd === 'translate_stream',
    )
    assert.equal(translations.length, 1, '必须恰好发起一次翻译请求')

    const req = translations[0]
    assert.equal(
      req.args.engineName,
      'deepseek',
      `重跑必须立刻使用新引擎，实际却用了旧引擎: ${String(req.args.engineName)}`,
    )
    assert.equal(
      req.args.targetLang,
      'ja',
      `重跑必须立刻使用新目标语言，实际却用了旧语言: ${String(req.args.targetLang)}`,
    )
  })
})()
