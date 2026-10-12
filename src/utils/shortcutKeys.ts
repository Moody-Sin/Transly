/**
 * 快捷键录制：把 KeyboardEvent 归一化为 Tauri global-shortcut 标准格式
 * （例如 `Alt+Shift+D`）。
 *
 * 抽成独立模块的原因：这是纯函数，却此前被埋在 `.tsx` 组件里，
 * 而 `pnpm run test:task` 走 `node --experimental-strip-types`（只剥类型、不转 JSX），
 * 因此 `.tsx` 中的逻辑无法被测试覆盖。
 */

/** KeyboardEvent 的最小结构子集，便于在无 DOM 的测试环境直接构造。 */
export interface ShortcutKeyEvent {
  code: string
  key: string
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
  metaKey: boolean
}

const MODIFIER_CODES: readonly string[] = [
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'MetaLeft',
  'MetaRight',
]

/** 按下修饰键本身，渲染为录制过程中的实时提示（如 `Ctrl+Shift`）。 */
export function formatModifiers(e: ShortcutKeyEvent): string {
  return modifierParts(e).join('+')
}

function modifierParts(e: ShortcutKeyEvent): string[] {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Super')
  return parts
}

/**
 * 归一化一次按键尝试：识别不出合法组合键时返回 null。
 * 纯修饰键与「无修饰键的单键」都视为无效，必须补修饰键。
 */
export function normalizeKey(e: ShortcutKeyEvent): string | null {
  // 纯修饰键不触发完成
  if (MODIFIER_CODES.includes(e.code)) return null

  const parts = modifierParts(e)
  // 必须至少包含一个修饰键
  if (parts.length === 0) return null

  // 主按键映射
  if (e.code.startsWith('Key')) {
    parts.push(e.code.slice(3).toUpperCase())
  } else if (e.code.startsWith('Digit')) {
    parts.push(e.code.slice(5))
  } else if (e.code.startsWith('F') && /^F\d+$/.test(e.code)) {
    parts.push(e.code)
  } else if (e.code === 'Space') {
    parts.push('Space')
  } else {
    // 其他符号按键降级处理
    const key = e.key.toUpperCase()
    if (key.length === 1) {
      parts.push(key)
    } else {
      return null
    }
  }

  return parts.join('+')
}
