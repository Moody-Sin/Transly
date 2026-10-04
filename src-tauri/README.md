# src-tauri

> Transly 桌面端原生服务与 Tauri 2 平台集成适配层。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块作为 Tauri 2 后端应用，负责与系统底层 API、全局快捷键、托盘、系统凭据以及本地 HTTP 服务进行交互。

## 目录

- [结构](#结构)
- [安全规则](#安全规则)
- [许可证](#许可证)

## 结构

- **commands**: 对外暴露的 Tauri command 强类型调用接入层。
- **secrets**: 封装 `keyring` 调用原生系统凭据管理器（Windows Credential Manager / macOS Keychain / Linux Secret Service）。
- **server**: 基于 `axum` 搭建的轻量本地 HTTP 服务（严格监听 `127.0.0.1`），为浏览器插件提供无缝批量翻译接口。

## 安全规则

1. 严禁明文落盘 API Key。
2. 本地服务仅监听 `127.0.0.1` 环回接口。

## 许可证

[MIT](../../LICENSE)
