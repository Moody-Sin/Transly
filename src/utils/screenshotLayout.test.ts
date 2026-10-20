import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  toPercentStyle,
  fitFontSize,
  clampRenderedBox,
  computeFloatingToolbarPosition,
} from './screenshotLayout.ts'

test('toPercentStyle: 归一化分量转百分比，且不产生浮点噪声', () => {
  const s = toPercentStyle({ x: 0.1, y: 0.2, w: 0.3, h: 0.4 })
  assert.equal(s.left, '10%')
  assert.equal(s.top, '20%')
  assert.equal(s.width, '30%')
  assert.equal(s.height, '40%')
})

test('toPercentStyle: 关键回归 —— 0.1*100 的浮点噪声必须被取整消除', () => {
  // 直接 `${0.1 * 100}%` 会得到 "10.000000000000002%"
  const s = toPercentStyle({ x: 0.1, y: 0.1, w: 0.1, h: 0.1 })
  assert.equal(s.left, '10%', '浮点噪声未取整会得到 10.000000000000002%')
  assert.ok(!s.left.includes('0000000'), `百分比含浮点噪声: ${s.left}`)
  // 0.07 这类值要保留有效精度，不能被粗暴取整成整数
  assert.equal(toPercentStyle({ x: 0.07, y: 0, w: 0, h: 0 }).left, '7%')
  assert.equal(toPercentStyle({ x: 0.12345, y: 0, w: 0, h: 0 }).left, '12.345%')
})

test('toPercentStyle: 边界值 0 与 1', () => {
  const zero = toPercentStyle({ x: 0, y: 0, w: 0, h: 0 })
  assert.equal(zero.left, '0%')
  assert.equal(zero.width, '0%')
  const one = toPercentStyle({ x: 1, y: 1, w: 1, h: 1 })
  assert.equal(one.left, '100%')
  assert.equal(one.width, '100%')
})

test('fitFontSize: 短文本用上限字号', () => {
  const size = fitFontSize('hi', { w: 400, h: 200 }, { maxPx: 20 })
  assert.equal(size, 20)
})

test('fitFontSize: 文本越长字号越小（单调不增）', () => {
  const box = { w: 300, h: 60 }
  const short = fitFontSize('short text', box, { maxPx: 20, minPx: 9 })
  const long = fitFontSize('a much much much longer piece of text here', box, {
    maxPx: 20,
    minPx: 9,
  })
  assert.ok(short !== null && long !== null)
  assert.ok(long! <= short!, `长文本字号应不大于短文本: ${long} vs ${short}`)
})

test('fitFontSize: 关键回归 —— 放不下时返回 null 而不是无限缩小', () => {
  // 极小框 + 极长文本：任何字号都放不下
  const size = fitFontSize('x'.repeat(5000), { w: 20, h: 12 }, { maxPx: 22, minPx: 9 })
  assert.equal(size, null, '放不下必须返回 null，让调用方走溢出显示而非缩成鬼影字')
})

test('fitFontSize: 结果永不小于最小字号下限', () => {
  const size = fitFontSize('medium length text', { w: 100, h: 30 }, { maxPx: 22, minPx: 12 })
  if (size !== null) {
    assert.ok(size >= 12, `字号 ${size} 低于下限 12`)
  }
})

test('fitFontSize: CJK 比同长度西文更宽（因此字号更小或同样）', () => {
  const box = { w: 200, h: 40 }
  const cjk = fitFontSize('这是一段中文文本内容', box, { maxPx: 20, minPx: 9 })
  const latin = fitFontSize('abcdefghijklmn', box, { maxPx: 20, minPx: 9 })
  assert.ok(cjk !== null && latin !== null)
  assert.ok(cjk! <= latin!, `CJK 字号(${cjk}) 应不大于等长西文(${latin})`)
})

test('fitFontSize: 空文本返回上限字号', () => {
  assert.equal(fitFontSize('   ', { w: 100, h: 50 }, { maxPx: 18 }), 18)
})

test('fitFontSize: 零尺寸框返回 null', () => {
  assert.equal(fitFontSize('text', { w: 0, h: 0 }), null)
  assert.equal(fitFontSize('text', { w: 100, h: 0 }), null)
})

test('clampRenderedBox: 框完全在容器内时原样返回', () => {
  const r = clampRenderedBox({ x: 0.1, y: 0.2, w: 0.3, h: 0.4 }, { w: 1, h: 1 })
  assert.deepEqual(r, { x: 0.1, y: 0.2, w: 0.3, h: 0.4 })
})

test('clampRenderedBox: 关键回归 —— 右/下边界也必须被钳制', () => {
  // 只钳左上的实现会原样放过 x=0.9,w=0.5（越界到 1.4）
  const r = clampRenderedBox({ x: 0.9, y: 0.9, w: 0.5, h: 0.5 }, { w: 1, h: 1 })
  assert.ok(r.x + r.w <= 1.0000001, `右边界越界: x=${r.x} w=${r.w}`)
  assert.ok(r.y + r.h <= 1.0000001, `下边界越界: y=${r.y} h=${r.h}`)
  assert.equal(r.x, 0.5)
  assert.equal(r.y, 0.5)
})

test('clampRenderedBox: 负坐标被钳到 0', () => {
  const r = clampRenderedBox({ x: -0.2, y: -0.3, w: 0.4, h: 0.4 }, { w: 1, h: 1 })
  assert.equal(r.x, 0)
  assert.equal(r.y, 0)
  assert.equal(r.w, 0.4)
  assert.equal(r.h, 0.4)
})

test('clampRenderedBox: 负尺寸归零（不得产生负宽高）', () => {
  const r = clampRenderedBox({ x: 0.2, y: 0.2, w: -0.5, h: -0.5 }, { w: 1, h: 1 })
  assert.equal(r.w, 0)
  assert.equal(r.h, 0)
  assert.ok(r.x >= 0 && r.y >= 0)
})

test('clampRenderedBox: 框大于容器时钳到容器尺寸', () => {
  const r = clampRenderedBox({ x: 0, y: 0, w: 2, h: 3 }, { w: 1, h: 1 })
  assert.equal(r.x, 0)
  assert.equal(r.y, 0)
  assert.equal(r.w, 2)
  assert.equal(r.h, 3)
  // 宽高本身不缩（缩放由 fitFontSize 负责），但原点不得为负
})

test('clampRenderedBox: 容器尺寸为零时返回零矩形', () => {
  const r = clampRenderedBox({ x: 0.5, y: 0.5, w: 0.2, h: 0.2 }, { w: 0, h: 0 })
  assert.deepEqual(r, { x: 0, y: 0, w: 0, h: 0 })
})

test('computeFloatingToolbarPosition: 选区下方空间充足时，停靠在选区右下角正下方', () => {
  const pos = computeFloatingToolbarPosition(
    { x: 100, y: 100, w: 500, h: 200 },
    { width: 1200, height: 800 },
    { width: 400, height: 40 },
    8,
    8,
  )
  assert.equal(pos.placement, 'bottom')
  // 右对齐选区右侧: 100 + 500 - 400 = 200
  assert.equal(pos.left, 200)
  // 下方停靠: 100 + 200 + 8 = 308
  assert.equal(pos.top, 308)
})

test('computeFloatingToolbarPosition: 选区贴底时自动翻转到选区正上方', () => {
  const pos = computeFloatingToolbarPosition(
    { x: 100, y: 600, w: 500, h: 180 },
    { width: 1200, height: 800 },
    { width: 400, height: 40 },
    8,
    8,
  )
  assert.equal(pos.placement, 'top')
  assert.equal(pos.left, 200)
  // 选区上方: 600 - 40 - 8 = 552
  assert.equal(pos.top, 552)
})

test('computeFloatingToolbarPosition: 选区靠右边缘时向左安全钳制防溢出', () => {
  const pos = computeFloatingToolbarPosition(
    { x: 800, y: 100, w: 400, h: 200 },
    { width: 1200, height: 800 },
    { width: 450, height: 40 },
    8,
    8,
  )
  // 视口宽 1200, padding 8, 最大右侧 1192
  // left + 450 <= 1192 => left = 742
  assert.equal(pos.left, 742)
  assert.ok(pos.left + 450 <= 1200 - 8)
})

test('computeFloatingToolbarPosition: 全屏/超大选区时内部贴底浮动', () => {
  const pos = computeFloatingToolbarPosition(
    { x: 0, y: 0, w: 1200, h: 800 },
    { width: 1200, height: 800 },
    { width: 400, height: 40 },
    8,
    8,
  )
  assert.equal(pos.placement, 'inside-bottom')
  assert.equal(pos.top, 800 - 8 - 40)
})
