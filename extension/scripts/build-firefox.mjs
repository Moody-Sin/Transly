import fs from 'node:fs'
import path from 'node:path'

const rootDir = path.resolve(import.meta.dirname, '..')
const distDir = path.join(rootDir, 'dist')
const firefoxDistDir = path.join(rootDir, 'dist-firefox')

if (!fs.existsSync(distDir)) {
  console.error('[Firefox Build Error] dist 目录不存在，请先执行标准构建')
  process.exit(1)
}

if (!fs.existsSync(firefoxDistDir)) {
  fs.mkdirSync(firefoxDistDir, { recursive: true })
}

// 拷贝所有编译好的 JS 文件与 HTML、_locales 到 dist-firefox
fs.cpSync(distDir, firefoxDistDir, { recursive: true })

// 读取原始 manifest 并转换为兼容 Firefox Gecko 的 Manifest V2/V3 格式
const chromeManifest = JSON.parse(
  fs.readFileSync(path.join(distDir, 'manifest.json'), 'utf8')
)

const firefoxManifest = {
  ...chromeManifest,
  browser_specific_settings: {
    gecko: {
      id: "transly@transly.local",
      strict_min_version: "109.0"
    }
  },
  // Firefox MV3 background 规范兼容
  background: {
    scripts: ["background.js"]
  }
}

fs.writeFileSync(
  path.join(firefoxDistDir, 'manifest.json'),
  JSON.stringify(firefoxManifest, null, 2),
  'utf8'
)

console.log('[Firefox Build] Firefox 专属产物已就绪于 extension/dist-firefox')
