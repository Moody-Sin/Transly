import zhCN from './zh-CN.json'
import en from './en.json'
import type { OcrErrorKind } from '../bindings'

export type Locale = 'zh-CN' | 'en'

export type I18nSchema = typeof zhCN & {
  ocrError: Record<OcrErrorKind, string>
}

// 显式标注 en 为 I18nSchema：en.json 缺键时编译期报错，
// 而不是被 `as` 断言静默掩盖（中英文案漂移是回归高发区）。
const enSchema: I18nSchema = en

const translations: Record<Locale, I18nSchema> = {
  'zh-CN': zhCN as I18nSchema,
  en: enSchema,
}

export function getTranslation(locale: Locale): I18nSchema {
  return translations[locale] || translations['zh-CN']
}
