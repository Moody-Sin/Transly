import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Windows 下监视 Rust 构建产物（target/ 下的 exe）会因文件被占用抛 EBUSY，
      // 该异常未捕获时直接终止 dev server，表现为 dev 页面瞬间白屏
      ignored: ['**/target/**'],
    },
  },
})
