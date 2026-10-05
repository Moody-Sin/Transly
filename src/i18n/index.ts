import zhCN from './zh-CN.json'
import en from './en.json'
import type { OcrErrorKind } from '../bindings'

export type Locale = 'zh-CN' | 'en'

export type I18nSchema = typeof zhCN & {
  ocrError: Record<OcrErrorKind, string>
}

export const translations: Record<Locale, I18nSchema> = {
  'zh-CN': zhCN as I18nSchema,
  en: en as I18nSchema,
}

export function getTranslation(locale: Locale): I18nSchema {
  return translations[locale] || translations['zh-CN']
}
