# src (桌面端前端)

> 基于 React + TypeScript + Tailwind CSS 的 Transly 桌面端交互界面。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录为 Transly 桌面客户端的渲染层，只负责 UI 呈现与用户交互，绝不直接发起外部翻译网络请求，所有翻译与系统级行为均通过 Tauri IPC 委托给后端处理。

## 目录

- [结构](#结构)
- [开发规范](#开发规范)
- [构建](#构建)
- [许可证](#许可证)

## 结构

- **components**: 通用 UI 组件与原子部件。
- **pages**: 主界面、设置窗口、划词气泡浮窗等视图。
- **hooks**: 状态与交互钩子。
- **stores**: 全局客户端状态。
- **i18n**: 国际化文案（`zh-CN` / `en`）。

## 开发规范

- 严格模式 TypeScript，严禁 `any`。
- 严禁手写拼接 IPC 调用，统一走生成类型绑定。

## 构建

```sh
pnpm build
```

## 许可证

[MIT](../LICENSE)
