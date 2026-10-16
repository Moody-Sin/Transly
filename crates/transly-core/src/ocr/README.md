# crates/transly-core/src/ocr (OCR 辅助运算与数据契约)

> 纯 Rust 实现的零平台依赖 OCR 数据契约、归一化坐标契约、文字块几何聚合、tesseract 输出解析与大图内存安全保护。

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

本模块为 Transly OCR 提供平台无关的核心算法辅助、统一错误契约 (`OcrErrorDto`) 以及大图内存安全保护。真实的平台硬件加速实现位于宿主适配层（`src-tauri/src/ocr/`）与屏幕捕获层（`src-tauri/src/screencap/`）。

## 目录

- [结构与功能](#结构与功能)
- [核心机制](#核心机制)
- [测试](#测试)
- [许可证](#许可证)

## 结构与功能

### `mod.rs` —— 错误契约与位图预算

- **OcrErrorKind 与 OcrErrorDto**：跨平台统一错误类型，导出 TS 绑定供前端直接使用。含 `tool_missing` / `capture_failed` / `permission_denied` / `too_large` 四个截图链路专用变体。
- **build_candidate_languages**：根据目标翻译语言智能推导 OCR 候选语言包优先级（Windows WinRT 专用；其 `"system"` 语义**不可**传给 tesseract `-l`）。
- **rgba_to_bgra_in_place**：原地 RGBA8 转换为 BGRA8 字节序，消除平台 API 调用时的全量二次内存拷贝。
- **bgrx_to_rgba_in_place**：原地 BGRX → RGBA8（交换 R/B 并把填充字节改写为不透明 alpha）。三个捕获后端给出的都是低位在前的 BGR 序缓冲；漏写 alpha 会编码出一张全透明 PNG，漏交换会得到红蓝互换的冻结帧（灰度界面上肉眼不可察觉）。
- **MAX_OCR_SOURCE_PIXELS 与 is_within_ocr_pixel_budget**：4000 万像素防 OOM 硬预算拦截，在执行任何像素拷贝前把关剪贴板恶意或超大位图。
- **clamp_dimensions**：等比下采样计算，确保位图尺寸符合系统 OCR 引擎最大尺寸上限。

### `geom.rs` —— 归一化坐标契约（全链路地基）

- **NormRect**：`[0,1]²` 归一化矩形，原点左上。截图的**物理像素**与覆盖窗的 **CSS 逻辑像素**在任何 DPI 下都不相等，一旦像素坐标过 IPC，缩放系数就必须前后端同步维护，DPI 差异必然漂移。
- **from_pixels_in**：⚠️ 分母必须是**喂给 OCR 那张图**的尺寸（下采样后），用原图尺寸会让所有块坐标整体偏移。
- **map_from_subrect**：把**裁剪子图内**的归一化矩形映射回全图（选区裁剪后 OCR 的坐标还原）。⚠️ 不还原会让所有译文块挤在选区左上角，且选区越靠右下偏移越大；退化选区（宽/高为 0）返回零矩形。
- **from_vision_norm**：macOS Vision 的 `boundingBox()` 是原点左下、y 向上，缺这步 y 翻转会让整块译文垂直镜像。
- **find_monitor_for_point**：在显示器列表中定位光标所在的物理矩形。

### `blocks.rs` —— 文字块几何聚合

- **merge_words_into_lines / merge_lines_into_paragraphs**：词 → 行 → 段的几何聚类。逐词送翻译会因上下文缺失导致语序崩坏。
- **words_to_blocks**：三平台共用的收口点 —— 词级框 → 行 → 段 → 归一化块（含聚合参数具名常量）。平台层只负责「拿到词」，聚合与归一化必须留在 core 才能获得测试覆盖（平台代码在非目标平台上会被 `cfg` 剔除）。⚠️ `img_w`/`img_h` 必须是**喂给 OCR 那张图**（裁剪后子图）的尺寸。
- **filter_blocks**：过滤空白、单字符、纯符号、低置信度块；**不过滤会让重复译文叠印**。返回前会把 `index` 重排为与数组下标一致。
- **plan_block_batches**：复用 `plan_batch_ranges` 切批，与扩展端共用同一份 `MAX_BATCH_*` 上限契约。

### `tsv.rs` —— tesseract 输出解析

- **parse_tesseract_tsv**：解析 tesseract 的 TSV 词级结果。**必须 `splitn(12, '\t')`**，`split('\t')` 会截断含制表符的文本列。
  ⚠️ 命令行的 `tsv` 是**输出格式配置名，必须放在所有选项之后**（`tesseract - - -l chi_sim tsv`）。写成 `tesseract - - tsv -l chi_sim` 会被当成输入文件名解析，**不报错但输出乱码**（实测 `你好世界` → `Prof`）。详见 `LESSONS.md` 第 23 条。
- **tesseract_lang_arg**：把目标语言映射为 `-l` 语言包名，绝不返回 `"system"`（那是 Windows 的语义）。

### `screencap.rs` —— 捕获的纯判定逻辑

- **detect_wayland_session**：以 `XDG_SESSION_TYPE` 为权威判据，仅在缺失时回落 `WAYLAND_DISPLAY`（XWayland 下只看后者会误禁用功能）。
- **map_capture_error**：把平台原始错误映射为可操作错误种类，**绝不返回 `Internal`**（那会让用户看到无信息错误）。
- **ScreenshotSessionDto**：跨 IPC 传递的会话元数据，**不含图像字节**（图像走自定义 URI 协议）。

## 核心机制

1. **防 OOM 像素预算守卫**：剪贴板位图尺寸由外部放入程序决定，若不对像素总量把关，20000×20000 的图像在 `to_vec()` 时会直接分配 1.6 GB 内存导致进程崩溃。本模块在入口处以 40,000,000 像素上限进行 O(1) 拦截。
2. **归一化坐标作为唯一坐标契约**：消除全链路 DPI 换算。前端只需 `left: x*100%`，无需知道任何缩放系数。裁剪后 OCR 的块坐标经 `map_from_subrect` 还原到全图，前端永远只见「相对全图」的坐标。
3. **零平台假设**：不引入 WinRT / Vision / xcb 依赖，保持 core crate 的跨平台纯度与独立可测性（平台代码在 `src-tauri` 的 cfg 门控模块内）。

## 测试

```sh
cargo test -p transly-core ocr
```

覆盖率门禁：`cargo llvm-cov -p transly-core --fail-under-lines 70`。

## 许可证

[Apache-2.0](../../LICENSE)
