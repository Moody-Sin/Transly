import { useState } from 'react'
import { Download, RefreshCw } from 'lucide-react'
import { commands } from './bindings'
import { getTranslation, type Locale } from './i18n'

type UpdateState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'up-to-date' }
  | { kind: 'available'; version: string; notes: string | null }
  | { kind: 'installing' }
  | { kind: 'error'; message: string }

/**
 * 设置中心的软件更新区块。
 *
 * 更新地址与签名公钥固定在 `tauri.conf.json`，这里只负责“检查 / 安装”两个意图，
 * 不直接调用插件 IPC，保持与其余后端能力一致的强类型命令入口。
 */
export function UpdateSection({ locale }: { locale: Locale }) {
  const t = getTranslation(locale).settings
  const [state, setState] = useState<UpdateState>({ kind: 'idle' })

  const check = async () => {
    setState({ kind: 'checking' })
    const res = await commands.checkForUpdate()
    if (res.status === 'error') {
      setState({ kind: 'error', message: res.error })
      return
    }
    setState(
      res.data
        ? { kind: 'available', version: res.data.version, notes: res.data.notes }
        : { kind: 'up-to-date' },
    )
  }

  const install = async () => {
    setState({ kind: 'installing' })
    const res = await commands.installUpdate()
    // 成功时应用会自行重启，不会走到这里；失败则保留可见的告警。
    if (res.status === 'error') {
      setState({ kind: 'error', message: res.error })
    }
  }

  const busy = state.kind === 'checking' || state.kind === 'installing'

  return (
    <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/60 p-3 border border-slate-100 dark:border-slate-800">
      <div>
        <div className="font-medium text-slate-700 dark:text-slate-200">{t.updateSectionTitle}</div>
        <div className="text-[11px] text-slate-400">{t.updateSectionDesc}</div>
      </div>

      <div className="flex flex-wrap items-center gap-2 pt-1">
        <button
          onClick={check}
          disabled={busy}
          className="inline-flex items-center gap-1.5 rounded-md border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-700 px-2.5 py-1 text-xs font-medium text-slate-600 dark:text-slate-200 transition-colors hover:bg-slate-100 dark:hover:bg-slate-600 disabled:cursor-not-allowed disabled:opacity-60"
        >
          <RefreshCw className={`h-3.5 w-3.5 ${state.kind === 'checking' ? 'animate-spin' : ''}`} />
          {state.kind === 'checking' ? t.updateChecking : t.updateCheck}
        </button>

        {state.kind === 'available' && (
          <button
            onClick={install}
            className="inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-indigo-700"
          >
            <Download className="h-3.5 w-3.5" />
            {t.updateInstall}
          </button>
        )}
      </div>

      {state.kind === 'up-to-date' && (
        <span className="text-[11px] text-emerald-600 dark:text-emerald-400">
          {t.updateUpToDate}
        </span>
      )}

      {state.kind === 'available' && (
        <span className="text-[11px] text-slate-500 dark:text-slate-400">
          {t.updateAvailable.replace('{version}', state.version)}
        </span>
      )}

      {state.kind === 'installing' && (
        <span className="text-[11px] text-slate-500 dark:text-slate-400">{t.updateInstalling}</span>
      )}

      {state.kind === 'error' && (
        <span className="text-[11px] text-rose-600 dark:text-rose-400">
          {t.updateFailed.replace('{msg}', state.message)}
        </span>
      )}
    </div>
  )
}
