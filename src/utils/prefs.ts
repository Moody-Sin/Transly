/**
 * 本地偏好（主题 / LLM 风格）的运行时校验。
 *
 * `localStorage` 与后端返回值都是不可信输入：历史遗留值、手工改写或跨版本废弃值
 * 都会流进 `<select>`，导致选中态与说明文案整块空白。此处统一收口并回退默认值。
 */

export const THEME_VALUES = ['light', 'dark', 'system'] as const
export type ThemePreference = (typeof THEME_VALUES)[number]
export const DEFAULT_THEME: ThemePreference = 'system'

export const LLM_STYLE_VALUES = ['general', 'academic', 'literary', 'business'] as const
export type LlmStylePreference = (typeof LLM_STYLE_VALUES)[number]
export const DEFAULT_LLM_STYLE: LlmStylePreference = 'general'

export function parseThemePreference(raw: string | null | undefined): ThemePreference {
  return THEME_VALUES.find((value) => value === raw) ?? DEFAULT_THEME
}

export function parseLlmStylePreference(raw: string | null | undefined): LlmStylePreference {
  return LLM_STYLE_VALUES.find((value) => value === raw) ?? DEFAULT_LLM_STYLE
}
