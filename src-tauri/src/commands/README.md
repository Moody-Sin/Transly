# src-tauri/src/commands (Tauri IPC 命令接入层)

> 前端与 Rust 平台适配层之间的强类型 IPC 命令薄封装。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块集中承载所有由 `#[tauri::command]` 标记的跨进程 IPC 命令，作为前端 React 界面与 Rust 核心逻辑的交互桥梁。所有命令签名均通过 `tauri-specta` 自动导出至前端 `src/bindings.ts`。

## 目录

- [职责与范围](#职责与范围)
- [命令类别](#命令类别)
- [编写规范](#编写规范)
- [测试](#测试)
- [许可证](#许可证)

## 职责与范围

- **薄层封装**：本模块不实现复杂业务逻辑，仅做参数校验、调用 `transly-core` 或平台层 API，并返回结构化数据。
- **线程模型**：凡涉及 Keyring 凭据读取、SQLite 数据库操作、注册表访问等同步阻塞调用，一律使用 `async fn` + `tokio::task::spawn_blocking` 调度至系统线程池，防止主线程与事件循环卡顿。

## 命令类别

1. **翻译与流式处理**：`translate_text`、`translate_stream`（SSE 增量广播）、`cancel_stream_task`（物理取消在途任务）、`detect_text_language`（离线微秒级探测）。
2. **历史记录与缓存**：`get_translation_history`、`search_translation_history`、`delete_history_entry`、`clear_translation_history`、`export_translation_history`（后台流式写盘导出 CSV/JSON）。
3. **术语库管理**：`list_glossary`、`add_glossary_term`、`delete_glossary_term`、`update_glossary_term`。
4. **快捷键与剪贴板**：`get_shortcut` / `set_shortcut`、`get_replace_shortcut` / `set_replace_shortcut`（带原子回滚保护）、`get_clipboard_monitor` / `set_clipboard_monitor`。
5. **系统设置与凭据**：`is_api_key_configured` / `set_api_key` / `delete_api_key`、`get_llm_config` / `save_llm_config`、`get_proxy_config` / `set_proxy_config`、`get_preferred_replace_engine` / `save_preferred_replace_engine`。
6. **扩展配对与浮窗交互**：`get_server_token` / `regenerate_server_token`、`approve_pairing` / `reject_pairing`、`take_pending_float_request`、`set_float_sticky`。
7. **OCR 图像识别**：`ocr_clipboard_image`（从剪贴板读取位图并调用平台 OCR 硬件加速识别）。

## 编写规范

- 禁止在命令非测试代码中裸用 `unwrap()` / `expect()`。
- 前后端通信大整数必须使用 `u32` / `i32`，禁止导出 `u64` / `i64` 以防止前端 V8 `BigInt` 序列化崩溃。
- 返回值遵循 `Result<T, String>` 或结构化 DTO，便于 `tauri-specta` 生成强类型。

## 测试

```sh
cargo test -p src-tauri commands::tests
```

## 许可证

[商业专有许可](../../../LICENSE)
