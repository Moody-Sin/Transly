/** 代际闸门：迟到结果一律返回 null，绝不污染新代状态 */
export function applyIfCurrent<T>(gen: number, current: number, value: T): T | null {
  return gen === current ? value : null
}
