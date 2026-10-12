import { useState, useEffect } from 'react'
import { Keyboard, Check, X } from 'lucide-react'
import type { I18nSchema } from './i18n'
import { formatModifiers, normalizeKey } from './utils/shortcutKeys'

export interface ShortcutRecorderProps {
  value: string
  onChange: (value: string) => void
  onSave: () => void
  t: I18nSchema
}

export default function ShortcutRecorder({ value, onChange, onSave, t }: ShortcutRecorderProps) {
  const [recording, setRecording] = useState(false)
  const [currentRecordingKeys, setCurrentRecordingKeys] = useState<string>('')

  useEffect(() => {
    if (!recording) return

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        setRecording(false)
        setCurrentRecordingKeys('')
        return
      }

      const normalized = normalizeKey(e)
      if (normalized) {
        // 只拦截已识别为组合键的事件：此前在识别之前就无条件 preventDefault，
        // 会把录制期间的复制 / 粘贴 / 全选一并吞掉。
        e.preventDefault()
        e.stopPropagation()
        onChange(normalized)
        setRecording(false)
        setCurrentRecordingKeys('')
      } else {
        // 纯修饰键或无法归一化的按键：只更新提示，不碰浏览器默认行为
        setCurrentRecordingKeys(formatModifiers(e))
      }
    }

    window.addEventListener('keydown', handleKeyDown, true)

    const handleKeyUp = (e: KeyboardEvent) => {
      // 实时检测修饰键释放：当全部修饰键被松开时，及时清空提示，杜绝视觉残留
      if (!e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
        setCurrentRecordingKeys('')
      } else {
        setCurrentRecordingKeys(formatModifiers(e))
      }
    }

    window.addEventListener('keyup', handleKeyUp, true)
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true)
      window.removeEventListener('keyup', handleKeyUp, true)
    }
  }, [recording, onChange])

  return (
    <div className="flex items-center gap-2">
      <div
        className={`flex h-8 flex-1 items-center justify-between rounded border px-3 text-xs font-mono transition-all ${
          recording
            ? 'border-indigo-500 bg-indigo-50/50 text-indigo-700 ring-2 ring-indigo-200 dark:border-indigo-400 dark:bg-indigo-950/40 dark:text-indigo-300'
            : 'border-slate-200 bg-white text-slate-700 hover:border-slate-300 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200'
        }`}
      >
        <div
          tabIndex={0}
          role="button"
          aria-pressed={recording}
          aria-label={t.settings.shortcutRecorderAria}
          onClick={() => setRecording(true)}
          onKeyDown={(e) => {
            // 键盘可达性：Tab 用户可通过 Enter / 空格进入录制态
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault()
              setRecording(true)
            }
          }}
          className="flex flex-1 cursor-pointer items-center justify-between focus:outline-none focus-visible:ring-1 focus-visible:ring-indigo-300 dark:focus-visible:ring-indigo-700"
        >
          <span className="flex items-center gap-1.5">
            <Keyboard
              className={`h-3.5 w-3.5 ${recording ? 'animate-pulse text-indigo-600' : 'text-slate-400'}`}
            />
            {recording
              ? currentRecordingKeys || t.settings.shortcutRecordingPlaceholder
              : value || t.settings.shortcutEmpty}
          </span>
          {!recording && (
            <span className="text-[10px] text-slate-400">{t.settings.shortcutClickToRecord}</span>
          )}
        </div>
        {recording && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation()
              setRecording(false)
              setCurrentRecordingKeys('')
            }}
            className="ml-2 rounded p-0.5 text-slate-400 hover:bg-slate-200 hover:text-slate-600 dark:hover:bg-slate-700"
            title={t.settings.shortcutCancelRecord}
          >
            <X className="h-3.5 w-3.5" />
          </button>
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
