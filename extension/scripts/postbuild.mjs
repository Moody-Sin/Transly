import fs from 'node:fs'
import path from 'node:path'

const rootDir = path.resolve(import.meta.dirname, '..')
const distDir = path.join(rootDir, 'dist')

if (!fs.existsSync(distDir)) {
  fs.mkdirSync(distDir, { recursive: true })
}

// 版本号注入。
//
// manifest.json 与 package.json 里是硬编码版本，而 CI 发布走的是 git tag。
// 若原样复制，打 tag v0.3.0 时产出的 zip 内 manifest 仍写着 0.2.1，
// Chrome Web Store 会因「版本号未递增」直接拒绝上传。
// 因此优先读取 CI 传入的 TRANSLY_EXT_VERSION，未提供时回退到 package.json。
const pkg = JSON.parse(fs.readFileSync(path.join(rootDir, 'package.json'), 'utf8'))
const injectedVersion = process.env.TRANSLY_EXT_VERSION?.trim()
const version = injectedVersion || pkg.version

if (injectedVersion && injectedVersion !== pkg.version) {
  console.log(
    `[Extension Build] 版本号由 ${pkg.version} 覆盖为 ${injectedVersion}（来自 TRANSLY_EXT_VERSION）`,
  )
}

const manifest = JSON.parse(fs.readFileSync(path.join(rootDir, 'manifest.json'), 'utf8'))
manifest.version = version
fs.writeFileSync(path.join(distDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`)

fs.copyFileSync(path.join(rootDir, 'popup.html'), path.join(distDir, 'popup.html'))

// Chrome 原生 i18n 文案目录必须随产物一起分发
fs.cpSync(path.join(rootDir, '_locales'), path.join(distDir, '_locales'), { recursive: true })

console.log(
  `[Extension Build] manifest.json (v${version}), popup.html and _locales/ successfully copied to dist/`,
)
