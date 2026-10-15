import fs from 'node:fs'
import path from 'node:path'
import assert from 'node:assert/strict'

const rootDir = path.resolve(import.meta.dirname, '..')
const firefoxDistDir = path.join(rootDir, 'extension', 'dist-firefox')

console.log('[Firefox Validation] 正在验证 Firefox 扩展产物...')

assert.ok(fs.existsSync(firefoxDistDir), 'dist-firefox 目录必须存在')

const manifestPath = path.join(firefoxDistDir, 'manifest.json')
assert.ok(fs.existsSync(manifestPath), 'manifest.json 必须存在')

const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'))

// 1. 验证基础元数据
assert.strictEqual(manifest.manifest_version, 3, 'Firefox 扩展必须是 MV3 规范')
assert.ok(manifest.name, 'name 字段必须存在')
assert.ok(manifest.version, 'version 字段必须存在')

// 2. 验证 key 属性已被移除（Firefox 不允许包含 Chrome 专属 key）
assert.strictEqual(manifest.key, undefined, 'Firefox 产物不得包含 Chrome 专属 key 字段')

// 3. 验证 browser_specific_settings.gecko
assert.ok(manifest.browser_specific_settings?.gecko?.id, '必须包含 gecko.id')
assert.strictEqual(
  manifest.browser_specific_settings.gecko.id,
  '{d4e73b40-7988-4e89-a2e6-81525a74e2d3}',
  'gecko.id 必须与服务端白名单精准一致',
)
assert.strictEqual(
  manifest.browser_specific_settings.gecko.strict_min_version,
  '109.0',
  'strict_min_version 必须指定 109.0 以上',
)

// 4. 验证 background 兼容性（Firefox MV3 使用 scripts 数组）
assert.ok(
  manifest.background?.scripts?.includes('background.js'),
  'background 必须包含 background.js',
)
assert.strictEqual(
  manifest.background?.service_worker,
  undefined,
  'Firefox 不使用 service_worker 声明',
)

// 5. 验证必要的核心脚本与文件在 dist-firefox 中完备
const requiredFiles = [
  'background.js',
  'content.js',
  'selection.js',
  'popup.js',
  'popup.html',
  '_locales/zh_CN/messages.json',
  '_locales/en/messages.json',
]

for (const relPath of requiredFiles) {
  const fullPath = path.join(firefoxDistDir, relPath)
  assert.ok(fs.existsSync(fullPath), `产物缺少必要文件: ${relPath}`)
  const stat = fs.statSync(fullPath)
  assert.ok(stat.size > 0, `文件不可为空: ${relPath}`)
}

console.log('✔ Firefox 扩展产物完整性与 Gecko MV3 规范校验 100% 通过！')
