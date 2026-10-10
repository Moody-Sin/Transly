import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  escapeCsvField,
  formatCsvRow,
  guardCsvFormulaInjection,
  parseCsv,
  parseGlossaryCsv,
  formatGlossaryCsv,
  type GlossaryCsvRow,
} from './csv.ts'

test('术语库 CSV 往返回归：含逗号 / 引号 / 换行的术语必须原样还原', () => {
  // 这是「导出 → 编辑 → 导入」闭环的最小充分集：任何一种字符被裸切都会毁掉数据
  const rows: GlossaryCsvRow[] = [
    { source: 'LLM', target: '大语言模型', matchCase: true },
    { source: 'Transformer, Encoder', target: '含逗号术语', matchCase: false },
    { source: 'say "hi"', target: '他说「你好」', matchCase: true },
    { source: 'multi\nline', target: '换行\n译文', matchCase: false },
    { source: 'a,b,"c"', target: 'd"e,f', matchCase: true },
    { source: '=SUM(A1)', target: '@handle', matchCase: false },
  ]

  const csv = formatGlossaryCsv(rows)
  assert.deepEqual(parseGlossaryCsv(csv), rows)
})

test('术语库解析：跳过表头、空行与残缺行，match_case 缺省为 true', () => {
  const csv = [
    'source,target,match_case',
    '',
    '"LLM","大语言模型",true',
    '"","只有目标",false',
    '"只有源","",true',
    '"CoT","思维链"',
    '   ',
    '"CoT","思维链 v2",false',
  ].join('\n')

  assert.deepEqual(parseGlossaryCsv(csv), [
    { source: 'LLM', target: '大语言模型', matchCase: true },
    { source: 'CoT', target: '思维链', matchCase: true },
    { source: 'CoT', target: '思维链 v2', matchCase: false },
  ])
})

test('术语库解析：兼容 BOM、CRLF 与大小写混写的布尔值', () => {
  const csv = '﻿source,target,match_case\r\n"RL","强化学习",FALSE\r\n"DPO","直接偏好优化",0\r\n'
  assert.deepEqual(parseGlossaryCsv(csv), [
    { source: 'RL', target: '强化学习', matchCase: false },
    { source: 'DPO', target: '直接偏好优化', matchCase: true },
  ])
})

test('parseCsv：引号内的分隔符与换行不被切分，双写引号还原为单引号', () => {
  const csv = 'a,"b,c","d""e"\n"跨\n行",f,'
  assert.deepEqual(parseCsv(csv), [
    ['a', 'b,c', 'd"e'],
    ['跨\n行', 'f', ''],
  ])
})

test('parseCsv：文件末尾无换行时补齐最后一行，恰好收在换行后不产生空行', () => {
  assert.deepEqual(parseCsv('a,b\nc,d'), [
    ['a', 'b'],
    ['c', 'd'],
  ])
  assert.deepEqual(parseCsv('a,b\n'), [['a', 'b']])
  assert.deepEqual(parseCsv(''), [])
})

test('转义：字段始终加引号且内部引号双写；公式注入前缀转义', () => {
  assert.equal(escapeCsvField('a,b'), '"a,b"')
  assert.equal(escapeCsvField('say "hi"'), '"say ""hi"""')
  assert.equal(escapeCsvField('line1\nline2'), '"line1\nline2"')
  assert.equal(formatCsvRow(['x', 'y,z']), '"x","y,z"')

  assert.equal(guardCsvFormulaInjection('=SUM(A1)'), "'=SUM(A1)")
  assert.equal(guardCsvFormulaInjection('@cmd'), "'@cmd")
  assert.equal(guardCsvFormulaInjection('normal'), 'normal')
})
