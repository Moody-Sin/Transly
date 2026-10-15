# scripts (工程自动化与端到端测试套件)

> Conventional Commits 规范校验、桌面端前端全交互 UI E2E 与跨进程浏览器扩展 E2E 测试脚本。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录承载 Transly 项目的工程自动化辅助脚本与真实浏览器/端到端自动化测试套件。遵循“优先使用端到端测试检验真实调用契约”的核心原则，在 CI 与本地提供高保真的质量验证。

## 目录

- [脚本构成](#脚本构成)
- [运行指南](#运行指南)
- [许可证](#许可证)

## 脚本构成

- **check-commit-msg.mjs**：Conventional Commits 提交信息规范校验脚本。用于 `commit-msg` Git 钩子拦截非标准提交。
- **test-desktop-ui-e2e.mjs**：基于 Playwright 的桌面端前端全交互真实渲染 E2E 测试套件（`pnpm run test:e2e:ui`）。全真模拟 37 个 Tauri IPC 命令与事件通道注册表，覆盖主窗口挂载、主题即时切换、即输即译、历史记录过滤与导出、设置中心与术语库管理、浏览器扩展配对申请弹窗以及迷你划词悬浮窗（`#float`）等核心交互场景。
- **test-extension-e2e.mjs**：跨进程真实 Rust 本地服务与浏览器扩展端到端联调测试套件（`pnpm run test:e2e:extension`）。由轻量测试服务器二进制（`transly_test_server`）托管真实 TCP 回环服务与测试网页，Playwright 启动无头 Chromium 挂载解压扩展，全链路闭环验证整页仅译文替换、富文本行内保真、开放模式 Shadow DOM 穿透、双语对照模式以及一键还原。

## 运行指南

```sh
# 运行桌面端前端 UI 全交互 E2E 测试
pnpm run test:e2e:ui

# 运行浏览器扩展真实端到端跨进程集成测试
pnpm run test:e2e:extension

# 一键串联运行全部端到端测试
pnpm run test:e2e
```

## 许可证

[商业专有许可](../LICENSE)
