# crates/transly-core/src/ocr (OCR 辅助运算与数据契约)

> 纯 Rust 实现的零平台依赖 OCR 数据契约、像素预算守卫与内存位图变换。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块为 Transly OCR 提供平台无关的核心算法辅助、统一错误契约 (`OcrErrorDto`) 以及大图内存安全保护。真实的平台硬件加速实现位于宿主适配层 (`src-tauri/src/ocr.rs`)。

## 目录

- [结构与功能](#结构与功能)
- [核心机制](#核心机制)
- [测试](#测试)
- [许可证](#许可证)

## 结构与功能

- **OcrErrorKind 与 OcrErrorDto**：跨平台统一错误类型，导出 TS 绑定供前端直接使用。
- **build_candidate_languages**：根据目标翻译语言智能推导 OCR 候选语言包优先级（中英双向对齐与系统语言兜底）。
- **rgba_to_bgra_in_place**：原地 RGBA8 转换为 BGRA8 字节序，消除平台 API 调用时的全量二次内存拷贝。
- **MAX_OCR_SOURCE_PIXELS 与 is_within_ocr_pixel_budget**：4000 万像素防 OOM 硬预算拦截，在执行任何像素拷贝前把关剪贴板恶意或超大位图。
- **clamp_dimensions**：等比下采样计算，确保位图尺寸符合系统 OCR 引擎最大尺寸上限。

## 核心机制

1. **防 OOM 像素预算守卫**：剪贴板位图尺寸由外部放入程序决定，若不对像素总量把关，20000×20000 的图像在 `to_vec()` 时会直接分配 1.6 GB 内存导致进程崩溃。本模块在入口处以 40,000,000 像素上限进行 O(1) 拦截。
2. **零平台假设**：不引入 Windows WinRT 或 macOS Vision 依赖，保持 core crate 的跨平台纯度与独立可测性。

## 测试

```sh
cargo test -p transly-core ocr
```

## 许可证

[MIT](../../../../LICENSE)
