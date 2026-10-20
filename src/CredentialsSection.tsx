import { Activity, Eye, EyeOff, Save } from 'lucide-react'
import type { I18nSchema } from './i18n'

export type EngineTestStatus = Record<
  string,
  { state: 'testing' | 'success' | 'error'; message: string }
>

type CredentialsSectionProps = {
  t: I18nSchema
  deeplKey: string
  setDeeplKey: (value: string) => void
  deeplConfigured: boolean
  deepseekKey: string
  setDeepseekKey: (value: string) => void
  deepseekConfigured: boolean
  openaiKey: string
  setOpenaiKey: (value: string) => void
  openaiConfigured: boolean
  showDeeplKey: boolean
  setShowDeeplKey: (value: boolean) => void
  showDeepseekKey: boolean
  setShowDeepseekKey: (value: boolean) => void
  showOpenaiKey: boolean
  setShowOpenaiKey: (value: boolean) => void
  deepseekBaseUrl: string
  setDeepseekBaseUrl: (value: string) => void
  deepseekModel: string
  setDeepseekModel: (value: string) => void
  openaiBaseUrl: string
  setOpenaiBaseUrl: (value: string) => void
  openaiModel: string
  setOpenaiModel: (value: string) => void
  engineTestStatus: EngineTestStatus
  handleTestEngine: (engine: string) => void
  handleDeleteKey: (engine: string) => void
  handleSaveKeys: () => void
}

/**
 * 设置中心的 API 密钥区块：DeepL / DeepSeek / OpenAI 三组凭据、
 * 自定义端点与模型、连通性测试，以及本地服务预设。
 *
 * 凭据状态仍由 `App` 持有（保存/删除涉及 keyring 与全局提示），
 * 这里只负责表单渲染与交互转发。
 */
export function CredentialsSection({
  t,
  deeplKey,
  setDeeplKey,
  deeplConfigured,
  deepseekKey,
  setDeepseekKey,
  deepseekConfigured,
  openaiKey,
  setOpenaiKey,
  openaiConfigured,
  showDeeplKey,
  setShowDeeplKey,
  showDeepseekKey,
  setShowDeepseekKey,
  showOpenaiKey,
  setShowOpenaiKey,
  deepseekBaseUrl,
  setDeepseekBaseUrl,
  deepseekModel,
  setDeepseekModel,
  openaiBaseUrl,
  setOpenaiBaseUrl,
  openaiModel,
  setOpenaiModel,
  engineTestStatus,
  handleTestEngine,
  handleDeleteKey,
  handleSaveKeys,
}: CredentialsSectionProps) {
  return (
    <div className="flex flex-col gap-2 rounded-lg bg-slate-50 dark:bg-slate-800/80 p-3 border border-slate-100 dark:border-slate-800">
      <span className="font-medium text-slate-700 dark:text-slate-200">
        {t.settings.credentialsTitle}
      </span>
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-slate-500 dark:text-slate-400">{t.settings.deeplKeyLabel}</label>
          {deeplConfigured && (
            <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
              {t.settings.configuredBadge}
              <button
                onClick={() => handleDeleteKey('deepl')}
                className="text-slate-400 hover:text-rose-600 ml-1 underline"
              >
                {t.settings.btnDeleteKey}
              </button>
            </span>
          )}
        </div>
        <div className="relative flex items-center">
          <input
            type={showDeeplKey ? 'text' : 'password'}
            value={deeplKey}
            onChange={(e) => setDeeplKey(e.target.value)}
            placeholder={
              deeplConfigured ? t.settings.keyPlaceholderOverwrite : t.settings.keyPlaceholderDeepl
            }
            autoComplete="off"
            spellCheck={false}
            className="w-full rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 pl-2.5 pr-8 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500 font-mono text-xs"
          />
          <button
            type="button"
            onClick={() => setShowDeeplKey(!showDeeplKey)}
            className="absolute right-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200"
            title={showDeeplKey ? t.settings.hidePassword : t.settings.showPassword}
          >
            {showDeeplKey ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </button>
        </div>
        <div className="mt-1 flex items-center justify-between min-h-[16px]">
          {(deeplConfigured || deeplKey.trim()) && (
            <button
              type="button"
              onClick={() => handleTestEngine('deepl')}
              disabled={engineTestStatus['deepl']?.state === 'testing'}
              className="flex items-center gap-1 text-[11px] text-indigo-600 dark:text-indigo-400 hover:underline disabled:opacity-50"
            >
              <Activity
                className={`h-3 w-3 ${engineTestStatus['deepl']?.state === 'testing' ? 'animate-spin' : ''}`}
              />
              <span>
                {engineTestStatus['deepl']?.state === 'testing'
                  ? t.settings.testingConnection
                  : t.settings.testConnection}
              </span>
            </button>
          )}
          {engineTestStatus['deepl'] && (
            <span
              className={`text-[10px] ${engineTestStatus['deepl'].state === 'success' ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-600 dark:text-rose-400'}`}
            >
              {engineTestStatus['deepl'].message}
            </span>
          )}
        </div>
      </div>
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-slate-500 dark:text-slate-400">
            {t.settings.deepseekKeyLabel}
          </label>
          {deepseekConfigured && (
            <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
              {t.settings.configuredBadge}
              <button
                onClick={() => handleDeleteKey('deepseek')}
                className="text-slate-400 hover:text-rose-600 ml-1 underline"
              >
                {t.settings.btnDeleteKey}
              </button>
            </span>
          )}
        </div>
        <div className="relative flex items-center">
          <input
            type={showDeepseekKey ? 'text' : 'password'}
            value={deepseekKey}
            onChange={(e) => setDeepseekKey(e.target.value)}
            placeholder={
              deepseekConfigured
                ? t.settings.keyPlaceholderOverwrite
                : t.settings.keyPlaceholderDeepseek
            }
            autoComplete="off"
            spellCheck={false}
            className="w-full rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 pl-2.5 pr-8 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500 font-mono text-xs"
          />
          <button
            type="button"
            onClick={() => setShowDeepseekKey(!showDeepseekKey)}
            className="absolute right-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200"
            title={showDeepseekKey ? t.settings.hidePassword : t.settings.showPassword}
          >
            {showDeepseekKey ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </button>
        </div>
        <div className="mt-1.5 text-[11px] text-slate-400">{t.settings.deepseekCustomTitle}</div>
        <div className="mt-1 grid grid-cols-2 gap-2">
          <input
            type="text"
            value={deepseekBaseUrl}
            onChange={(e) => setDeepseekBaseUrl(e.target.value)}
            placeholder={t.settings.deepseekBaseUrlPlaceholder}
            className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
          />
          <input
            type="text"
            value={deepseekModel}
            onChange={(e) => setDeepseekModel(e.target.value)}
            placeholder={t.settings.deepseekModelPlaceholder}
            className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
          />
        </div>
        <div className="mt-1 flex items-center justify-between min-h-[16px]">
          {(deepseekConfigured || deepseekKey.trim()) && (
            <button
              type="button"
              onClick={() => handleTestEngine('deepseek')}
              disabled={engineTestStatus['deepseek']?.state === 'testing'}
              className="flex items-center gap-1 text-[11px] text-indigo-600 dark:text-indigo-400 hover:underline disabled:opacity-50"
            >
              <Activity
                className={`h-3 w-3 ${engineTestStatus['deepseek']?.state === 'testing' ? 'animate-spin' : ''}`}
              />
              <span>
                {engineTestStatus['deepseek']?.state === 'testing'
                  ? t.settings.testingConnection
                  : t.settings.testConnection}
              </span>
            </button>
          )}
          {engineTestStatus['deepseek'] && (
            <span
              className={`text-[10px] ${engineTestStatus['deepseek'].state === 'success' ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-600 dark:text-rose-400'}`}
            >
              {engineTestStatus['deepseek'].message}
            </span>
          )}
        </div>
      </div>
      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-slate-500 dark:text-slate-400">{t.settings.openaiKeyLabel}</label>
          {openaiConfigured && (
            <span className="flex items-center gap-1 text-[11px] text-emerald-600 font-medium">
              {t.settings.configuredBadge}
              <button
                onClick={() => handleDeleteKey('openai')}
                className="text-slate-400 hover:text-rose-600 ml-1 underline"
              >
                {t.settings.btnDeleteKey}
              </button>
            </span>
          )}
        </div>
        <div className="relative flex items-center">
          <input
            type={showOpenaiKey ? 'text' : 'password'}
            value={openaiKey}
            onChange={(e) => setOpenaiKey(e.target.value)}
            placeholder={
              openaiConfigured
                ? t.settings.keyPlaceholderOverwrite
                : t.settings.keyPlaceholderOpenai
            }
            autoComplete="off"
            spellCheck={false}
            className="w-full rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 pl-2.5 pr-8 py-1.5 text-slate-800 dark:text-slate-100 outline-none focus:border-indigo-500 font-mono text-xs"
          />
          <button
            type="button"
            onClick={() => setShowOpenaiKey(!showOpenaiKey)}
            className="absolute right-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200"
            title={showOpenaiKey ? t.settings.hidePassword : t.settings.showPassword}
          >
            {showOpenaiKey ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </button>
        </div>
        <div className="mt-1.5 text-[11px] text-slate-400">{t.settings.openaiCustomTitle}</div>
        <div className="mt-1 grid grid-cols-2 gap-2">
          <input
            type="text"
            value={openaiBaseUrl}
            onChange={(e) => setOpenaiBaseUrl(e.target.value)}
            placeholder={t.settings.openaiBaseUrlPlaceholder}
            className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
          />
          <input
            type="text"
            value={openaiModel}
            onChange={(e) => setOpenaiModel(e.target.value)}
            placeholder={t.settings.openaiModelPlaceholder}
            className="rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-2 py-1 text-[11px] text-slate-700 dark:text-slate-200 outline-none focus:border-indigo-500"
          />
        </div>
        <div className="mt-1 flex items-center justify-between min-h-[16px]">
          <div className="flex items-center gap-2 flex-wrap">
            <button
              type="button"
              onClick={() => {
                setOpenaiBaseUrl('http://localhost:11434/v1')
                setOpenaiModel('qwen2.5:7b')
                if (!openaiKey) setOpenaiKey('local')
              }}
              className="text-[10px] text-indigo-600 dark:text-indigo-400 hover:underline"
            >
              💡 {t.settings.ollamaPresetBtn}
            </button>
            <button
              type="button"
              onClick={() => {
                setOpenaiBaseUrl('http://localhost:1234/v1')
                setOpenaiModel('local-model')
                if (!openaiKey) setOpenaiKey('local')
              }}
              className="text-[10px] text-indigo-600 dark:text-indigo-400 hover:underline"
            >
              💡 {t.settings.lmStudioPresetBtn}
            </button>
            <button
              type="button"
              onClick={() => {
                setOpenaiBaseUrl('http://localhost:8080/v1')
                setOpenaiModel('local-model')
                if (!openaiKey) setOpenaiKey('local')
              }}
              className="text-[10px] text-indigo-600 dark:text-indigo-400 hover:underline"
            >
              💡 {t.settings.llamaCppPresetBtn}
            </button>
            {(openaiConfigured || openaiKey.trim()) && (
              <button
                type="button"
                onClick={() => handleTestEngine('openai')}
                disabled={engineTestStatus['openai']?.state === 'testing'}
                className="flex items-center gap-1 text-[11px] text-indigo-600 dark:text-indigo-400 hover:underline disabled:opacity-50"
              >
                <Activity
                  className={`h-3 w-3 ${engineTestStatus['openai']?.state === 'testing' ? 'animate-spin' : ''}`}
                />
                <span>
                  {engineTestStatus['openai']?.state === 'testing'
                    ? t.settings.testingConnection
                    : t.settings.testConnection}
                </span>
              </button>
            )}
          </div>
          {engineTestStatus['openai'] && (
            <span
              className={`text-[10px] ${engineTestStatus['openai'].state === 'success' ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-600 dark:text-rose-400'}`}
            >
              {engineTestStatus['openai'].message}
            </span>
          )}
        </div>
      </div>
      <div className="flex justify-end pt-1">
        <button
          onClick={handleSaveKeys}
          className="flex items-center gap-1 rounded bg-indigo-600 px-3 py-1.5 font-medium text-white shadow-sm hover:bg-indigo-700"
        >
          <Save className="h-3.5 w-3.5" />
          <span>{t.settings.btnSaveKeys}</span>
        </button>
      </div>
    </div>
  )
}
