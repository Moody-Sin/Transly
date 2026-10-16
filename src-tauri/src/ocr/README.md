# `ocr` —— 三平台文字识别

截图翻译与剪贴板 OCR 的识别层。三平台各自实现 `recognize_words`，统一产出
**图内像素坐标**的词级框；词 → 行 → 段 → 归一化块的聚合下沉在 `crates/transly-core`。

## 目录

| 文件 | 平台 | 实现 |
|---|---|---|
| `mod.rs` | — | 三段式 `cfg` 分发、`recognize_blocks`、剪贴板 OCR 入口 |
| `windows.rs` | Windows | WinRT `Windows.Media.Ocr`（`OcrWord::BoundingRect`） |
| `macos.rs` | macOS | Vision `VNRecognizeTextRequest` |
| `linux.rs` | Linux | 外呼 `tesseract` 子进程（管道，零落盘） |

## 契约

```rust
/// 图内像素坐标的词级框。
pub fn recognize_words(image: &RgbaImage, target_lang: &str)
    -> Result<Vec<PixelBox>, OcrErrorDto>;

/// 整图的归一化块（三平台共用收口点）。
pub fn recognize_blocks(image: &RgbaImage, target_lang: &str)
    -> Result<Vec<OcrBlockDto>, OcrErrorDto>;

/// 只对**指定区域**识别，并把块坐标映射回整图。
pub fn recognize_in_region(full: &RgbaImage, region: NormRect, target_lang: &str)
    -> Result<Vec<OcrBlockDto>, OcrErrorDto>;   // 在 overlay 层
```

- **同步阻塞**（秒级），调用方必须 `spawn_blocking`。
- 引擎可用但无有效文字 → 返回 `OcrErrorDto::empty_text()`，**不是空 `Vec`**
  （空 `Vec` 会让前端「什么也没发生」，用户无法区分「没文字」与「功能坏了」）。

## 硬约束

- **零落盘**：Linux 走 `tesseract - -` 管道（PNG 进 stdin、TSV 出 stdout），
  不产生临时文件。这是 `README.md` 对外的「全流程内存零落盘」承诺。
- **三段式 `cfg`**（`windows` / `macos` / `not(any(...))`）。写成「`not(windows)`
  即 Linux」会把 tesseract 代码编进 macOS。平台专属 `use` 同样要门控，
  否则 `-D warnings` 下会因 unused import 失败。
- **语言包映射各平台不同**，不得混用：
  - Linux：`tesseract_lang_arg` → `chi_sim`/`jpn`/`kor`/`eng`
  - Windows：`build_candidate_languages` → `zh-Hans-CN`/`system`/…（含 `"system"`）
  - macOS：Vision 用 BCP-47（`zh-Hans`/`en-US`/`ja`），**不认 `"system"`**

## 已核实的坑

- **★ tesseract 的 `tsv` 必须是最后一个参数**：写成 `tesseract - - tsv -l chi_sim`
  会被当成输入文件名解析，**不报错但输出乱码**（实测「你好世界」→ `Prof`）。
  见 `LESSONS.md` 第 23 条。
- **tesseract 管道死锁**：大图必须先写完 stdin 并 `drop`，再读 stdout，
  否则双方互等（PNG 数百 KB 足以撑满管道缓冲）。
- **Windows `BoundingRect` 的归一化分母**必须是**下采样后**的 `(nw, nh)`，
  用原图尺寸会让所有块整体偏移。
- **Vision `boundingBox()` 原点在左下、y 向上**，必须翻转（`y_top = 1 - y - h`），
  缺这一步会让整块译文垂直镜像。
- **macOS 缺 `autoreleasepool`** 会导致 ObjC 对象泄漏（后台线程必须包）。
- **裁剪后 OCR 必须把坐标映射回整图**（`NormRect::map_from_subrect`），
  否则所有译文挤在选区左上角。`tests/ocr_e2e.rs` 已用「只在右下角画字」的用例钉住。
- **同行词不得按垂直位置重排**：同一行词的垂直中心会因字体下伸部错开 1–2px，
  按 `center_y` 排序合并会把 `Hello world again` 变成 `Hello again world`
  （`merge_words_into_lines` 已修复，见 `LESSONS.md`）。

## 测试

- `crates/transly-core/src/ocr/blocks.rs`：聚合与归一化的纯逻辑单测。
- `tests/ocr_e2e.rs`：**真实调用 tesseract**，用内置点阵字体造「左上 HELLO /
  右下 WORLD」的已知布局图，断言块落在正确象限。仅在 Linux 运行
  （CI 已装 `tesseract-ocr*`）。这是唯一能捕获「坐标轴搞反」「y 未翻转」
  「归一化分母用错」「漏映射回整图」的测试。
