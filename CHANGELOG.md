# 更新日志

本项目的所有重要变更均记录于此文件。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
并且本项目严格遵循 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增
- 引入 `GoogleWebEngine` 免费网页翻译实现，接入核心翻译抽象层。
- 本地扩展通信 HTTP 服务新增 Bearer Token 鉴权中间件与 `/translate` 批量接口。
- 浏览器插件后台通信脚本接入 Token 自动携带机制与批量翻译转发。
- 桌面端前端 UI 接入 Tauri 2 核心 IPC 调用通道与错误反馈。
- 初始化项目工程架构、规范文档与规则。
- 确立桌面端（Tauri 2 + React）与浏览器插件（整页仅译文模式）协作技术方案。
