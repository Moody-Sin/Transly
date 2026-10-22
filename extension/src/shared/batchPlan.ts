/**
 * 批量切批规划（纯函数，与 Rust 侧 `transly_core::plan_batch_ranges` 语义一致）。
 *
 * 为什么必须有这一层：扩展端原先只按「项数」切批，而服务端按「字符总量」拒绝。
 * 两者量纲不同——30 个平均 1667 字的节点字符总量即越过 50000 上限，
 * 服务端返回 413，该批全部节点被标为失败，页面静默无译文。
 * 因此切批必须同时满足项数与字符总量两个上限，且上限值向服务端索取。
 */

/** 服务端批量上限。字段与 `/health` 响应对齐。 */
export interface BatchLimits {
  /** 单批最大项数 */
  maxBatchItems: number
  /** 单批字符总量上限 */
  maxBatchTotalChars: number
  /** 单条文本字符上限 */
  maxItemChars: number
}

/**
 * 将一批文本按项数与字符总量双上限切成连续区间下标。
 *
 * 返回左闭右开区间列表，保证：
 * - 每个区间内字符总量不超过 `maxBatchTotalChars`；
 * - 每个区间内项数不超过 `maxBatchItems`；
 * - 单条文本自身超限时独占一个区间，交由服务端逐项报错，绝不静默丢弃。
 *
 * @param texts 待切批文本（仅使用其字符数，不读取内容）
 * @param limits 服务端上限
 */
/**
 * 统计码点数（Unicode scalar value），与 Rust 侧 `str::chars().count()` 口径一致。
 *
 * **不要用 `String.prototype.length`**：那是 UTF-16 code unit 数，emoji、
 * 部分生僻字与星平面字符占 2 个 code unit。两侧口径一旦不同就会出问题：
 * - 服务端按码点校验，扩展按 code unit 校验 → 合法的长 emoji 选区被前端
 *   误判为「超长」而拒绝，而服务端本可以正常翻译；
 * - 切批时 JS 恒大于等于 Rust → 过度切批，多发若干请求。
 */
export function countChars(text: string): number {
  let count = 0
  // 按码点步进：既不分配中间数组，也不解构出未使用的变量
  for (let i = 0; i < text.length;) {
    const codePoint = text.codePointAt(i)
    if (codePoint === undefined) break
    i += codePoint > 0xffff ? 2 : 1
    count += 1
  }
  return count
}

/**
 * 将一批文本按项数与字符总量双上限切成连续区间下标。
 *
 * 返回左闭右开区间列表，保证：
 * - 每个区间内字符总量不超过 `maxBatchTotalChars`；
 * - 每个区间内项数不超过 `maxBatchItems`；
 * - 单条自身超过 `maxItemChars` 的文本**严格独占一个区间**，不与前后项混批。
 *
 * 为什么超长项必须独占：服务端对超限项返回 413 拒掉**整批**（不是逐项报错）。
 * 若让一条 20000 字的段落和 29 条正常段落同批，那 29 条也会因这一条被整批拒绝，
 * 而 20000 < 50000，旧的字符总量判据完全看不出它有问题。
 *
 * @param texts 待切批文本（仅使用其字符数，不读取内容）
 * @param limits 服务端上限
 */
export function planBatchRanges(texts: string[], limits: BatchLimits): Array<[number, number]> {
  const ranges: Array<[number, number]> = []
  const maxItems = Math.max(1, limits.maxBatchItems)
  const maxChars = Math.max(1, limits.maxBatchTotalChars)
  const maxItemChars = Math.max(1, limits.maxItemChars)

  let start = 0
  let acc = 0

  for (let idx = 0; idx < texts.length; idx += 1) {
    const len = countChars(texts[idx])

    // 自身超单条上限：无论如何独占一格，且不与前后任何项合并
    if (len > maxItemChars) {
      if (start < idx) ranges.push([start, idx])
      ranges.push([idx, idx + 1])
      start = idx + 1
      acc = 0
      continue
    }

    const count = idx - start

    if (count > 0 && (acc + len > maxChars || count >= maxItems)) {
      ranges.push([start, idx])
      start = idx
      acc = 0
    }
    acc += len
  }

  if (start < texts.length) {
    ranges.push([start, texts.length])
  }
  return ranges
}

/**
 * 视口调度优先级排序（首屏优先 Viewport-First）。
 *
 * 将收集到的待翻译项目分为视口内与视口外两组，
 * 视口内项目优先排列在头部，视口外在后，各自保持文档原有前向顺序。
 *
 * @param items 包含 inViewport 标志的项目数组
 */
export function splitByViewport<T extends { inViewport?: boolean }>(
  items: T[],
): {
  prioritized: T[]
  inViewportCount: number
  offscreenCount: number
} {
  const inViewport: T[] = []
  const offscreen: T[] = []

  for (const item of items) {
    if (item.inViewport) {
      inViewport.push(item)
    } else {
      offscreen.push(item)
    }
  }

  return {
    prioritized: [...inViewport, ...offscreen],
    inViewportCount: inViewport.length,
    offscreenCount: offscreen.length,
  }
}
