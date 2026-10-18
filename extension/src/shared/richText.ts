/**
 * 富文本段落聚合与占位保护辅助工具。
 *
 * 解决问题：网页中如果一个段落包含 <a>、<code>、<strong> 等行内子标签，
 * 离散节点切分会把段落砍成碎片单词独立翻译，因语序差异拼接后语法破版。
 * 通过提取行内子元素并用 Unicode 占位符保护，整句翻译后精准复原。
 */

export interface TextPart {
  id: string
  text: string
  isInline?: boolean
}

export interface RichTextComposite {
  compositeText: string
  placeholders: Map<string, string> // 占位符 -> 原文内容
  idToPlaceholder: Map<string, string> // id -> 占位符
  baseSlots?: (string | null)[] // 各占位符前后对应的 base id 槽位
}

export function extractPlaceholderTokens(text: string): string[] {
  const matches = text.match(/⟦\s*T_\d+\s*⟧/g)
  if (!matches) return []
  return matches.map((m) => m.replace(/⟦\s*T_(\d+)\s*⟧/, '⟦T_$1⟧'))
}

/**
 * 将离散的段落碎片聚合为带占位符的单一句子。
 * 支持显式 `isInline: boolean` 属性；缺省时回退为奇数项为行内的历史兼容策略。
 * 当 enclosed 为 true 时生成闭合标记 ⟦T_i⟧...⟦/T_i⟧ 以同时翻译行内子文本。
 */
export function buildRichTextComposite(
  parts: TextPart[],
  enclosed: boolean = false,
): RichTextComposite {
  const placeholders = new Map<string, string>()
  const idToPlaceholder = new Map<string, string>()
  const baseSlots: (string | null)[] = []

  if (parts.length <= 1) {
    return {
      compositeText: parts.length === 1 ? parts[0].text : '',
      placeholders,
      idToPlaceholder,
      baseSlots: parts.length === 1 && !parts[0].isInline ? [parts[0].id] : [null],
    }
  }

  let compositeText = ''
  let currentBaseId: string | null = null

  for (let i = 0; i < parts.length; i += 1) {
    const part = parts[i]
    const isInline = part.isInline !== undefined ? part.isInline : i % 2 === 1

    if (isInline) {
      baseSlots.push(currentBaseId)
      currentBaseId = null

      const ph = `⟦T_${i}⟧`
      placeholders.set(ph, part.text)
      idToPlaceholder.set(part.id, ph)
      if (enclosed) {
        compositeText += `⟦T_${i}⟧${part.text}⟦/T_${i}⟧`
      } else {
        compositeText += ph
      }
    } else {
      currentBaseId = part.id
      compositeText += part.text
    }
  }

  baseSlots.push(currentBaseId)

  return {
    compositeText,
    placeholders,
    idToPlaceholder,
    baseSlots,
  }
}

/**
 * 解析翻译后的复合句子，根据占位符切分并还原每个部分对应的译文。
 * 若占位符包含闭合标记 ⟦/T_i⟧，则提取行内子文本译文；否则设为空值标记。
 * 若占位符在翻译过程中被引擎吞掉，返回 null 触发安全回退。
 */
export function restoreRichTextComposite(
  translatedComposite: string,
  baseIds: (string | null)[],
  placeholderToId: Map<string, string>,
): Map<string, string> | null {
  // 容错处理：大模型偶发在 Unicode 括线内插入空格（例如 ⟦ T_1 ⟧ 或 ⟦/T_1 ⟧），先进行归一化
  const normalized = translatedComposite
    .replace(/⟦\s*T_(\d+)\s*⟧/g, '⟦T_$1⟧')
    .replace(/⟦\s*\/T_(\d+)\s*⟧/g, '⟦/T_$1⟧')

  const tokens = extractPlaceholderTokens(normalized)
  // 双重校验：数量一致还不够。若大模型把 ⟦T_1⟧ 输出了两次而漏掉 ⟦T_3⟧，
  // tokens.length 仍等于期望数量，却会造成某个节点漏翻、另一个节点被重复回填。
  // 因此必须要求去重后的集合大小也一致 —— 即每个期望占位符恰好出现一次。
  if (tokens.length !== placeholderToId.size || new Set(tokens).size !== placeholderToId.size) {
    // 占位符数量不匹配或存在重复/缺失，可能被翻译引擎吞掉或篡改，触发安全回退
    return null
  }

  // 语序校验：占位符在译文中的出现顺序必须与原文一致。
  //
  // 占位符编号即原文中的位置序号（`⟦T_i⟧` 的 i 随原文 part 下标递增），
  // 而 `baseIds` 是**按原文顺序**排列的非行内文本槽位。下方回填逻辑以
  // 「线性推进 slotIndex」消费 baseIds，这个前提只在占位符顺序未被改动时成立。
  //
  // 若引擎调换了占位符顺序（德语/日语等倒装语序下并不罕见），线性消费会把
  // 属于前半段的文本槽位填进后半段的译文切片，造成同段落内词序错乱。
  // 这种情形无法从译文本身唯一还原槽位归属，必须安全回退：
  // 调用方（content/index.ts）会记录 rich_text_mismatch 并保留原文 DOM 结构。
  const expectedOrder = [...tokens].sort((a, b) => {
    const na = Number(a.replace(/⟦T_(\d+)⟧/, '$1'))
    const nb = Number(b.replace(/⟦T_(\d+)⟧/, '$1'))
    return na - nb
  })
  for (let i = 0; i < tokens.length; i += 1) {
    if (tokens[i] !== expectedOrder[i]) {
      return null
    }
  }

  const result = new Map<string, string>()
  let remaining = normalized
  let slotIndex = 0

  for (const ph of tokens) {
    const targetId = placeholderToId.get(ph)
    if (!targetId) return null

    const idx = remaining.indexOf(ph)
    if (idx === -1) return null

    const before = remaining.slice(0, idx)
    if (slotIndex < baseIds.length) {
      const baseId = baseIds[slotIndex]
      if (baseId) {
        result.set(baseId, before)
      }
      slotIndex += 1
    }

    remaining = remaining.slice(idx + ph.length)

    // 检查是否存在闭合标签 ⟦/T_N⟧ 提取行内译文
    const idMatch = ph.match(/⟦T_(\d+)⟧/)
    const closeToken = idMatch ? `⟦/T_${idMatch[1]}⟧` : null

    if (closeToken && remaining.includes(closeToken)) {
      const closeIdx = remaining.indexOf(closeToken)
      const innerTranslated = remaining.slice(0, closeIdx)
      result.set(targetId, innerTranslated)
      remaining = remaining.slice(closeIdx + closeToken.length)
    } else {
      result.set(targetId, '')
    }
  }

  if (slotIndex < baseIds.length) {
    const baseId = baseIds[slotIndex]
    if (baseId) {
      result.set(baseId, remaining)
    }
  }

  return result
}

/**
 * 清洗复合文本中的所有内部占位符与闭合标签，返回纯净的段落译文字符串。
 * 供双语对照模式或降级回退时整段展示使用。
 */
export function cleanCompositeText(text: string): string {
  if (!text) return ''
  return text
    .replace(/⟦\s*T_\d+\s*⟧(.*?)⟦\s*\/T_\d+\s*⟧/gs, '$1')
    .replace(/⟦\s*\/?[Tt]_\d+\s*⟧/g, '')
    .trim()
}
