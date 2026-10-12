# src/utils (前端通用辅助工具)

> 桌面端前端共享通用辅助工具函数集合。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本目录承载跨视图（主窗口、悬浮窗、历史面板）复用的纯工具函数与系统级交互桥接。

## 目录

- [模块](#模块)
- [测试](#测试)
- [许可证](#许可证)

## 模块

- **speech.ts**：Web Speech API 规范化封装。将源语言代码（含 `"auto"`、未配置或小写缩写）智能映射至标准 BCP-47 区域标签（如 `zh-CN`、`en-US`、`ja-JP`），杜绝因语言标记非法导致浏览器发音静默。

## 测试

单元测试通过 Node.js 原生测试运行器执行：

```sh
pnpm run test:task
```

## 许可证

[MIT](../../LICENSE) © Transly Authors
