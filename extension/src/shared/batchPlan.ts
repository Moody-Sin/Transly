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
export function planBatchRanges(texts: string[], limits: BatchLimits): Array<[number, number]> {
  const ranges: Array<[number, number]> = []
  const maxItems = Math.max(1, limits.maxBatchItems)
  const maxChars = Math.max(1, limits.maxBatchTotalChars)

  let start = 0
  let acc = 0

  for (let idx = 0; idx < texts.length; idx += 1) {
    const len = texts[idx].length
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
