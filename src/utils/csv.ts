/**
 * CSV 编解码（RFC 4180）。
 *
 * 历史面板导出一直按 RFC 4180 加引号，而术语库导入此前按裸 `,` 切分，
 * 含逗号 / 引号 / 换行的术语往返即被拆坏。导入与导出必须共用同一套引号解析，
 * 否则「导出 → 编辑 → 导入」永远无法闭环。
 */

/** 字段转义：整体加引号，内部引号按 RFC 4180 双写。 */
export function escapeCsvField(value: string): string {
  return `"${value.replace(/"/g, '""')}"`
}

/** 防御 Excel / 电子表格公式注入（CSV Formula Injection）。仅改值，不改引号。 */
export function guardCsvFormulaInjection(value: string): string {
  return /^[=+\-@\t\r]/.test(value) ? `'${value}` : value
}

/** 整行序列化：逐字段转义后用逗号连接。 */
export function formatCsvRow(fields: readonly string[]): string {
  return fields.map(escapeCsvField).join(',')
}

/**
 * 解析 CSV 文本为二维数组。
 *
 * 覆盖：带引号字段中的分隔符 / 引号 / 换行、`""` 转义、CRLF 与 LF 混排、BOM。
 * 无法解析的游离引号按字面量处理，不抛错（脏数据不该让整个导入流程崩掉）；
 * 但未闭合的引号会按 RFC 4180 吞掉其后的全部内容 —— 这是有意的严格语义。
 */
export function parseCsv(text: string): string[][] {
  const src = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text
  const rows: string[][] = []
  let row: string[] = []
  let field = ''
  let inQuotes = false

  for (let i = 0; i < src.length; i += 1) {
    const ch = src[i]
    if (inQuotes) {
      if (ch === '"') {
        if (src[i + 1] === '"') {
          field += '"'
          i += 1
        } else {
          inQuotes = false
        }
      } else {
        field += ch
      }
      continue
    }
    if (ch === '"') {
      inQuotes = true
      continue
    }
    if (ch === ',') {
      row.push(field)
      field = ''
      continue
    }
    if (ch === '\r' || ch === '\n') {
      if (ch === '\r' && src[i + 1] === '\n') i += 1
      row.push(field)
      field = ''
      rows.push(row)
      row = []
      continue
    }
    field += ch
  }

  // 收尾：文件末尾无换行时补上最后一行；恰好收在换行后则不产生空行
  if (field !== '' || row.length > 0) {
    row.push(field)
    rows.push(row)
  }
  return rows
}

/** 术语库 CSV 行（与后端 `GlossaryTermDto` 字段一一对应）。 */
export interface GlossaryCsvRow {
  source: string
  target: string
  matchCase: boolean
}

const GLOSSARY_HEADER = ['source', 'target', 'match_case']

/** 序列化为术语库 CSV（含表头），与 `parseGlossaryCsv` 严格互逆。 */
export function formatGlossaryCsv(rows: readonly GlossaryCsvRow[]): string {
  const body = rows.map((r) => formatCsvRow([r.source, r.target, String(r.matchCase)]))
  return [formatCsvRow(GLOSSARY_HEADER), ...body].join('\n')
}

/**
 * 解析术语库 CSV：跳过表头与空行，源/目标任一为空的行视为无效行丢弃，
 * `match_case` 缺省为 `true`（与设置面板默认值一致）。
 */
export function parseGlossaryCsv(text: string): GlossaryCsvRow[] {
  const out: GlossaryCsvRow[] = []
  for (const cells of parseCsv(text)) {
    if (cells.length === 0) continue
    const source = (cells[0] ?? '').trim()
    const target = (cells[1] ?? '').trim()
    if (!source || !target) continue
    if (
      source.toLowerCase() === GLOSSARY_HEADER[0] &&
      target.toLowerCase() === GLOSSARY_HEADER[1]
    ) {
      continue
    }
    const matchCase = (cells[2] ?? '').trim().toLowerCase() !== 'false'
    out.push({ source, target, matchCase })
  }
  return out
}
