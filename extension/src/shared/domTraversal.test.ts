import assert from 'node:assert/strict'
import test from 'node:test'
import {
  iterateElementsDeep,
  iterateTextNodesDeep,
  TEXT_NODE,
  ELEMENT_NODE,
} from './domTraversal.ts'

/**
 * 轻量 mock DOM：只实现遍历函数真正用到的那几个成员。
 * 目的是在无浏览器环境下验证「是否下探 open shadow root」这一行为，
 * 而不是去模拟完整 DOM 语义。
 */
class MockText {
  nodeType = TEXT_NODE
  nodeValue: string
  constructor(value: string) {
    this.nodeValue = value
  }
}

class MockElement {
  nodeType = ELEMENT_NODE
  childNodes: Array<MockElement | MockText> = []
  attributes = new Set<string>()
  shadowRoot: MockShadowRoot | null = null
  className: string
  tagName: string

  // 注：不要用 TS 参数属性（`constructor(public x)`），node --experimental-strip-types
  // 只剥离类型、不做代码变换，会直接语法报错。
  constructor(className = '', tagName = 'div') {
    this.className = className
    this.tagName = tagName
  }

  get classList() {
    return {
      contains: (c: string) => this.className.split(/\s+/).includes(c),
    }
  }

  setAttribute(name: string) {
    this.attributes.add(name)
  }

  removeAttribute(name: string) {
    this.attributes.delete(name)
  }

  append(node: MockElement | MockText) {
    this.childNodes.push(node)
    return node
  }

  attachShadow(mode: 'open' | 'closed'): MockShadowRoot {
    const root = new MockShadowRoot(mode)
    this.shadowRoot = root
    return root
  }

  /** 模拟浏览器语义：`querySelectorAll('*')` 不跨 shadow 边界 */
  querySelectorAll(selector: string): MockElement[] {
    if (selector !== '*') return []
    const out: MockElement[] = []
    const walk = (node: MockElement | MockText) => {
      if (node instanceof MockElement) {
        out.push(node)
        for (const child of node.childNodes) walk(child)
      }
    }
    for (const child of this.childNodes) walk(child)
    return out
  }
}

/** 真实 DOM 中 mode 挂在 ShadowRoot 上（host.shadowRoot.mode），不在宿主元素上 */
class MockShadowRoot extends MockElement {
  mode: 'open' | 'closed'

  constructor(mode: 'open' | 'closed') {
    super('', 'shadow-root')
    this.mode = mode
  }
}

function textValues(root: MockElement): string[] {
  return Array.from(iterateTextNodesDeep(root as unknown as Node)).map(
    (n) => (n as unknown as MockText).nodeValue,
  )
}

function elementTags(root: MockElement): string[] {
  return Array.from(iterateElementsDeep(root as unknown as Node)).map(
    (e) => (e as unknown as MockElement).tagName,
  )
}

test('iterateTextNodesDeep 收集 light DOM 的文本节点', () => {
  const body = new MockElement('', 'body')
  body.append(new MockText('hello'))
  const p = body.append(new MockElement('', 'p'))
  p.append(new MockText('world'))

  assert.deepEqual(textValues(body), ['hello', 'world'])
})

test('iterateTextNodesDeep 下探开放模式 Shadow DOM（回归：还原原文必须穿透 shadow）', () => {
  const body = new MockElement('', 'body')
  body.append(new MockText('light'))

  const host = body.append(new MockElement('', 'my-widget'))
  const shadow = host.attachShadow('open')
  shadow.append(new MockText('shadowed'))
  const inner = shadow.append(new MockElement('', 'span'))
  inner.append(new MockText('nested-in-shadow'))

  assert.deepEqual(
    textValues(body),
    ['light', 'shadowed', 'nested-in-shadow'],
    'shadow 内及其嵌套的文本必须被收集，否则还原无法覆盖',
  )
})

test('iterateTextNodesDeep 跳过封闭模式 Shadow DOM（外部本就不可访问）', () => {
  const body = new MockElement('', 'body')
  body.append(new MockText('light'))
  const host = body.append(new MockElement('', 'my-widget'))
  const shadow = host.attachShadow('closed')
  shadow.append(new MockText('secret'))

  assert.deepEqual(textValues(body), ['light'])
})

test('iterateTextNodesDeep 支持嵌套多层 shadow', () => {
  const body = new MockElement('', 'body')
  const outer = body.append(new MockElement('', 'a'))
  const s1 = outer.attachShadow('open')
  const mid = s1.append(new MockElement('', 'b'))
  const s2 = mid.attachShadow('open')
  s2.append(new MockText('deep'))

  assert.deepEqual(textValues(body), ['deep'])
})

test('iterateElementsDeep 下探开放模式 Shadow DOM', () => {
  const body = new MockElement('', 'body')
  const host = body.append(new MockElement('', 'widget'))
  const shadow = host.attachShadow('open')
  shadow.append(new MockElement('', 'inner-a'))
  shadow.append(new MockElement('', 'inner-b'))

  const tags = elementTags(body)
  assert.deepEqual(tags, ['widget', 'inner-a', 'inner-b'])
})

test('iterateElementsDeep 对非元素起点安全返回空', () => {
  const orphan = { nodeType: 9 } as unknown as Node
  assert.deepEqual(Array.from(iterateElementsDeep(orphan)), [])
})

test('还原语义：shadow 内已翻译节点能被完整还原，且父标记被清除', () => {
  // 模拟一次整页翻译后的真实状态
  const body = new MockElement('', 'body')
  const lightText = new MockText('译文-light')
  const p = body.append(new MockElement('', 'p'))
  p.append(lightText)
  p.setAttribute('data-transly-has-translation')

  const host = body.append(new MockElement('', 'widget'))
  host.setAttribute('data-transly-has-translation')
  const shadow = host.attachShadow('open')
  const shadowText = new MockText('译文-shadow')
  const shadowP = shadow.append(new MockElement('', 'p'))
  shadowP.append(shadowText)
  shadowP.setAttribute('data-transly-has-translation')

  // 以 restoreOriginalText 的核心逻辑复演（与 content/index.ts 中一致的三步）
  const originalTextMap = new WeakMap<object, string>()
  originalTextMap.set(lightText, '原文-light')
  originalTextMap.set(shadowText, '原文-shadow')

  for (const node of iterateTextNodesDeep(body)) {
    const orig = originalTextMap.get(node as unknown as object)
    if (orig !== undefined) node.nodeValue = orig
  }
  for (const el of iterateElementsDeep(body)) {
    el.removeAttribute('data-transly-has-translation')
  }

  assert.equal(lightText.nodeValue, '原文-light')
  assert.equal(
    shadowText.nodeValue,
    '原文-shadow',
    '回归核心：shadow 内译文必须被还原，否则「还原原文」在 Web Components 页面上永久失效',
  )
  assert.equal(p.attributes.has('data-transly-has-translation'), false)
  assert.equal(host.attributes.has('data-transly-has-translation'), false)
  assert.equal(shadowP.attributes.has('data-transly-has-translation'), false)
})
