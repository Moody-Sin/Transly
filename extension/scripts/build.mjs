/**
 * 扩展生产构建。
 *
 * 四个执行上下文逐个独立构建，保证每个产物**自包含**：
 * - `chrome.scripting.executeScript` 按经典脚本注入 `content.js` / `selection.js`；
 * - `popup.html` 用经典 `<script src>`；
 * 只有 Service Worker 声明了 `"type": "module"`。
 *
 * 若用单次多入口构建，被多个入口共享的模块（`src/i18n`）会被 Rollup
 * 提升为独立 chunk，入口产物以 `import ... from "./assets/xxx.js"` 开头，
 * 在经典脚本上下文中直接抛 `SyntaxError` —— 扩展仍能加载、控制台无报错，
 * 但 Popup 与划词气泡全部失效。因此这里逐入口调用 Rollup 构建，
 * 并在构建后断言产物**不含任何 ESM 导入**，让该类回归无法再次静默通过。
 */
import fs from 'node:fs'
import path from 'node:path'
import { build } from 'vite'

const rootDir = path.resolve(import.meta.dirname, '..')
const distDir = path.join(rootDir, 'dist')
const ENTRIES = ['background', 'content', 'selection', 'popup']

for (const [index, name] of ENTRIES.entries()) {
  await build({
    root: rootDir,
    configFile: false,
    logLevel: 'warn',
    build: {
      outDir: distDir,
      emptyOutDir: index === 0,
      rollupOptions: {
        input: { [name]: path.join(rootDir, 'src', name, 'index.ts') },
        output: {
          entryFileNames: '[name].js',
          inlineDynamicImports: true,
        },
      },
    },
  })
}

// 断言每个产物自包含：出现 ESM 导入即说明被拆成了共享 chunk
const IMPORT_PATTERN = /^\s*(?:import|export)\s/m
for (const name of ENTRIES) {
  const file = path.join(distDir, `${name}.js`)
  const code = fs.readFileSync(file, 'utf8')
  if (IMPORT_PATTERN.test(code)) {
    throw new Error(`[Extension Build] dist/${name}.js 含有 ESM 导入，无法在经典脚本上下文运行`)
  }
}

const leftovers = fs.existsSync(path.join(distDir, 'assets'))
if (leftovers) {
  throw new Error('[Extension Build] dist/assets 非空，说明仍存在被拆出的共享 chunk')
}

console.log(`[Extension Build] ${ENTRIES.join(', ')} 已构建为自包含产物`)
