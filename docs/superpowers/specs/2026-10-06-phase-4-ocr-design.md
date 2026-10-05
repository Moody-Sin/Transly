# Transly 阶段四：剪贴板图片 OCR 与翻译闭环设计规范

- **状态**：已评审确认
- **创建日期**：2026-10-06
- **关联规格**：`docs/superpowers/specs/2026-07-17-v0.2.0-design.md`（阶段四：图片/截图 OCR 翻译闭环）

---

## 1. 目标与范围

### 1.1 核心目标
打通 Transly 桌面端“剪贴板图片识别 $\to$ 文本填充 $\to$ 即时翻译”的完整交互闭环，落实阶段四规划。

### 1.2 功能范围（Windows 优先）
1. **输入来源**：系统剪贴板图片。用户使用系统截图工具（Windows `Win+Shift+S`、微信/QQ 截图等）或复制网页图片后，一键提取识别。
2. **本地原生 OCR**：Windows 10/11 平台基于系统内置的 `Windows.Media.Ocr` 驱动，零外部网络依赖，免打包大型离线模型。
3. **定向候选容错轮询**：依据 `targetLang` 先验定向首选语言引擎，去重轮询备选语言，规避非中英场景识别死角及相同引擎重复空转。
4. **共享流水线复用**：识别所得文本灌入主输入框，以 `debounceMs: 0` 立即接入现有代际隔离的 `useTaskRunner`，复用翻译引擎池、缓存与术语库。
5. **明确平台边界**：非 Windows 平台编译明确的平台未支持存根，不返回假成功，不阻断跨平台 CI 构建。

### 1.3 明确不纳入本阶段的特性（已知限制）
- 不在此阶段引入第三方截图框选依赖（如 `tauri-plugin-screenshots` 或 `xcap`），避免处理多屏 DPI、全局悬浮遮罩及权限弹窗等额外系统复杂度。
- 不引入独立的“图片翻译”主窗口 Tab 页签，不撕裂 `TaskRunner` 单一状态机。
- 不做图片原图预览与历史持久化，内存阅后即焚，保障隐私。
- 不枚举系统所有识别器做盲试（YAGNI）。

---

## 2. 架构设计与分层约束

```
┌─────────────────────────────────────────────────────────────┐
│ 前端 UI (src/App.tsx)                                       │
│ - 输入框底栏 [识别剪贴板图片] 按钮                          │
│ - 独立 OCR 错误反馈（与翻译错误分类隔离）                  │
│ - setSourceText(text) + runner.run(text, {debounceMs: 0})   │
└──────────────────────────────┬──────────────────────────────┘
                               │ Tauri IPC (tauri-specta)
┌──────────────────────────────▼──────────────────────────────┐
│ 宿主层 (src-tauri)                                          │
│ - commands::ocr_clipboard_image(target_lang)                │
│ - spawn_blocking + WinRT Apartment (MTA) 治理               │
│ - tauri_plugin_clipboard_manager::read_image()              │
│ - 尺寸安全下采样 (image::imageops::resize)                  │
│ - 像素重排 RGBA -> BGRA8 & 装配 SoftwareBitmap              │
└──────────────────────────────┬──────────────────────────────┘
                               │ 纯 Rust 调用
┌──────────────────────────────▼──────────────────────────────┐
│ 核心抽象与契约 (crates/transly-core)                        │
│ - OcrEngine trait 与 OcrRequest / OcrResponse               │
│ - OcrErrorKind / OcrError 专属错误契约                      │
│ - 跨平台存根与错误分类                                      │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. 详细设计

### 3.1 依赖与配置调整

在 `src-tauri/Cargo.toml` 中为 Windows 目标配置显式依赖：

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.62", features = [
    "Media_Ocr",
    "Graphics_Imaging",
    "Storage_Streams",
    "Win32_System_WinRT",
    "Globalization",
] }
image = { version = "0.25", default-features = false, features = ["png"] }
```

### 3.2 错误契约定义 (`crates/transly-core/src/error.rs`)

彻底消除将 OCR 失败伪装为 `UnsupportedLanguage` 的分类污染，在核心层引入强类型 OCR 错误定义：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum OcrErrorKind {
    NoImage,             // 剪贴板中无有效位图数据
    NoLanguagePack,      // 系统未安装对应 OCR 语言包组件
    EmptyText,           // 识别完成但图片中未提取出任何有效文本
    DimensionExceeded,   // 图片尺寸超出 Windows OCR 处理极限且缩放失败
    UnsupportedPlatform, // 当前操作系统暂未支持平台原生 OCR
    Internal,            // COM 初始化、位图转换或内部驱动异常
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OcrErrorDto {
    pub kind: OcrErrorKind,
    pub message: String,
}
```

### 3.3 Windows Media OCR 驱动引擎 (`src-tauri/src/platform/ocr_windows.rs`)

#### 3.3.1 线程安全与 COM Apartment
`tauri-plugin-clipboard-manager::read_image()` 严禁在主线程阻塞调用；同时 `Windows.Media.Ocr` 依赖 WinRT Apartment。全流程必须置于 `tauri::async_runtime::spawn_blocking` 中执行：
1. 线程入口显式调用 `windows::Win32::System::WinRT::RoInitialize(RO_INIT_MULTITHREADED)`。
2. 利用 RAII Guard 结构体，确保在函数任意路径返回时自动触发 `RoUninitialize()`。

#### 3.3.2 尺寸防卫与等比下采样
Windows `OcrEngine::MaxImageDimension()` 通常硬限制为 2600px。超过该尺寸的截图传入将直接被 Windows 拒绝抛错：
- 读取图片宽高 $(w, h)$；
- 若 $\max(w, h) > \text{MaxImageDimension}$，使用 `image::imageops::resize` 采用平滑滤镜将其等比缩放至上限阈值之内。

#### 3.3.3 像素转换与 SoftwareBitmap 装填
Tauri `clipboard.read_image()` 提取的数据为 RGBA 8-bit 排布：
- 转换缓冲区块，就地交换红蓝通道：$R \leftrightarrow B$，变为 `BitmapPixelFormat::Bgra8`；
- 调用 `windows::Storage::Streams::DataWriter` 将字节载入 `InMemoryRandomAccessStream`；
- 装配为 `SoftwareBitmap` 并指定 `BitmapAlphaMode::Premultiplied`。

#### 3.3.4 去重候选语言轮询执行器
为消除外文截图识别盲区并杜绝重复尝试，按以下顺序构建候选列表：
```rust
let primary = if target_lang == "zh-CN" { "en-US" } else { "system" };
let candidates = [primary, "system", "en-US"];
// 保持顺序去重，最多 2 次尝试，最少 1 次
let mut tried = Vec::new();
```

针对每一个候选：
- `'en-US'`: 构造 `windows::Globalization::Language::CreateLanguage("en-US")` 并调用 `OcrEngine::TryCreateFromLanguage(&lang)`；
- `'system'`: 调用 `OcrEngine::TryCreateFromUserProfileLanguages()`；
- 若引擎初始化返回 `null`（对应语言组件未安装），静默记录并尝试下一候选；
- 若引擎就绪，调用 `RecognizeAsync(&bitmap)?.get()`；
- 若提取文本 `text.trim()` 非空，则立即成功退出轮询；
- 若所有可用引擎均吐出空文本，返回 `OcrErrorKind::EmptyText`；
- 若轮询涉及的所有语言引擎均因缺语言包无法创建，返回 `OcrErrorKind::NoLanguagePack`。

### 3.4 宿主 IPC 命令 (`src-tauri/src/commands/mod.rs`)

提供强类型命令，并纳入 `export_bindings.rs` 重新生成 `bindings.ts`：

```rust
#[tauri::command]
#[specta::specta]
pub async fn ocr_clipboard_image(
    target_lang: String,
    app: tauri::AppHandle,
) -> Result<String, OcrErrorDto>
```

- 原 `ocr_image(image_base64: String)` 命令予以保留，内部接入相同的 Windows OCR 引擎，为后续文件拖拽预留底层支持。

### 3.5 图像生命周期与隐私约束
- **零磁盘落盘**：图像字节从系统剪贴板读取后，全程驻留内存堆区；
- **即时销毁**：OCR 识别完成生成 `String` 文本后，包含图像像素的 Buffer 与 `SoftwareBitmap` 随 `spawn_blocking` 作用域闭合立即 Drop 释放；
- **前端零拷贝**：前端不持有图像 Base64、不持有 Blob、不渲染缩略图预览，彻底阻断 WebView 内存膨胀与潜在泄露。

### 3.6 前端交互与状态流 (`src/App.tsx`)

#### 3.6.1 UI 入口布局
- 在主输入框底部操作栏（“翻译”按钮左侧）添加专属按钮：`[从剪贴板识别图片]`，附带图片图标（Lucide `ImagePaste` 或 `Image`）。
- 国际化文案：
  - `zh-CN`: `"btnOcrPaste": "识别剪贴板图片"`, `"ocrRunning": "正在识图中..."`
  - `en`: `"btnOcrPaste": "Paste & OCR"`, `"ocrRunning": "Scanning image..."`

#### 3.6.2 执行状态流
```ts
const handleOcrClipboard = async () => {
  setOcrLoading(true)
  setOcrError(null)
  try {
    const res = await commands.ocrClipboardImage(targetLang)
    if (res.status === 'ok') {
      const text = res.data.trim()
      setSourceText(text)
      // debounceMs: 0 立即触发统一任务状态机，零等待
      runner.run(text, { source: 'clipboard', debounceMs: 0 })
    } else {
      setOcrError(t.ocrError[res.error.kind] || res.error.message)
    }
  } catch (e) {
    setOcrError(String(e))
  } finally {
    setOcrLoading(false)
  }
}
```
- **错误解耦**：OCR 过程中的报错（剪贴板无图、无语言包、图内无字）显示在输入框专用通知提示区，不将翻译状态机置为 `failed` 态，保证“识别失败”与“翻译失败”边界分明。

---

## 4. 验证与验收标准

### 4.1 验收标准对应表
| 阶段四验收项 | 设计落地与验证方案 |
| --- | --- |
| **识别错误与翻译错误可区分** | OCR 失败由专用 `OcrErrorDto` 表达并在前端独立呈现；翻译失败走 `TaskError`。两者互不污染。 |
| **无假成功** | 空图片或无文本时严格返回 `OcrErrorKind::EmptyText`，不触发翻译流程，不产出空译文。 |
| **取消语义可验证** | OCR 识别瞬间完成；后续翻译完全受现有代际闸门 `generation` 保护，点击取消立即作废。 |
| **图片生命周期可验证** | 代码库全局搜索无文件写入 API；内存 Buffer 随作用域结束释放，前端内存不留存像素数据。 |
| **跨平台构建不回退** | 非 Windows 平台（Ubuntu CI）编译存根模块，保证 `cargo test --workspace` 与 CI 绿灯。 |

---

## 5. 实施步骤规划（移交 writing-plans）

1. **Step 1: 契约与依赖**：在 `transly-core` 定义 `OcrErrorKind`/`OcrErrorDto`；在 `src-tauri` 引入 Windows 平台特化依赖。
2. **Step 2: 宿主 OCR 引擎实现**：实现 Windows 平台 OCR 与非 Windows 存根，包含 COM 生命周期、降采样与去重轮询。
3. **Step 3: IPC 命令与绑定同步**：实现 `ocr_clipboard_image` 并更新 `bindings.ts`。
4. **Step 4: 前端交互接入**：在 `App.tsx` 实现按钮、加载态、独立错误反馈与即时翻译流，补齐国际化词条。
5. **Step 5: 单元测试与端到端门禁**：补充错误分类单测，运行全量 CI 校验脚本。
