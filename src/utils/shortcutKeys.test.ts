import { test } from 'node:test'
import assert from 'node:assert/strict'
import { normalizeKey, formatModifiers, type ShortcutKeyEvent } from './shortcutKeys.ts'

function ev(partial: Partial<ShortcutKeyEvent> & { code: string; key?: string }): ShortcutKeyEvent {
  return {
    key: partial.key ?? partial.code,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    metaKey: false,
    ...partial,
  }
}

test('normalizeKey: 字母键取 code 后缀并大写，修饰键固定顺序拼接', () => {
  assert.equal(normalizeKey(ev({ code: 'KeyD', altKey: true, shiftKey: true })), 'Alt+Shift+D')
  assert.equal(normalizeKey(ev({ code: 'KeyT', ctrlKey: true, shiftKey: true })), 'Ctrl+Shift+T')
  assert.equal(
    normalizeKey(ev({ code: 'KeyK', ctrlKey: true, altKey: true, shiftKey: true, metaKey: true })),
    'Ctrl+Alt+Shift+Super+K',
  )
  // Super 永远排在 Shift 之后，与 Tauri 解析顺序一致
  assert.equal(normalizeKey(ev({ code: 'KeyM', metaKey: true })), 'Super+M')
})

test('normalizeKey: 数字键去掉 Digit 前缀，不做补零', () => {
  assert.equal(normalizeKey(ev({ code: 'Digit7', ctrlKey: true })), 'Ctrl+7')
  assert.equal(normalizeKey(ev({ code: 'Digit0', altKey: true })), 'Alt+0')
})

test('normalizeKey: 功能键走 /^F\\d+$/ 分支，含非法名称如 Foo 时回退到符号分支', () => {
  assert.equal(normalizeKey(ev({ code: 'F5', ctrlKey: true })), 'Ctrl+F5')
  assert.equal(normalizeKey(ev({ code: 'F12', altKey: true })), 'Alt+F12')
  // 'Foo' 以 F 开头但不匹配 F<数字>，必须落回符号降级分支
  assert.equal(normalizeKey(ev({ code: 'Foo', ctrlKey: true, key: 'o' })), 'Ctrl+O')
})

test('normalizeKey: Space 独立成支，输出空格单词而非字符', () => {
  assert.equal(
    normalizeKey(ev({ code: 'Space', ctrlKey: true, shiftKey: true })),
    'Ctrl+Shift+Space',
  )
  // 未经 Shift 时 Space 键的 e.key 仍是 ' '，走符号分支会得到空格字符 —— 必须由独立分支兜住
  assert.equal(normalizeKey(ev({ code: 'Space', ctrlKey: true, key: ' ' })), 'Ctrl+Space')
})

test('normalizeKey: 符号键降级为 e.key 单字符并大写，多字符键直接拒绝', () => {
  assert.equal(normalizeKey(ev({ code: 'Slash', ctrlKey: true, key: '/' })), 'Ctrl+/')
  assert.equal(normalizeKey(ev({ code: 'Comma', altKey: true, key: ',' })), 'Alt+,')
  assert.equal(normalizeKey(ev({ code: 'Semicolon', ctrlKey: true, key: ';' })), 'Ctrl+;')
  // 无修饰键的符号键：先在「必须含修饰键」处被拒
  assert.equal(normalizeKey(ev({ code: 'Slash', key: '/' })), null)
  // 有修饰键但 e.key 是多字符（ArrowLeft / Enter 等）：降级分支返回 null
  assert.equal(normalizeKey(ev({ code: 'ArrowLeft', ctrlKey: true, key: 'ArrowLeft' })), null)
  assert.equal(normalizeKey(ev({ code: 'Enter', ctrlKey: true, key: 'Enter' })), null)
})

test('normalizeKey: 纯修饰键与无修饰键单键一律返回 null（必须补主键）', () => {
  assert.equal(normalizeKey(ev({ code: 'ShiftLeft', shiftKey: true })), null)
  assert.equal(normalizeKey(ev({ code: 'ControlRight', ctrlKey: true })), null)
  assert.equal(normalizeKey(ev({ code: 'AltRight', altKey: true })), null)
  assert.equal(normalizeKey(ev({ code: 'MetaLeft', metaKey: true })), null)
  assert.equal(normalizeKey(ev({ code: 'KeyD' })), null)
})

test('formatModifiers: 录制过程中的实时修饰键提示，空则为字符串空值', () => {
  assert.equal(formatModifiers(ev({ code: 'KeyD' })), '')
  assert.equal(formatModifiers(ev({ code: 'KeyD', ctrlKey: true, shiftKey: true })), 'Ctrl+Shift')
})
