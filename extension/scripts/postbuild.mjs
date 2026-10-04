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

console.log('[Extension Build] manifest.json and popup.html successfully copied to dist/')
