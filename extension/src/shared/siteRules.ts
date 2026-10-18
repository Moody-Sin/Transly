export interface SiteRuleConfig {
  auto?: boolean
  targetLang?: string
  engine?: string
  excludeSelectors?: string
  disableInputAssistant?: boolean
}

export type RawSiteRule = 'auto' | SiteRuleConfig | undefined | null

/**
 * 规范化站点规则，无缝向后兼容历史的 'auto' 字符串标记。
 */
export function normalizeSiteRule(raw: RawSiteRule): SiteRuleConfig {
  if (!raw) {
    return { auto: false, disableInputAssistant: false }
  }
  if (raw === 'auto') {
    return { auto: true, disableInputAssistant: false }
  }
  if (typeof raw === 'object') {
    return {
      auto: Boolean(raw.auto),
      targetLang:
        typeof raw.targetLang === 'string' && raw.targetLang.trim()
          ? raw.targetLang.trim()
          : undefined,
      engine: typeof raw.engine === 'string' && raw.engine.trim() ? raw.engine.trim() : undefined,
      excludeSelectors:
        typeof raw.excludeSelectors === 'string' && raw.excludeSelectors.trim()
          ? raw.excludeSelectors.trim()
          : undefined,
      disableInputAssistant: Boolean(raw.disableInputAssistant),
    }
  }
  return { auto: false, disableInputAssistant: false }
}

/**
 * 安全检测元素是否命中了自定义排除选择器。
 *
 * 具备防爆保护：若用户输入了语法非法的 CSS 选择器，返回 false 放行，
 * 绝不因选择器语法错误抛出异常阻断整页翻译流程。
 */
export function isElementExcludedBySelector(element: Element, selectors?: string): boolean {
  if (!selectors || !selectors.trim()) {
    return false
  }
  try {
    return Boolean(element.closest(selectors.trim()))
  } catch {
    return false
  }
}
