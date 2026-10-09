import js from '@eslint/js'
import tseslint from 'typescript-eslint'
import reactHooks from 'eslint-plugin-react-hooks'

export default tseslint.config(
  {
    ignores: [
      '**/dist/**',
      '**/dist-firefox/**',
      '**/target/**',
      '**/node_modules/**',
      'src/bindings.ts',
      'src-tauri/**',
      'extension/scripts/**',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
    },
  },
  {
    // 仅作用于 React 前端。`useTaskRunner` 是重度 useRef/useEffect 的自定义钩子，
    // 代际闸门与依赖数组的正确性是整套取消/重试语义的根基，必须由规则守护。
    files: ['src/**/*.{ts,tsx}'],
    plugins: { 'react-hooks': reactHooks },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
    },
  },
)
