# `bin` —— 辅助二进制入口

不属于桌面应用本身的额外可执行入口，仅供构建与测试使用。

## 目录

| 文件 | 用途 |
|---|---|
| `export_bindings.rs` | 生成 `src/bindings.ts`（`pnpm run generate:bindings` 调用） |
| `transly_test_server.rs` | E2E 专用的真实本地服务进程（浏览器插件端到端测试拉起它） |

## 契约

- **`export_bindings` 不重复实现路径逻辑**：唯一实现在 `src_tauri::export_bindings`
  （`src/lib.rs`），本入口只调它。CI 用 `git diff --exit-code src/bindings.ts` 校验同步，
  因此绑定文件必须由代码生成，不得手改。
- **`transly_test_server` 必须启动真实路由**：真实 Host 守卫、真实 Bearer 鉴权、
  真实 CORS 预检、真实切批校验，只把翻译引擎换成零网络零凭据的 Stub。
  它的价值就在于「除引擎外的每一层都是真的」——若为了省事把鉴权或 CORS 桩掉，
  `test-extension-e2e.mjs` 报出的「通过」就不再能证明线上路径可用。

## 硬约束

- 两个入口都**不得**出现在发布产物里：它们只通过 `cargo run --bin <name>` 或
  `cargo build --bin <name>` 使用，`tauri build` 只打包 `transly` 主二进制。
- `transly_test_server` 的 Stub 引擎**不得**读系统凭据库，否则 CI（无 Keyring 服务）会挂。
