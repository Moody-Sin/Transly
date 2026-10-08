import fs from 'node:fs'
import path from 'node:path'

const rootDir = path.resolve(import.meta.dirname, '..')
const distDir = path.join(rootDir, 'dist')

if (!fs.existsSync(distDir)) {
  fs.mkdirSync(distDir, { recursive: true })
}

fs.copyFileSync(
  path.join(rootDir, 'manifest.json'),
  path.join(distDir, 'manifest.json')
)

fs.copyFileSync(
  path.join(rootDir, 'popup.html'),
  path.join(distDir, 'popup.html')
)

// Chrome 原生 i18n 文案目录必须随产物一起分发
fs.cpSync(
  path.join(rootDir, '_locales'),
  path.join(distDir, '_locales'),
  { recursive: true }
)

console.log('[Extension Build] manifest.json, popup.html and _locales/ successfully copied to dist/')
