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
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true)
    }
  }, [recording, onChange])

  return (
    <div className="flex items-center gap-2">
      <div
        tabIndex={0}
        role="button"
        aria-pressed={recording}
        aria-label={t.settings.shortcutRecorderAria}
        onClick={() => setRecording(true)}
        onKeyDown={(e) => {
          // 键盘可达性：裸 div 只能鼠标点击，Tab 用户永远无法进入录制态
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            setRecording(true)
          }
        }}
        className={`flex h-8 flex-1 cursor-pointer items-center justify-between rounded border px-3 text-xs font-mono transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-indigo-300 dark:focus-visible:ring-indigo-700 ${
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
