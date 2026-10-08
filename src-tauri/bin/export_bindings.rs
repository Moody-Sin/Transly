//! 导出 tauri-specta 强类型绑定（供 CI 与开发者手动刷新 `src/bindings.ts`）。
//!
//! 路径与写入逻辑统一在 `src_tauri::export_bindings`，本入口不重复实现。

fn main() {
    src_tauri::export_bindings().expect("导出 TypeScript 绑定失败");
}
