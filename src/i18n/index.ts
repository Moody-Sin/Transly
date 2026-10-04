import zhCN from './zh-CN.json'
import en from './en.json'

export type Locale = 'zh-CN' | 'en'

export const translations = {
  'zh-CN': zhCN,
  en,
}

export function getTranslation(locale: Locale) {
  return translations[locale] || translations['zh-CN']
}
