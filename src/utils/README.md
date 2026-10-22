# src/utils (前端通用辅助工具)

> 桌面端前端共享通用辅助工具函数集合。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录承载跨视图（主窗口、悬浮窗、历史面板、截图覆盖窗）复用的纯工具函数与系统级交互桥接。

## 目录

- [模块](#模块)
- [约定](#约定)
- [测试](#测试)
- [许可证](#许可证)

## 模块

| 文件 | 职责 |
|---|---|
| `speech.ts` | Web Speech API 封装；把源语言代码（含 `auto` / 未配置 / 小写缩写）映射到标准 BCP-47 区域标签，杜绝语言标记非法导致发音静默 |
| `csv.ts` | CSV 行拼装与解析；含公式注入防护（`guardCsvFormulaInjection`） |
| `download.ts` | Blob 一键下载桥接 |
| `prefs.ts` | 主题（`theme`）与 LLM 风格（`llmStyle`）本地偏好的**容错**解析，非法值一律回退默认 |
| `shortcutKeys.ts` | 快捷键录制时的按键归一化（`normalizeKey` / `formatModifiers`） |
| `screenshotLayout.ts` | 截图译文叠加的纯布局计算：归一化→百分比、字号自适应（`fitFontSize`）、框体钳制（`clampRenderedBox`） |
| `useDialogFocus.ts` | 模态对话框焦点管理：Tab 循环、Escape 关闭、关闭后焦点归位 |
| `useTransientMessage.ts` | 瞬时提示状态：写入文案后按需自动清除，统一管理清除定时器与卸载清理 |

## 约定

- **能不碰 React 就不碰**：除两个 `use*.ts` 外，其余模块必须是可在 Node 环境直接调用的纯函数
  （被测逻辑一律下沉到纯函数，hook 只做状态封装）。
- **不可信输入回退默认**：`prefs.ts` 这类解析函数面对 `localStorage` 这种可被改写的输入，
  必须返回合法值而不是 `null` 或抛错，避免非法状态流进渲染路径。

## 测试

单元测试通过 Node.js 原生测试运行器执行（`src/utils/*.test.ts` 与 `src/task/*.test.ts` 一并跑）：

```sh
pnpm run test:task
```

`useDialogFocus.ts` / `useTransientMessage.ts` 依赖 React 运行时与 DOM，不在 Node 测试范围内；
它们由 `tsc` 与 `eslint-plugin-react-hooks`（`rules-of-hooks` / `exhaustive-deps`）守护。

## 许可证

[商业专有许可](../../LICENSE) © Transly Authors
