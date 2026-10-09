import { useState, useEffect, useRef } from 'react'
import { Keyboard, Check, X } from 'lucide-react'
import type { I18nSchema } from './i18n'

export interface ShortcutRecorderProps {
  value: string
  onChange: (value: string) => void
  onSave: () => void
  t: I18nSchema
}

/**
 * 格式化键盘按键：转换为 Tauri global-shortcut 标准格式（例如：Alt+Shift+D）
 */
function normalizeKey(e: KeyboardEvent): string | null {
  const parts: string[] = []

  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Super')

  const code = e.code

  // 纯修饰键不触发完成
  if (
    [
      'ControlLeft',
      'ControlRight',
      'AltLeft',
      'AltRight',
      'ShiftLeft',
      'ShiftRight',
      'MetaLeft',
      'MetaRight',
    ].includes(code)
  ) {
    return null
  }

  // 必须至少包含一个修饰键
  if (parts.length === 0) {
    return null
  }

  // 主按键映射
  if (code.startsWith('Key')) {
    parts.push(code.slice(3).toUpperCase())
  } else if (code.startsWith('Digit')) {
    parts.push(code.slice(5))
  } else if (code.startsWith('F') && /^F\d+$/.test(code)) {
    parts.push(code)
  } else if (code === 'Space') {
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

export default function ShortcutRecorder({ value, onChange, onSave, t }: ShortcutRecorderProps) {
  const [recording, setRecording] = useState(false)
  const [currentRecordingKeys, setCurrentRecordingKeys] = useState<string>('')
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!recording) return

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()

      if (e.key === 'Escape') {
        setRecording(false)
        setCurrentRecordingKeys('')
        return
      }

      const normalized = normalizeKey(e)
      if (normalized) {
        onChange(normalized)
        setRecording(false)
        setCurrentRecordingKeys('')
      } else {
        // 显示当前按下的修饰键
        const parts: string[] = []
        if (e.ctrlKey) parts.push('Ctrl')
        if (e.altKey) parts.push('Alt')
        if (e.shiftKey) parts.push('Shift')
        if (e.metaKey) parts.push('Super')
        setCurrentRecordingKeys(parts.join('+'))
      }
    }

    window.addEventListener('keydown', handleKeyDown, true)
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true)
    }
  }, [recording, onChange])

  return (
    <div ref={containerRef} className="flex items-center gap-2">
      <div
        onClick={() => setRecording(true)}
        className={`flex h-8 flex-1 cursor-pointer items-center justify-between rounded border px-3 text-xs font-mono transition-all ${
          recording
            ? 'border-indigo-500 bg-indigo-50/50 text-indigo-700 ring-2 ring-indigo-200 dark:border-indigo-400 dark:bg-indigo-950/40 dark:text-indigo-300'
            : 'border-slate-200 bg-white text-slate-700 hover:border-slate-300 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200'
        }`}
      >
        <span className="flex items-center gap-1.5">
          <Keyboard
            className={`h-3.5 w-3.5 ${recording ? 'animate-pulse text-indigo-600' : 'text-slate-400'}`}
          />
          {recording
            ? currentRecordingKeys || t.settings.shortcutRecordingPlaceholder
            : value || t.settings.shortcutEmpty}
        </span>
        {recording ? (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation()
              setRecording(false)
              setCurrentRecordingKeys('')
            }}
            className="rounded p-0.5 text-slate-400 hover:bg-slate-200 hover:text-slate-600 dark:hover:bg-slate-700"
            title={t.settings.shortcutCancelRecord}
          >
            <X className="h-3.5 w-3.5" />
          </button>
        ) : (
          <span className="text-[10px] text-slate-400">{t.settings.shortcutClickToRecord}</span>
        )}
      </div>

      <button
        onClick={onSave}
        disabled={recording}
        className="flex h-8 items-center gap-1 rounded border border-indigo-200 bg-indigo-50 px-3 text-xs font-medium text-indigo-600 hover:bg-indigo-100 disabled:opacity-50 dark:border-indigo-800 dark:bg-indigo-950/60 dark:text-indigo-400 dark:hover:bg-indigo-900/60"
      >
        <Check className="h-3.5 w-3.5" />
        {t.settings.btnSaveShortcut}
      </button>
    </div>
  )
}
