/**
 * 扩展端文案统一入口。
 *
 * 文案一律取自 `extension/_locales/<locale>/messages.json`（Chrome 原生 i18n），
 * 禁止在代码或 HTML 中硬编码，与桌面端「禁止硬编码文案」规则保持一致。
 */
export function t(key: string, substitutions?: string[]): string {
  const message = chrome.i18n.getMessage(key, substitutions)
  return message || key
}

/**
 * 为静态节点填充文案：
 * - `data-i18n`：textContent
 * - `data-i18n-placeholder`：placeholder
 * - `data-i18n-title`：title
 */
export function applyStaticTranslations(root: ParentNode = document): void {
  document.documentElement.lang = chrome.i18n.getUILanguage()

  root.querySelectorAll<HTMLElement>('[data-i18n]').forEach((el) => {
    const key = el.getAttribute('data-i18n')
    if (key) el.textContent = t(key)
  })
  root.querySelectorAll<HTMLElement>('[data-i18n-placeholder]').forEach((el) => {
    const key = el.getAttribute('data-i18n-placeholder')
    if (key) el.setAttribute('placeholder', t(key))
  })
  root.querySelectorAll<HTMLElement>('[data-i18n-title]').forEach((el) => {
    const key = el.getAttribute('data-i18n-title')
    if (key) el.setAttribute('title', t(key))
  })
}
