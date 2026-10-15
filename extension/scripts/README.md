# extension/scripts (浏览器扩展构建与适配脚本)

> 扩展独立打包构建、静态资产拷贝与 Firefox (Gecko) MV3 自动化适配脚本。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录承载 Transly 浏览器扩展的专用构建工具流。解决 Rollup/Vite 多入口拆分与浏览器扩展经典脚本环境（无 ESM 支持）之间的冲突，并提供全自动跨浏览器分发支持。

## 目录

- [脚本构成](#脚本构成)
- [关键构建保障](#关键构建保障)
- [许可证](#许可证)

## 脚本构成

- **build.mjs**：核心构建入口。对 `background`、`content`、`selection`、`popup` 四个执行上下文分别执行独立的 Vite/Rollup 打包，杜绝共享模块被提升为带 `import` 的独立 chunk；构建完成后自动执行 AST/正则断言，一旦产物出现 `^\s*(import|export)\s` 或存在 `dist/assets/` 立即阻断报错。
- **postbuild.mjs**：构建后处理。拷贝 `manifest.json`、`popup.html` 以及 `_locales/` 语言包至 `dist` 输出目录。
- **build-firefox.mjs**：自动化 Firefox Gecko 产物生成脚本。将 Chromium MV3 配置转换为 Firefox MV3 兼容格式（注入 `browser_specific_settings.gecko`，将 `service_worker` 兼容转为 `scripts` 数组），输出至 `dist-firefox`。

## 关键构建保障

- **经典脚本 ESM 隔离**：由于 `chrome.scripting.executeScript` 注入的脚本与 `popup.html` 的普通 `<script>` 均为经典脚本上下文，任何外部 chunk `import` 都会引发浏览器静默 `SyntaxError`。`build.mjs` 从流程上彻底杜绝此问题。

## 许可证

[商业专有许可](../../LICENSE)
