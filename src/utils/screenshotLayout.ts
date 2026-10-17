/**
 * 截图覆盖窗的布局纯逻辑。
 *
 * 独立成模块是为了**可单测**：字号估算与百分比换算的错误在肉眼上极难发现
 * （浮点误差、字号差 1px），而它们直接决定译文会不会盖住相邻块或溢出屏幕。
 */

/** 归一化矩形（原点左上，分量 ∈ [0,1]）。与 Rust 侧 `NormRect` 同构。 */
export interface NormRectLike {
  x: number
  y: number
  w: number
  h: number
}

/** 物理像素尺寸。 */
export interface Box {
  w: number
  h: number
}

/**
 * 归一化分量 → CSS 百分比字符串。
 *
 * 必须先取整到 4 位小数再乘 100：`0.1 * 100` 在 IEEE754 下是
 * `10.000000000000002`，直接拼进 `style.left` 会得到一长串无意义小数，
 * 且不同浏览器对超长小数的舍入可能不一致，导致块之间出现亚像素错位。
 */
function pct(value: number): string {
  return `${Number((value * 100).toFixed(4))}%`
}

/** 归一化矩形 → 百分比定位样式。组件里禁止再实现一遍换算。 */
export function toPercentStyle(rect: NormRectLike): {
  left: string
  top: string
  width: string
  height: string
} {
  return {
    left: pct(rect.x),
    top: pct(rect.y),
    width: pct(rect.w),
    height: pct(rect.h),
  }
}

/** `fitFontSize` 的可调参数。 */
export interface FitFontSizeOptions {
  /** 字号上限（px），默认 22 */
  maxPx?: number
  /** 字号下限（px），默认 9 —— 低于此值文字不可读，应改用溢出显示 */
  minPx?: number
  /** 行高倍数，默认 1.25 */
  lineHeight?: number
}

/** 单个字符的宽度相对字号的系数。 */
function charWidthRatio(ch: string): number {
  const code = ch.codePointAt(0) ?? 0
  // CJK / 全角标点：字宽 ≈ 1 个字号
  const isWide =
    (code >= 0x1100 && code <= 0x115f) ||
    (code >= 0x2e80 && code <= 0xa4cf) ||
    (code >= 0xac00 && code <= 0xd7a3) ||
    (code >= 0xf900 && code <= 0xfaff) ||
    (code >= 0xfe30 && code <= 0xfe6f) ||
    (code >= 0xff00 && code <= 0xff60) ||
    (code >= 0xffe0 && code <= 0xffe6)
  if (isWide) return 1.0
  // 空格更窄
  if (ch === ' ') return 0.3
  // 西文按 0.55 估算（等宽与比例字体的折中，偏保守）
  return 0.55
}

/**
 * 为「把译文塞进原文框」挑选字号（px）。
 *
 * 返回 `null` 表示**连最小字号都放不下**——调用方应改为按 `minPx` 渲染并允许溢出
 * （或截断），而不是继续缩小：缩到 2px 的鬼影字既不可读也浪费一次排版计算。
 *
 * 估算刻意**偏保守**（宁可猜小）：字号偏小只是留白，字号偏大会盖住相邻块，
 * 后者会直接破坏「按原坐标叠加」的可读性。
 */
export function fitFontSize(text: string, box: Box, opts: FitFontSizeOptions = {}): number | null {
  const { maxPx = 22, minPx = 9, lineHeight = 1.25 } = opts

  const content = text.trim()
  if (!content) return maxPx
  if (box.w <= 0 || box.h <= 0) return null

  // 从上限往下试，取第一个能放下的字号。
  // 步长 1px：字号是整数像素，继续细分对渲染无意义。
  for (let size = maxPx; size >= minPx; size--) {
    const lineHeightPx = size * lineHeight
    const maxLines = Math.floor(box.h / lineHeightPx)
    if (maxLines < 1) continue

    // ⚠️ 必须按**估算宽度**算容量，不能用「字数」。
    // 一个汉字约占 1 个字号宽、一个西文字母约占 0.55——若按字数算，
    // 14 个汉字会被误判为能塞进 20px 字号（实际需要 ~280px 而非 ~200px），
    // 于是译文溢出到相邻块上。
    const perLineCapacityPx = box.w
    const linesNeeded = countLines(content, size, perLineCapacityPx)
    if (linesNeeded <= maxLines) return size
  }

  return null
}

/** 按估算宽度计算文本在给定字号下需要几行（贪心断行，不做连字优化）。 */
function countLines(text: string, fontSizePx: number, maxWidthPx: number): number {
  if (maxWidthPx <= 0) return Number.POSITIVE_INFINITY
  let lines = 1
  let lineWidth = 0
  for (const ch of text) {
    const w = charWidthRatio(ch) * fontSizePx
    if (lineWidth + w > maxWidthPx && lineWidth > 0) {
      lines++
      lineWidth = w
    } else {
      lineWidth += w
    }
  }
  return lines
}

/**
 * 把渲染框钳制在容器内。
 *
 * 译文可能比原文长得多，`fitFontSize` 只保证字号合适，不保证框不越界；
 * 越界的块会渲染到覆盖窗之外（不可见）或被 `overflow-hidden` 裁掉半截。
 */
export function clampRenderedBox(box: NormRectLike, container: Box): NormRectLike {
  const cw = container.w
  const ch = container.h
  if (cw <= 0 || ch <= 0) return { x: 0, y: 0, w: 0, h: 0 }

  // 负尺寸先归零，否则后面的 min/max 会算出负宽高
  const w = Math.max(0, box.w)
  const h = Math.max(0, box.h)
  const x = Math.min(Math.max(0, box.x), Math.max(0, cw - w))
  const y = Math.min(Math.max(0, box.y), Math.max(0, ch - h))

  return { x, y, w, h }
}
