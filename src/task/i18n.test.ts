import test from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'

function getObjectKeyPaths(obj: Record<string, unknown>, prefix = ''): string[] {
  let keys: string[] = []
  for (const [k, v] of Object.entries(obj)) {
    const fullPath = prefix ? `${prefix}.${k}` : k
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      keys = keys.concat(getObjectKeyPaths(v as Record<string, unknown>, fullPath))
    } else {
      keys.push(fullPath)
    }
  }
  return keys.sort()
}

test('桌面端 i18n 语言包（zh-CN.json 与 en.json）键结构必须完全对称', () => {
  const zhPath = path.resolve(process.cwd(), 'src/i18n/zh-CN.json')
  const enPath = path.resolve(process.cwd(), 'src/i18n/en.json')

  const zhJson = JSON.parse(fs.readFileSync(zhPath, 'utf8'))
  const enJson = JSON.parse(fs.readFileSync(enPath, 'utf8'))

  const zhKeys = getObjectKeyPaths(zhJson)
  const enKeys = getObjectKeyPaths(enJson)

  const missingInEn = zhKeys.filter((k) => !enKeys.includes(k))
  const missingInZh = enKeys.filter((k) => !zhKeys.includes(k))

  assert.deepEqual(missingInEn, [], `en.json 缺少以下多语言键: ${missingInEn.join(', ')}`)
  assert.deepEqual(missingInZh, [], `zh-CN.json 缺少以下多语言键: ${missingInZh.join(', ')}`)
})

test('浏览器扩展 _locales 语言包（zh_CN 与 en）messages.json 键结构必须完全对称', () => {
  const zhPath = path.resolve(process.cwd(), 'extension/_locales/zh_CN/messages.json')
  const enPath = path.resolve(process.cwd(), 'extension/_locales/en/messages.json')

  const zhJson = JSON.parse(fs.readFileSync(zhPath, 'utf8'))
  const enJson = JSON.parse(fs.readFileSync(enPath, 'utf8'))

  const zhKeys = Object.keys(zhJson).sort()
  const enKeys = Object.keys(enJson).sort()

  const missingInEn = zhKeys.filter((k) => !enKeys.includes(k))
  const missingInZh = enKeys.filter((k) => !zhKeys.includes(k))

  assert.deepEqual(
    missingInEn,
    [],
    `extension/_locales/en/messages.json 缺少以下键: ${missingInEn.join(', ')}`,
  )
  assert.deepEqual(
    missingInZh,
    [],
    `extension/_locales/zh_CN/messages.json 缺少以下键: ${missingInZh.join(', ')}`,
  )
})
