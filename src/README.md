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

- **App.tsx**: 桌面端主工作台界面、引擎与翻译风格切换、系统设置中心。
- **FloatWindow.tsx**: 划词悬浮迷你窗独立入口（在 `main.tsx` 根据 `#float` 路由分流），轻量级冷启动，支持失焦关闭、图钉置顶与自由拖拽。
- **HistoryPanel.tsx**: 翻译历史记录面板，支持实时模糊搜索、单条删除与全量 CSV/JSON 流式导出。
- **ShortcutRecorder.tsx**: 全局快捷键智能录制器，支持修饰键捕获与冲突检测。
- **task**: 共享任务状态机、代际隔离、结构化错误回落与 `useTaskRunner` 入口钩子，附 `node:test` 单测。
- **i18n**: 国际化文案字典（`zh-CN` / `en`）与编译期强类型约束。
- **utils**: 前端通用辅助工具（如 Web Speech API 原生发音代码规范化）。
- **bindings.ts**: tauri-specta 生成的强类型 IPC 绑定，禁止手工编辑。

## 开发规范

- 严格模式 TypeScript，严禁 `any`。
- 严禁手写拼接 IPC 调用，统一走生成类型绑定。

## 构建

```sh
pnpm build
```

## 许可证

[MIT](../LICENSE)
