# Transly 截图翻译 —— 阶段 3 施工交接单（三平台 OCR）

> **交接对象**：接手实施阶段 3（三平台 OCR 打通）的智能体。
> **前置**：阶段 0/1/2 已完成并通过真机验收（Windows 2560×1600 @150% 缩放，三个检查点全 PASS）。
> **工作分支**：`feature/screenshot-translation`（HEAD = `15444f1`）
> **全局设计**：`~/.claude/plans/sunny-swimming-quail.md`；阶段 2 契约见 `HANDOFF-phase2.md`。

---

## 0. 必读：铁律（违反即返工）

**`AGENTS.md` + `HANDOFF-phase2.md` §0 的全部条款继续生效**，本单只列阶段 3 新增/最易踩的几条：

1. **分层**：OCR 核心逻辑（TSV 解析、块聚合、坐标换算、语言包映射）**必须在 `crates/transly-core`**（纯 Rust，零 Tauri 依赖）。`src-tauri` 只做平台 API 调用与薄封装。
   - 判定标准：**能被单元测试覆盖的纯逻辑，一行都不许写在 `src-tauri`**。平台专属代码在 Linux 上被 `cfg` 剔除，**其正确性只能靠评审 + 真机**，所以能下沉的必须下沉。
2. **零落盘**：OCR 全流程内存。**Linux 严禁把截图写临时文件**（`tesseract - -` 管道已实测可用，见 §2.1）。
3. **`unwrap()` 禁令**：用 `?` 或 `unwrap_or_default()` 并注释理由。
4. **`-D warnings`**：平台专属 `use` 必须同样 `cfg` 门控，否则非目标平台 unused import 编译失败。
5. **门禁**：`cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `pnpm run lint` + `pnpm run build`。改命令后必须跑 `cargo run -p src-tauri --bin export_bindings` 并确认 `src/bindings.ts` 同步。
6. **LESSONS 第 11 条（强制动作）**：新增回归测试后，**必须把实现改回缺陷版确认测试 FAILED**，再恢复。提交信息里写明哪几处确认过 FAILED。
7. **子智能体**：实施者用 `deepseek`，评审用 `Gemini 3.8 Flash`，一次不超两个，未完工不汇报。

---

## 1. 范围

### 1.1 交付物

| 交付物 | 路径 | 说明 |
|---|---|---|
| OCR 模块拆目录 | `src-tauri/src/ocr/{mod.rs,windows.rs,macos.rs,linux.rs}` + `README.md` | 由现有单文件 `src-tauri/src/ocr.rs` 拆分；**三段式 `cfg`**（`windows`/`macos`/`not(any(...))`） |
| 统一入口 | 同上 | `recognize_blocks(image: &RgbaImage, target_lang: &str) -> Result<Vec<PixelBox>, OcrErrorDto>`，三平台同签名 |
| 选区映射（纯逻辑） | `crates/transly-core/src/ocr/geom.rs` | `NormRect::map_from_subrect`，把子图归一化坐标映射回全图（见 §4.3） |
| 命令打通 | `src-tauri/src/overlay/mod.rs` | `recognize` 真正调用 OCR 并返回块（当前返回空 `Vec`） |
| 覆盖窗渲染块 | `src/ScreenshotOverlay.tsx` | 已有 `shot-block` 渲染逻辑，本阶段只需让它在真实块下工作（含坐标换算正确性） |
| i18n | `src/i18n/{zh-CN,en}.json` | 如新增文案需中英双份（缺键编译期报错） |

**不属于本阶段**：译文（阶段 4）、入口（阶段 5）、CHANGELOG/README 收尾（阶段 6）。

### 1.2 验收检查点（逐平台）

1. **Linux X11（本机可做，必须做）**：截图 → 框选中英混合文字 → 返回的块文本与位置正确。
2. **Windows（本机可做，必须做）**：同上。本机为 2560×1600 @150%，是 DPI 折算的真实考场。
3. **macOS（无真机，如实说明未做）**：代码只能靠评审 + 交叉 `clippy`。
4. **坐标正确性（核心）**：块的归一化坐标必须落在**原图**上正确的位置——用一个「文字在已知角落」的测试图验证（见 §5.2）。

---

## 2. 已实测核实的硬事实（**不要凭记忆推翻**）

### 2.1 tesseract CLI 契约（本机实测，Ubuntu 22.04 / tesseract 4.1.1）

**★ 最重要的一条：`tsv` 必须放在所有选项之后。**

```bash
# ✅ 正确
tesseract - - -l chi_sim tsv
# ❌ 错误：tsv 被当成输入文件名，不报错但输出乱码
tesseract - - tsv -l chi_sim
```

实测对比（同一张含「你好世界 / 测试文本识别」的图）：

| 用法 | 结果 |
|---|---|
| `tesseract - - tsv -l chi_sim` | 乱码：`Prof` / `F-` / `MR` / `ITATWRA`，**且无任何报错** |
| `tesseract - - -l chi_sim --psm 6 tsv` | **`你好世界` / `测试文本识别`，conf 89–96** ✅ |

**推论：不要因为「识别质量差」就去换语言包。** 本机 2018 版的 `chi_sim.traineddata`（2.4MB）质量完全够用；我一度误判并下载了 28MB 的 `tessdata_best`，实测**不需要**。详见 `LESSONS.md` 第 23 条。

其余已核实：

- **stdin/stdout 管道可用**：`tesseract - - <args> tsv` 从 stdin 读 PNG、向 stdout 写 TSV，**全程零落盘**。
- **中英混合**：`-l chi_sim+eng` 与单独 `-l chi_sim` 都能正确识别混合文本（实测 `Mixed 中英 Text` 全对），**不必为「英文必须单独加载」而复杂化**。
- **语言包映射已实现**：`transly_core::tesseract_lang_arg(target_lang)` → `chi_sim`/`jpn`/`kor`/`eng`，**绝不返回 `"system"`**（那是 Windows 语义）。
- **`TESSDATA_PREFIX` 在 4.1.1 上行为不可靠**（指向父目录与指向 tessdata 目录本身都曾失败）→ **不要依赖该环境变量**；若需要指定目录用 `--tessdata-dir <path>`。
- **TSV 列**：`level page_num block_num par_num line_num word_num left top width height conf text`，只取 `level==5`（词级）；`conf` 是 **0–100 整数**（不是 0–1），`-1` 表示无有效结果。
- **安装**：`apt-get install -y tesseract-ocr tesseract-ocr-chi-sim tesseract-ocr-eng`（本机已装）。CI 的 apt 清单需同步补上（见 §6）。
- **`tesseract` 不存在时**必须返回 `OcrErrorDto::tool_missing("tesseract", <可执行安装指引>)`，**不要**归成 `internal`。

### 2.2 Windows（`windows 0.62.2`，已核实签名）

```rust
// windows::Media::Ocr
OcrResult::Lines() -> Result<IVectorView<OcrLine>>
OcrResult::Text()  -> Result<HSTRING>
OcrLine::Words()   -> Result<IVectorView<OcrWord>>
OcrLine::Text()    -> Result<HSTRING>
OcrWord::BoundingRect() -> Result<Foundation::Rect>   // { X: f32, Y: f32, Width: f32, Height: f32 }
OcrWord::Text()    -> Result<HSTRING>
```

- **坐标空间**：`BoundingRect` 的单位是**喂给 `RecognizeAsync` 那张 `SoftwareBitmap` 的像素**。现有代码在 `clamp_dimensions` 下采样后才创建 bitmap，故必须用**下采样后的 `(nw, nh)`** 作 `NormRect::from_pixels_in` 的分母（用原图尺寸会让所有块整体偏移）。
- **Windows 不提供置信度** → `PixelBox::confidence = None`（`filter_blocks` 对 `None` 放行，不做置信度过滤）。
- 现有 `recognize_clipboard_image` 已实现「读剪贴板 → 下采样 → RGBA→BGRA → SoftwareBitmap → 语言轮询」全链路，**块级实现应复用它**（把返回 `String` 改成返回 `Vec<PixelBox>`），不要另起一套。
- **`ComGuard`（`RoInitialize` MTA）必须保留**，且 OCR 必须在 `spawn_blocking` 里跑。

### 2.3 macOS（`objc2-vision 0.3.2`，已核实）

```rust
VNRecognizeTextRequest::setRecognitionLanguages(&NSArray<NSString>)
VNRecognizeTextRequest::setRecognitionLevel(VNRequestTextRecognitionLevel)
VNRecognizeTextRequest::setUsesLanguageCorrection(bool)
VNImageRequestHandler::performRequests_error(&NSArray<VNRequest>) -> Result<(), Retained<NSError>>
VNRecognizeTextRequest::results() -> Option<Retained<NSArray<VNRecognizedTextObservation>>>
VNRecognizedTextObservation::topCandidates(NSUInteger) -> Retained<NSArray<VNRecognizedText>>
VNRecognizedTextObservation::boundingBox() -> CGRect      // 归一化，**原点左下**
VNRecognizedText::string() -> Retained<NSString>
```

- **坐标是唯一的坑**：Vision 的 `boundingBox()` 归一化且**原点左下、y 向上** → 必须走 `NormRect::from_vision_norm(x, y, w, h)`（`y_top = 1 - y - h`）。**缺这一步会让整块译文垂直镜像，且肉眼不易察觉。**
- `CGImage` → Vision：用 `VNImageRequestHandler` 的 `initWithCGImage` 变体。若该变体在 `objc2-vision` 中不可用，**退路**：把 `RgbaImage` 经 `CVPixelBuffer` 或先编码 PNG 再 `CGImageSourceCreateWithData`（**必须仍走内存**）。
- **`performRequests_error` 返回 `Result`** → 走 `?`，不要 `unwrap()`。
- **非主线程必须包 `objc2::rc::autoreleasepool`**（`spawn_blocking` 的线程是后台线程），否则 ObjC 对象只增不减。
- **语言代码**：Vision 用 BCP-47（`zh-Hans`、`en-US`、`ja`）。现有 `build_candidate_languages` 产出的是 Windows 风格（含 `"system"`），**macOS 必须另做映射，且不得把 `"system"` 传给 Vision**。

---

## 3. 可直接复用的 core 接口（**已实现并测试，不要重写**）

```rust
// crates/transly-core/src/ocr/tsv.rs
pub fn parse_tesseract_tsv(tsv: &str) -> Result<Vec<PixelBox>, OcrTsvError>;
pub fn tesseract_lang_arg(target_lang: &str) -> &'static str;

// crates/transly-core/src/ocr/blocks.rs
pub struct PixelBox { pub x: u32, pub y: u32, pub w: u32, pub h: u32, pub text: String, pub confidence: Option<f32> }
pub fn merge_words_into_lines(words: &[PixelBox], y_tolerance: u32) -> Vec<PixelBox>;
pub fn merge_lines_into_paragraphs(lines: &[PixelBox], max_gap_px: u32, min_x_overlap: f32) -> Vec<PixelBox>;
pub fn filter_blocks(blocks: Vec<OcrBlockDto>) -> Vec<OcrBlockDto>;   // 过滤空白/单字符/纯符号/低置信度，并重排 index
pub struct OcrBlockDto { pub index: u32, pub text: String, pub x: f32, pub y: f32, pub w: f32, pub h: f32, pub line_count: u32, pub confidence: Option<f32> }
pub const MIN_BLOCK_CONFIDENCE: f32 = 0.5;

// crates/transly-core/src/ocr/geom.rs
pub struct NormRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
NormRect::from_pixels_in(px, py, pw, ph, img_w, img_h)   // ⚠️ img_w/h 必须是喂给 OCR 那张图的尺寸
NormRect::from_vision_norm(x, y, w, h)                   // Vision 原点左下 → 本契约原点左上
NormRect::clamped_unit()                                  // 越界钳制的唯一收口点
NormRect::to_pixels_in(img_w, img_h)                      // 映射回像素矩形（钳制到图内）

// crates/transly-core/src/ocr/mod.rs
OcrErrorDto::{no_image, empty_text, no_language_pack, unsupported_platform, wayland_unsupported, tool_missing, capture_failed, permission_denied, too_large, internal}
OcrErrorDto::new(kind, message)
```

**三平台统一的处理链**（顺序不可变）：

```
平台原始词级结果 → Vec<PixelBox>（图内像素坐标）
  → merge_words_into_lines(words, y_tol)        // y_tol 建议取「平均词高」的 0.6 倍
  → merge_lines_into_paragraphs(lines, max_gap, min_overlap)
  → 转 OcrBlockDto（用 from_pixels_in 归一化，分母 = 喂给 OCR 那张图）
  → filter_blocks                                // 同时重排 index
```

---

## 4. 详细设计

### 4.1 `src-tauri/src/ocr/` 拆分

```
src-tauri/src/ocr/
├── mod.rs      # 三段式 cfg 分发 + 统一签名 + README 索引
├── windows.rs  # WinRT OcrEngine（从现有 ocr.rs 迁移）
├── macos.rs    # Vision VNRecognizeTextRequest
├── linux.rs    # 外呼 tesseract 子进程
└── README.md
```

**统一签名（三平台必须一致）**：

```rust
/// 对给定图像做 OCR，返回**图内像素坐标**的词级框。
pub fn recognize_words(image: &image::RgbaImage, target_lang: &str)
    -> Result<Vec<transly_core::PixelBox>, transly_core::OcrErrorDto>;
```

**为什么返回词级而非块级**：词→行→段的聚合是纯逻辑，必须留在 `src-tauri` 之外（可单测）；平台层只负责「拿到词」。

**必须保留 `recognize_clipboard_image` 的对外行为**（剪贴板 OCR 是既有功能，不能回归）：它现在返回拼接后的纯文本，改造后应改为「调用 `recognize_words` → 聚合 → 拼文本」，**输出文本内容必须与改造前一致**。请为此加回归测试。

### 4.2 Linux（`linux.rs`）—— 本机唯一能实跑验证的分支

```rust
// 1) 先判工具存在性，给出可执行的安装指引（不要归 internal）
if which::which("tesseract").is_err() { /* 或自行探测 PATH */
    return Err(OcrErrorDto::tool_missing("tesseract",
        "请安装 tesseract：sudo apt-get install -y tesseract-ocr tesseract-ocr-chi-sim"));
}

// 2) 图像编码为 PNG 到内存（绝不落盘）
let mut png = Vec::new();
image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;

// 3) 子进程管道：stdin 喂 PNG，stdout 读 TSV
//    ⚠️ tsv 必须是最后一个参数（见 §2.1）
let mut child = Command::new("tesseract")
    .args(["-", "-", "-l", tesseract_lang_arg(target_lang), "tsv"])
    .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
    .spawn()?;
// 写 stdin 必须在独立线程或先 write_all 再 drop，否则与读 stdout 死锁
// （子进程输出量大时管道缓冲写满，双方互等）
```

**要点**：
- **`tsv` 放最后**（§2.1）。这是本阶段最容易写错的一处，建议加注释指向 LESSONS 第 23 条。
- **必须避免管道死锁**：先 `write_all` 到 stdin 再 `drop(stdin)`，然后 `wait_with_output()`；或 stdin 写放独立线程。**大图（2560×1600 PNG ≈ 500KB）足以撑爆管道缓冲**，这不是理论风险。
- **非零退出码要带 stderr 内容**报错，便于诊断语言包缺失（`Failed loading language 'chi_sim'` 是最常见的失败）。
- **`spawn_blocking` 包裹**：子进程等待是阻塞的。
- **Wayland 无需在此处理**（捕获层已挡），但**若 OCR 被直接调用而环境无 tesseract**，错误必须是 `tool_missing`。

### 4.3 选区 → 全图坐标映射（**纯逻辑，放 core**）

用户在覆盖窗里框选一块区域；我们**裁剪该区域**再送 OCR（面积小、识别快、上下文聚焦）。但块最终要渲染在**整张冻结帧**上，故必须把「子图内归一化坐标」映射回「全图归一化坐标」。

在 `crates/transly-core/src/ocr/geom.rs` 增加：

```rust
impl NormRect {
    /// 把**子矩形内**的归一化矩形映射回父图（本矩形所在）的归一化坐标。
    ///
    /// `self` 是选区在父图中的位置，`inner` 是子图内的归一化矩形。
    /// 用于「裁剪后 OCR，再把块坐标还原到全图」——不还原会让所有译文块
    /// 挤在选区左上角，且选区越靠右下偏移越大。
    pub fn map_from_subrect(&self, inner: &NormRect) -> NormRect {
        NormRect {
            x: self.x + inner.x * self.w,
            y: self.y + inner.y * self.h,
            w: inner.w * self.w,
            h: inner.h * self.h,
        }
    }
}
```

**必须为它写单测**：反向验证时把 `+` 改成 `-`（或漏乘 `self.w`）必须 FAILED。

### 4.4 覆盖窗打通（`src-tauri/src/overlay/mod.rs`）

现状：`recognize(app, rect, generation) -> Result<Vec<OcrBlockDto>, OcrErrorDto>` **返回空 `Vec`**。

改造：

```rust
pub async fn recognize(app: &AppHandle, rect: NormRect, generation: u32)
    -> Result<Vec<OcrBlockDto>, OcrErrorDto>
{
    // 1) 取会话（含 generation 校验 + clamped_unit，**已实现，保留**）
    // 2) 从会话取冻结帧：目前存的是 PNG 字节 → 需解码回 RgbaImage
    //    （会话存 PNG 而非 RgbaImage 是为了省内存：4K 下 PNG ~500KB vs RGBA 33MB）
    // 3) 按 rect.to_pixels_in(img_w, img_h) 裁剪 → image::imageops::crop_imm
    // 4) spawn_blocking(recognize_words) → 词级框
    // 5) 聚合：merge_words_into_lines → merge_lines_into_paragraphs
    // 6) 归一化：from_pixels_in(..., crop_w, crop_h) → map_from_subrect(rect)
    // 7) filter_blocks → 返回
}
```

**要点**：
- **`generation` 校验与 `clamped_unit` 是信任边界**，改造中不得丢失（阶段 2 已实现且有语义）。
- **裁剪后的图尺寸是归一化分母**（不是全图尺寸）。
- **空结果**：OCR 成功但无文字 → 返回 `OcrErrorDto::empty_text()`，**不是**空 `Vec`（空 `Vec` 会让前端「什么也没发生」，用户无法区分「没文字」与「功能坏了」）。
- **`spawn_blocking` 包裹**，OCR 是秒级阻塞操作。
- 会话里若要缓存块供阶段 4 换语言重译，**本阶段不必做**。

### 4.5 覆盖窗渲染（`src/ScreenshotOverlay.tsx`）

现有 `shot-block` 渲染已按归一化百分比定位（阶段 2 的 E2E 用例 8 已覆盖）。本阶段**只需确认它在真实块下正确**，特别是：

- `block.w`/`block.h` 为 `null` 时的处理（specta 把 `f32` 生成为 `number | null`）。
- 块文字过长时的溢出（已用 `overflow-hidden`）。
- **本阶段不做字号自适应**（属阶段 4 的 `layout.ts`）。

**若发现阶段 2 的渲染有缺陷，直接修**（AGENTS.md「自主修复」）。

---

## 5. 测试要求

### 5.1 core 单测（受 70% 覆盖率门禁，当前 91.71%，**不得下降**）

| 被测 | 必须能捕获的缺陷 |
|---|---|
| `NormRect::map_from_subrect` | 漏加选区原点偏移 / 漏乘选区宽高 |
| 平台语言包映射（macOS BCP-47） | 把 Windows 的 `"system"` 透传给 Vision |

### 5.2 端到端验证（本机可做，**必须做**）

**不要只测「返回非空」。** 必须验证**块的坐标落在正确位置**：

1. 用 `PIL` 造一张**已知布局**的测试图（如：左上角画「Hello」、右下角画「世界」），保存为 PNG。
2. 在测试里直接调 `recognize_words`（Linux 分支）+ 聚合 + 归一化。
3. 断言：文字为「Hello」的块其 `x < 0.3 && y < 0.3`；文字为「世界」的块其 `x > 0.6 && y > 0.6`。
   - 这能同时捕获：**坐标轴搞反**、**y 翻转漏做**、**归一化分母用错**。
4. **反向验证**：把 `from_vision_norm` 的 `1.0 - y - h` 改成 `y`（或在 Linux 路径里人为翻转一次 y），断言必须 FAILED。

> 造图脚本建议放 `scripts/` 并在测试里生成，**不要把 PNG 提交进仓库**（构建产物）。或用 `image` crate 在测试内直接绘制。

### 5.3 真机验收（本机可做 Windows + Linux，macOS 无真机）

**本机是 WSL2 + Windows 11 双环境，两条都能验**：

- **Linux**：本机 X server 是 WSLg 的 XWayland，**不支持根窗口 `GetImage`**（已用 libX11 的 `XGetImage` 独立证明，是服务器限制而非我们的缺陷）。故 **Linux 的捕获无法验，但 OCR 可以**——用 §5.2 的测试图直接喂 `recognize_words`。
- **Windows**：可完整验（捕获 + OCR）。方法见 §7 的真机验收流程。
- **macOS**：**无真机**，如实说明未做。

---

## 6. 需要同步改动的配套文件

| 文件 | 改动 |
|---|---|
| `.github/workflows/ci.yml` | 三处 apt 清单补 `tesseract-ocr tesseract-ocr-chi-sim tesseract-ocr-eng`（Linux 测试要真跑 OCR） |
| `src-tauri/Cargo.toml` | 若 macOS 需要 `objc2-core-foundation` 显式声明（见 `HANDOFF-phase2.md` §3.2 的判定理由） |
| `src-tauri/src/ocr/README.md` | 新建（全局规范要求源码目录有 README） |
| `crates/transly-core/src/ocr/README.md` | 若新增 core 函数需补条目 |
| `LESSONS.md` | 只记反复踩坑/有价值的内容，**简短** |

---

## 7. 坑表（**不要重新踩**）

| 坑 | 说明 |
|---|---|
| **`tsv` 参数顺序** | 必须在最后。写错**不报错只输出乱码**（§2.1，LESSONS 23） |
| **tesseract 管道死锁** | 大图下必须先写完 stdin 再读 stdout，否则双方互等（§4.2） |
| **Windows `BoundingRect` 分母** | 必须用**下采样后**的 `(nw, nh)`，用原图尺寸会让所有块偏移（§2.2） |
| **Vision `boundingBox` 原点左下** | 必须 `from_vision_norm`，缺则整块垂直镜像（§2.3） |
| **Vision 的 `"system"`** | `build_candidate_languages` 是 Windows 专用，**不得**传给 Vision（§2.3） |
| **macOS 缺 autoreleasepool** | `spawn_blocking` 线程必须包，否则 ObjC 对象泄漏（§2.3） |
| **裁剪后归一化分母** | 是**裁剪图**尺寸，不是全图；映射回全图用 `map_from_subrect`（§4.3） |
| **`recognize` 返回空 Vec** | 无文字必须返回 `empty_text()` 错误，否则用户无法区分「没文字」与「功能坏了」（§4.4） |
| **`recognize_clipboard_image` 回归** | 剪贴板 OCR 是既有功能，改造后输出文本必须一致（§4.1） |
| **`OcrErrorDto::message` 是硬编码中文** | 前端必须用 `t.ocrError[kind]` 映射，不能直接透传（阶段 2 已踩，见提交 `72b9247`） |
| **specta 把 `f32` 生成为 `number \| null`** | 前端写算术前先看生成的 `bindings.ts`（LESSONS 20） |
| **Windows 分支在本机 clippy 看不到** | 用 `/tmp` 隔离 scratch crate 交叉 `clippy --target x86_64-pc-windows-msvc -- -D warnings`（LESSONS 19） |

---

## 8. 开工前自查

- [ ] `git log --oneline -3` 确认 HEAD 是 `15444f1`，`git status` 干净
- [ ] 基线门禁：`cargo test --workspace` 应得 **252 passed**
- [ ] 读过 `LESSONS.md` 第 11、12、19、20、23 条
- [ ] **手工跑通一次** `tesseract - - -l chi_sim tsv < 某张含中文的 PNG` 并肉眼核对输出（§2.1）
- [ ] 知道本机已装 tesseract 4.1.1 + chi_sim/eng 语言包

## 9. 完成后交付要求

1. 全部门禁命令通过，并把**实际输出关键行 + 数字**附在汇报里。
2. §1.2 的验收检查点在**能做的平台**上做过，**如实说明哪些未做及原因**。
3. 反向验证执行过，说明**哪几处确认 FAILED**。
4. 新增源码目录补 `README.md`。
5. 汇报里明确列出**未完成项与已知限制**，不要报「全部完成」而掩盖缺口。

---

## 附：阶段 4–6 概览（**本单不要求**）

| 阶段 | 内容 | 前置 |
|---|---|---|
| **4** 批量翻译+叠译文（2d） | `translate_screenshot_blocks`（用 `translate_batch_with_cache`）；前端 `layout.ts`（`fitFontSize`/`toPercentStyle`/`clampRenderedBox`）；译文层 + 还原原文 + 引擎/语言下拉 + 复制 + 单块重试 | 阶段 3 |
| **5** 三入口+降级探测（1.5d） | 主窗口按钮、托盘项、第三个全局快捷键（默认 `Alt+Shift+S`）；`ShortcutStatusSlot` 加第三槽位；`get_screenshot_capability()` | **仅依赖阶段 2**（可提前做） |
| **6** E2E+文档（1.5d） | E2E 用例扩展；CHANGELOG/README/PROJECT_RULES/各 README | 全部 |
