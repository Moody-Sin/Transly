# extension/src/popup (浏览器扩展弹窗交互界面)

> 扩展工具栏 Popup 弹窗逻辑：一键安全配对、双语对照切换、站点记忆与进度反馈。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块驱动点击浏览器工具栏 Transly 扩展图标时展示的 Popup 弹窗界面（`popup.html`）。提供桌面端配对状态管理、目标语言/引擎选择、整页翻译触发及实时进度回显。

## 目录

- [功能](#功能)
- [核心机制](#核心机制)
- [许可证](#许可证)

## 功能

- **一键自动安全握手配对**：点击【一键配对桌面端】发起握手并自动轮询，桌面端确认后自动写入 `chrome.storage.local`，免除手动复制粘贴长 Token。同时提供备用手动粘贴 Token 选项。
- **整页沉浸式翻译控制**：提供「翻译此网页 (仅译文)」与「还原原文」快捷操作，支持查看当前 Tab 实时进度与批次状态。
- **翻译模式单选组**：支持切换「仅译文（原地替换）」与「双语对照」双模式。
- **站点自动翻译记忆**：底栏提供「为此站点记住自动翻译」复选框，基于当前站点域名持久化保存自动翻译偏好。
- **实时健康探测**：打开弹窗时自动探测桌面端 `127.0.0.1:39281` 连通性与 Token 有效性，以绿标（已连接）、黄标（Token 失效 401）或红标（服务未启动）即时提示。

## 核心机制

- **独立构建无 ESM 拆包**：Popup 脚本作为经典脚本被 `popup.html` 引用，构建时保证代码自包含，杜绝 Rollup 将共享模块拆分至 `dist/assets/` 导致经典脚本抛出 `import` 语法错误。
- **原生 Chrome i18n 注入**：所有文案基于 `data-i18n` 属性与 `chrome.i18n.getMessage` 动态注入，杜绝硬编码。

## 许可证

[商业专有许可](../../../LICENSE)
