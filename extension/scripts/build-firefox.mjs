import fs from 'node:fs'
import path from 'node:path'

const rootDir = path.resolve(import.meta.dirname, '..')
const distDir = path.join(rootDir, 'dist')
const firefoxDistDir = path.join(rootDir, 'dist-firefox')

if (!fs.existsSync(distDir)) {
  console.error('[Firefox Build Error] dist 目录不存在，请先执行标准构建')
  process.exit(1)
}

// 先整体清空目标目录再复制。
// `fs.cpSync` 只增量覆盖，不会删除目标目录里已不存在的文件——
// 跨版本构建会把上一版已删除的产物残留在 zip 里，导致发布包含幽灵文件。
if (fs.existsSync(firefoxDistDir)) {
  fs.rmSync(firefoxDistDir, { recursive: true, force: true })
}
fs.mkdirSync(firefoxDistDir, { recursive: true })

// 拷贝所有编译好的 JS 文件与 HTML、_locales 到 dist-firefox
fs.cpSync(distDir, firefoxDistDir, { recursive: true })

// 读取原始 manifest 并转换为兼容 Firefox Gecko 的 Manifest V2/V3 格式
const chromeManifest = JSON.parse(fs.readFileSync(path.join(distDir, 'manifest.json'), 'utf8'))

const firefoxManifest = {
  ...chromeManifest,
  browser_specific_settings: {
    gecko: {
      // 固定的 addon ID：必须是一个合法 UUID 字面量，否则 addons-linter 会判定非法。
      // 此前遗留的是脚手架模板占位符 {{EMAIL_...}}，从未被替换过。
      id: '{d4e73b40-7988-4e89-a2e6-81525a74e2d3}',
      strict_min_version: '109.0',
    },
  },
  // Firefox MV3 background 规范兼容
  background: {
    scripts: ['background.js'],
  },
}
delete firefoxManifest.key

fs.writeFileSync(
  path.join(firefoxDistDir, 'manifest.json'),
  JSON.stringify(firefoxManifest, null, 2),
  'utf8',
)

console.log('[Firefox Build] Firefox 专属产物已就绪于 extension/dist-firefox')
