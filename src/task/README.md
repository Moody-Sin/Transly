# src/task (共享任务状态机)

> 桌面端四类入口共用的任务状态机、代际隔离与结构化错误回落。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

主输入框、划词、剪贴板监听与浮窗四个入口共用同一套任务状态机，避免各自维护一套取消/重试语义导致行为漂移。本目录只包含纯逻辑与一个 React 钩子，不含任何 UI。

## 目录

- [模块](#模块)
- [状态机](#状态机)
- [测试](#测试)
- [许可证](#许可证)

## 模块

- **taskState.ts**：`TaskState` 联合类型、状态迁移函数 `nextState`、可重试判定 `canRetry` / `canRetryExecution`。`ErrorKind` 直接复用 tauri-specta 生成的绑定，避免前后端枚举各自维护。
- **generation.ts**：代际闸门 `applyIfCurrent`。取消或新请求后，旧代响应一律返回 `null`，绝不覆盖新代状态。
- **outcome.ts**：`toFailureOutcome`。IPC 层抛错（而非返回 `status === 'error'`）时回落为可重试的 `internal` 错误，否则 UI 会永久停留在 running。
- **useTaskRunner.ts**：`useTaskRunner` 钩子。串联防抖、代际隔离、划词/剪贴板事件订阅（按 `request_id` 去重）、浮窗粘滞控制。

## 状态机

```
idle ──submit──> running ──resolve──> succeeded
                    │      └─reject──> failed
                    └──cancel──> cancelled
```

- 只有 `failed` 且错误 `retryable` 时才提供重试入口。
- `cancel` 只作废在途响应，已应用的译文保持（取消 ≠ 还原）。
- 组件卸载时递增代际，作废全部在途请求。

## 测试

```sh
pnpm run test:task
```

使用 `node:test` + `--experimental-strip-types`，无需构建步骤。

## 许可证

[MIT](../../LICENSE)
