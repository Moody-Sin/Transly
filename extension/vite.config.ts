import { defineConfig } from 'vite'

/**
 * 开发服务器配置（`pnpm dev`）。
 *
 * **生产构建请使用 `scripts/build.mjs`，不要直接 `vite build`。**
 * 单次多入口构建会把被多个入口共享的模块（这里是 `src/i18n`）提升为
 * 独立 chunk，产物以 `import ... from "./assets/xxx.js"` 开头；
 * 而 `chrome.scripting.executeScript` 注入的 `content.js` / `selection.js`
 * 与 `popup.html` 的经典 `<script>` 都不支持 ESM 导入，会直接抛
 * `SyntaxError` —— 扩展能加载、无报错，但 Popup 与划词气泡静默失效。
 */
export default defineConfig({
  build: {
    outDir: 'dist',
    rollupOptions: {
      input: {
        background: 'src/background/index.ts',
        content: 'src/content/index.ts',
        selection: 'src/selection/index.ts',
        popup: 'src/popup/index.ts',
      },
      output: {
        entryFileNames: '[name].js',
      },
    },
  },
})
