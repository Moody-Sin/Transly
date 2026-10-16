# Transly 截图翻译 —— 阶段 2 施工交接单

> **交接对象**：接手实施阶段 2（屏幕捕获 + 覆盖窗骨架）的智能体。
> **交接人**：上一轮负责调研、设计与阶段 0/1 实施的智能体。
> **交接时间**：2026-10-16
> **工作分支**：`feature/screenshot-translation`（已存在，HEAD = `c1e1dee`）
> **完整设计文档**：本单只覆盖**阶段 2**。全局设计见 `~/.claude/plans/sunny-swimming-quail.md`（若不可达，以本单为准，本单已把阶段 2 所需的全部契约内联）。

---

## 0. 必读：项目铁律（违反即返工）

本项目的 `AGENTS.md` 与全局 `CLAUDE.md` 是**最高优先级**，以下为其中对阶段 2 影响最大的条款，原文摘录：

### 0.1 分层与依赖方向（AGENTS.md §1）

- 翻译、缓存、OCR、术语库、语言检测核心逻辑**必须在 `crates/transly-core`**（纯 Rust，**禁止引入 Tauri 依赖**）。
- `src-tauri` 作为平台适配宿主层，**仅负责** Tauri 命令薄封装、系统原生凭据（`keyring`）、系统托盘与全局快捷键。
- `src` 前端只负责 UI 与交互，**严禁在前端直接发起任何翻译网络请求**，必须通过 `tauri-specta` 生成的强类型绑定调用后端。

**判定标准**：任何一行能被单元测试覆盖的纯逻辑，都不该写在 `src-tauri`。阶段 1 已把坐标、块聚合、TSV 解析、捕获判定全部下沉，阶段 2 若在 `src-tauri` 里写「坐标换算」「DPI 折算」「归一化除法」，即为违反。

### 0.2 安全第一（AGENTS.md §2）

- 所有外部商业翻译 API Key（DeepL / OpenAI / DeepSeek）**必须托管于系统原生凭据管理器**，**严禁明文落盘**。
- 本地 HTTP 服务**严格绑定 `127.0.0.1`**，强制 Bearer Token 强鉴权与扩展专用 CORS 白名单。
- 前端配置**严格内容安全策略 (CSP)**。

**阶段 2 相关**：冻结帧（屏幕截图）**必须全程内存**，**严禁落盘到任何临时目录**——这是 `README.md:134` 对外的「全流程内存零落盘」承诺，也是 Linux Wayland 降级的核心理由。

### 0.3 提交前门禁（AGENTS.md §3）

以下命令**必须全部通过**，缺一不可：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm run lint
pnpm run build
```

另有仓库 `lefthook.yml` 的 pre-commit 钩子会自动跑 `frontend-format` / `frontend-lint` / `frontend-test` / `rust-clippy` / `rust-fmt` / `rust-test` / `type-check-desktop` / `type-check-extension` 与 `conventional-commits` 提交信息校验。

### 0.4 严禁提交（AGENTS.md §3）

密钥、大型离线二进制模型文件、构建产物（`target/`、`dist/`、`node_modules/`）。
**`Cargo.lock` 与 `pnpm-lock.yaml` 必须提交**（本项目是可复现构建的应用，锁文件是构建契约的一部分），CI 以 `--frozen-lockfile` 安装以强制其同步。

### 0.5 全局 CLAUDE.md 的硬性要求

- 称呼用户为**老大**；**全程使用简体中文**。
- **约定式提交**（Conventional Commits）+ **语义化版本** + **维护更新日志**（Keep a Changelog）+ **标准 README.md**（为源码目录添加 `README.md`）。
- **自主修复**：看到 Bug 直接修，不寻求手把手指导，自行定位日志/错误/失败用例并闭环解决，CI 失败亦自主修复。
- **经验教训**：简短记录到项目根 `LESSONS.md`，**反复踩坑/有价值再记录**，避免小错误造成文档沉余。
- **注释/文档不能全信，时刻保持怀疑的态度。**
- **优先使用真实端到端测试验证。**
- **`.gitignore` 内容只添加不删减。**
- 产生的垃圾及时清理。
- **子智能体无权再派智能体**，确保智能体互不干扰；**一次派发不超过两个子智能体**；没完工不可提前向老大报告。
- 子智能体分工：**实施者**用 `deepseek` 模型；**评审/审查员**用 `Gemini 3.8 Flash`，且评审员必须仔细核查（`deepseek` 容易遗漏和出错）。

### 0.6 LESSONS.md 第 11 条（**强制动作，不是建议**）

> **新增回归测试后，必须把实现临时改回缺陷版并确认测试 FAILED，再恢复修复。** 这是区分「真测试」与「摆设」的唯一可靠手段。

阶段 2 提交前，对新增的每个关键测试都要执行此动作，并在提交信息里写明「反向验证已确认 FAILED」。

---

## 1. 当前进度与阶段 2 的确切范围

### 1.1 已完成（**不要重做，也不要改动**）

| 阶段 | 内容 | 状态 |
|---|---|---|
| **0** 依赖与基建（0.5d） | Cargo 依赖（**已移除 xcap**、`image` 上移、windows features、objc2 组、xcb）、`Info.plist`、CSP、`deb/rpm.recommends`、capabilities、CI 三处 apt 清单 | ✅ 提交 `d8925f7` |
| **1** core 纯逻辑（1.5d） | `ocr/geom.rs`、`ocr/blocks.rs`、`ocr/tsv.rs`、`ocr/screencap.rs`、`engine/batch_pipeline.rs`、`OcrErrorKind` 4 个新变体、i18n 中英双份 | ✅ 提交 `c1e1dee` |

阶段 1 交付的**全部内容已通过门禁并落地**（`cargo test --workspace` = 72 + 178 = 250 passed；`cargo llvm-cov -p transly-core` 行覆盖 91.13%，门禁 70%）。

### 1.2 阶段 2 的范围（**本单要做的全部**）

| 交付物 | 路径 | 说明 |
|---|---|---|
| 捕获调度 + 平台分支 | `src-tauri/src/screencap/mod.rs` | 三段 cfg，统一入口 `capture_monitor` |
| Windows 捕获 | `src-tauri/src/screencap/windows.rs` | GDI `BitBlt` |
| macOS 捕获 | `src-tauri/src/screencap/macos.rs` | `CGDisplayCreateImage` + 权限两段式 |
| Linux 捕获 | `src-tauri/src/screencap/linux.rs` | xcb `GetImage`（X11）；Wayland 直接降级 |
| 覆盖窗生命周期 + 自定义 URI 协议 + 会话状态 | `src-tauri/src/overlay/mod.rs` | 新建 |
| IPC 命令薄层 | `src-tauri/src/commands/screenshot.rs` | `commit` **暂返回空块**（OCR 在阶段 3） |
| 前端路由 | `src/main.tsx` | 加 `#shot` 分支 |
| 覆盖窗组件（框选，**不含 OCR**） | `src/ScreenshotOverlay.tsx` | 新建 |
| i18n 文案 | `src/i18n/zh-CN.json` + `en.json` | 新增 `screenshot` 块，中英双份 |
| 标签页/导航/托盘入口（可选） | — | **属阶段 5**，阶段 2 不要求 |

> **注意**：`src-tauri/src/screencap/README.md` 已被上一轮**删除**（那是一个只有 README 没有实现的死文档）。阶段 2 建代码时应**同时**建 `README.md`（全局 `CLAUDE.md` 要求「为源码目录添加 `README.md`」）。

### 1.3 阶段 2 的验收检查点（**逐平台手动**）

1. 覆盖窗**完整覆盖光标所在显示器**——含 **150% / 200% 缩放**与**双屏不同缩放**场景。
2. **冻结帧与屏幕像素严格对齐**（拖一个纯色窗口到角落作参照）。
3. Esc / 拖拽 / 轻点行为正确。
4. **连续触发 5 次无残留窗口、无内存增长。**
5. 未通过检查点 1、2，**不得进入阶段 3**。

---

## 2. 核心设计决策（**已拍板，不要重新论证**）

这一节的每条都是上一轮与老大讨论后确定的，改动即偏航。

### 2.1 归一化坐标 `[0,1]²` 是唯一坐标契约 ★

全链路（选区、OCR 块、译文渲染）一律用**归一化矩形**，原点左上。理由：截图的**物理像素**与覆盖窗的 **CSS 逻辑像素**在任何 DPI 下都不相等（Windows 200% 缩放、macOS Retina）。一旦像素坐标过 IPC，缩放系数就必须两端同步维护，DPI 差异必然漂移。归一化后前端只需 `left: x*100%`，**全链路零缩放系数**。

契约已实现在 `crates/transly-core/src/ocr/geom.rs`（`NormRect`），阶段 2 **直接调用，不要在前端或 `src-tauri` 重新实现换算**。

### 2.2 不透明「冻结帧」覆盖窗，而非透明窗

覆盖窗是**不透明**的全屏无边框窗口，直接以捕获到的冻结帧为背景图。用户观感与透明窗完全一致（都是「原图 + 译文覆盖」），但收益显著：

- 免掉透明窗对合成器的依赖（**X11 无 picom 时透明窗会渲染成黑**）；
- 截图内容与被翻译内容**严格同一份像素**，杜绝「重叠窗口已移动导致框错位」；
- **可在无头 Chromium 里做完整 E2E**，无需真实透明合成。

**不做平台原生真全屏**：macOS 会切 Space、Windows 独占全屏语义复杂，收益不抵风险。

### 2.3 图像字节走自定义 URI 协议，绝不走 IPC

4K 帧 = 3840×2160×4 ≈ **33 MB**。走 IPC 需 base64（+33%）再 `JSON.parse`，JS 堆峰值可达图像 3 倍。这与项目既有教训同源（CHANGELOG 0.3.1「流式文件导出后端命令」正是为绕开大 payload 过 IPC）。

```
register_uri_scheme_protocol("translyshot", ...)
  → /{session_id}/base.png 直接从会话读 PNG 字节
  → session_id 不匹配当前会话 → 404（防陈旧窗口读到新会话图）
```

URL 形式跨平台不同（Win/Android 为 `http://translyshot.localhost/...`，其余为 `translyshot://localhost/...`）——**由 Rust 在 DTO 里直接给出完整 `image_url`**，前端不做平台判断。

CSP（`src-tauri/tauri.conf.json`）**已含**：
```
"img-src 'self' data: blob: translyshot: http://translyshot.localhost;"
```
`data:` / `blob:` 是留给 E2E 与 Plan B 的。

**Plan B**（若某平台自定义协议受阻）：改用 `tauri::ipc::Response::new(png_bytes)` + `URL.createObjectURL`，CSP 已备 `blob:`。约 20 行改动。

### 2.4 只覆盖光标所在显示器

跨显示器框选需拼接多张不同 DPR 的图再映射归一化坐标，收益极低、错误率高 → **明确砍掉**。

### 2.5 不得使用 tao 的 `monitor_from_point` ★

见 §3.1，这是**已实测确认的坑**。

---

## 3. 已实测核实的硬事实（**不要凭记忆推翻，这些不是推测**）

上一轮对这些 API 逐一到上游源码/生成产物里核对过。**注意**：其中有 **2 条与旧计划书不符**，已在此更正。

### 3.1 tao 坐标单位自相矛盾（Xvfb 实测 + 源码确认）

| 函数 | 实现（`tao-0.37.1/src/platform_impl/linux/`） | 单位 |
|---|---|---|
| `Monitor::position()` / `size()` | `Logical*.to_physical(scale)`（`monitor.rs:29/40`） | **物理** |
| `Window::cursor_position()` | `LogicalPosition::to_physical(scale)`（`util.rs:18`） | **物理** |
| `Window::monitor_from_point(x, y)` | **原样喂给 GDK** `display.monitor_at_point()`（`monitor.rs:88`） | **逻辑**（入参） |

**实测证据**：`GDK_BACKEND=x11 GDK_SCALE=2` 下，`Monitor::size()` 报 `2560x1440`、`cursor_position()` 报 `(1800,900)`（均为物理），而逻辑框只有 `1280x720` —— 两者不可能同时成立。

**结论：不得用 `monitor_from_point(cursor)` 定位覆盖窗。** 改为「取 `available_monitors()` 全部 + 光标物理坐标，在 core 里自己做矩形包含判定」——**已实现为 `find_monitor_for_point`**，纯几何、可单测、可反向验证。

另：**macOS `Monitor::size()` 疑似双重缩放**（`macos/monitor.rs:216`）：`CGDisplayPixelsWide()` **已是物理像素**，却又套 `PhysicalSize::from_logical()`（**再乘一次 scale**）→ Retina 下会放大 2×。而 `position()` 用 `CGDisplayBounds`（逻辑点）乘 scale 是**正确**的。**无法在本机实跑 macOS**，故必须靠「捕获后尺寸自校验」兜住（见 §5.3）。

### 3.2 `CGDisplayCreateImage` 的可用性（**已实证，勿凭 `#[cfg]` 字面推翻**）

**结论：当前 `Cargo.toml` 的声明已够用，无需追加 feature。** 但有两个真实陷阱。

**① feature 门控看起来吓人，实际已满足。** `objc2-core-graphics 0.3.2` 的 `src/generated/mod.rs:842-843`：

```rust
#[cfg(all(feature = "CGDirectDisplay", feature = "CGImage"))]
pub use self::__CGDirectDisplay::CGDisplayCreateImage;
```

字面上需要**两个** feature，而 `src-tauri/Cargo.toml:87` 只写了 `features = ["CGImage"]`。**但 `CGImage` 的 default 列表本身含 `CGDirectDisplay`**，而我们**未** 设 `default-features = false`，故两者都已启用。

**实证命令**（不是推理）：

```bash
cargo tree --target aarch64-apple-darwin -e features -i objc2-core-graphics | grep CGDirectDisplay
# 输出含：objc2-core-graphics feature "CGDirectDisplay"
```

> 这一条是上一轮交接单里**写错过**的：曾据 `#[cfg]` 字面断言「必须补 `CGDirectDisplay`，否则 macOS 编译失败」。`cargo tree` 显示该 feature 已启用。**结论：不要改这行 Cargo.toml。** 若将来有人加了 `default-features = false`，**那时**才需要显式补上。

**② 真正的陷阱：`#[deprecated]` 会让 `-D warnings` 直接失败。** 该函数定义带 `#[deprecated = "Please use ScreenCaptureKit instead."]`，**使用点必须加 `#[allow(deprecated)]`** 并附注释说明理由（Apple 建议 ScreenCaptureKit，但它是当前可用的稳定 API，且比 `CGWindowListCreateImage` 更贴合「按显示器捕获」；捕获封装在单一函数内，将来替换不影响上层）。

**确切签名**（`src/generated/CGDirectDisplay.rs:563`）：

```rust
#[cfg(feature = "CGImage")]
#[deprecated = "Please use ScreenCaptureKit instead."]
pub extern "C-unwind" fn CGDisplayCreateImage(
    display_id: CGDirectDisplayID,
) -> Option<CFRetained<CGImage>>;       // 无权限时返回 None（不是空白图）
```

其余已核实可用（`objc2-core-graphics 0.3.2`）：
- `CGPreflightScreenCaptureAccess() -> bool`、`CGRequestScreenCaptureAccess() -> bool`（`src/generated/CGWindow.rs:285/293`，**不受 feature 门控**）
- `CGDisplayBounds(display: CGDirectDisplayID) -> CGRect`（`CGDirectDisplay.rs:137`）
- `CGDisplayPixelsWide/High(display) -> usize`（`:145/:153`）
- `CGMainDisplayID() -> CGDirectDisplayID`（`:41`）

**取像素的唯一路径**（`CGBitmapContextCreate` 的 plain 版在该 crate 中**不存在**，只有 `CGBitmapContextCreateAdaptive`，不要指望它）：

```rust
// 均为关联函数（extern_class! 生成），参数是 Option<&T>
CGImage::width(image: Option<&CGImage>) -> usize;
CGImage::height(image: Option<&CGImage>) -> usize;
CGImage::bytes_per_row(image: Option<&CGImage>) -> usize;
CGImage::bitmap_info(image: Option<&CGImage>) -> CGBitmapInfo;
CGImage::data_provider(image: Option<&CGImage>) -> Option<CFRetained<CGDataProvider>>;

CGDataProvider::data(provider: Option<&CGDataProvider>) -> Option<CFRetained<CFData>>;
// ↑ 即 CGDataProviderCopyData，返回的是**拷贝**，可安全持有

CFData::length(&self) -> CFIndex;                                   // objc2-core-foundation 0.3.2
CFData::byte_ptr(&self) -> *const u8;
CFData::bytes(&self, range: CFRange, buffer: *mut u8);              // unsafe
```

**生命周期要点**：
- `CGDisplayCreateImage` 返回 `CFRetained<CGImage>`，**RAII 自动释放**；`CGDataProvider::data()` 返回 `CFRetained<CFData>`，同样 RAII。
- **`byte_ptr()` 的裸指针只在 `CFData` 存活期间有效** → **字节拷贝必须在 `CFData` 的作用域内完成**，不能把裸指针带出作用域。
- `bitmap_info()` 决定字节序（`CGBitmapInfo`），macOS 抓屏通常是 **BGRA / premultipliedFirst**，需按它判断是否需要通道重排——**不要假定**。

**关于 `objc2-core-foundation` 要不要显式声明**（已实证）：
- 它**已在依赖树中**，且 `CFData` feature **已启用**（`cargo tree ... -i objc2-core-foundation | grep CFData` 有输出）。
- 它是 `objc2-core-graphics` 的**传递依赖**。若你的代码只做 `let d = CGDataProvider::data(p)?; d.byte_ptr()` 这类**方法调用**（靠类型推断 + `Deref`），**不需要**动 `Cargo.toml`。
- 若要**显式命名类型**（如写函数签名 `-> CFRetained<CFData>`），按项目纪律（`AGENTS.md`「它是我们代码的直接依赖，不该靠别人的传递依赖」，与 CI 里显式声明 `libxcb1-dev` 同理）应**显式声明** `objc2-core-foundation = { version = "0.3.2", features = ["CFData"] }`。

### 3.3 ⚠️ 更正一：`xcb` **有** build.rs，旧计划书「无 build.rs」的说法是错的

`xcb 1.7.1` 的 `Cargo.toml` 明确写着 `build = "build/main.rs"`，它用 `quick-xml` 从 crate 内置的 `xml/*.xml`（33 个文件）**生成 Rust 绑定**。`build-dependencies` 含 `quick-xml`。

**这条更正的实际影响**：
- 旧计划书用「无 build.rs」论证「不依赖系统头文件」，**论证过程错了**，但**结论仍然成立**——它读的是 crate 自带的 XML，不需要系统上装 `libxcb` 的头文件；链接是对 `libxcb.so.1` / `libxcb.so` 的动态链接（`src/ffi/base.rs:124` 的 `link: "xcb"`），运行时需要 `libxcb.so.1`（GUI 环境必有，`libx11-dev` 也传递带入 `libxcb1-dev`）。
- 首轮编译会比无 build.rs 的 crate 慢（要解析 XML 生成代码），**不要误以为是卡死**。

**已核实仍成立**：
- **randr 走 wire protocol，零 `extern "C"`**：`target/debug/build/xcb-*/out/randr.rs` 中 `extern "C"` 出现 **0** 次、`xcb_get_conn_funcs` **47** 处（对比 `xproto.rs`：`extern "C"` 0 次、`xcb_get_conn_funcs` 120 处）。**→ 确实不需要 `libxcb-randr.so` / `libxcb-randr0-dev`。**

### 3.4 ⚠️ 更正二：`GetImage` 在 `x::` 命名空间，且 `GetImageReply` 需用 `Reply` trait

旧计划书里 `xcb::GetImage { ... }` 的写法**不完整**。官方 `examples/screenshot.rs`（crate 自带，可直接参考）的正确写法：

```rust
use xcb::x;

let (conn, screen_num) = xcb::Connection::connect(None).unwrap();
let setup = conn.get_setup();
let screen = setup.roots().nth(screen_num as usize).unwrap();

let cookie = conn.send_request(&x::GetImage {
    format: x::ImageFormat::ZPixmap,
    drawable: x::Drawable::Window(screen.root()),
    x: 0,
    y: 0,
    width,
    height,
    plane_mask: u32::MAX,
});
let reply = conn.wait_for_reply(cookie).unwrap();
let src = reply.data();
```

**已核实的确切字段**（生成的 `out/xproto.rs`）：

```rust
pub struct GetImage {
    pub format: ImageFormat,
    pub drawable: Drawable,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub plane_mask: u32,
}
```

- `GetImageReply::data(&self) -> &[u8]`（偏移 32，长度 `length()*4`）
- `GetImageReply::depth(&self) -> u8`
- `GetImageReply::visual(&self) -> Visualid`
- ⚠️ **`GetImageReply::length()` 是私有方法**（`fn length(&self) -> u32`，无 `pub`）——不要直接调，用 `data()`。

**像素格式**：官方示例注释明确「Captured image stores order pixels as **BGR**」，且按 `src.chunks(4)` 迭代——**即默认 depth 为 24/32 时是每像素 4 字节的 BGRX**。**但不同 X server 的 depth 可能是 8/16/24/32**，必须按 `reply.depth()` 与 `setup.pixmap_formats()` 里的 `bits_per_pixel` 分支解包。**至少要处理 24 与 32（4 字节 BGRX）；8/16 位色深在现代桌面上极罕见，若处理请写清注释与降级文案**。

**显示器枚举**（randr，已核实）：

```rust
pub struct GetMonitors { pub window: xproto::Window, pub get_active: bool }
// MonitorInfo 的访问器：
pub fn x(&self) -> i16;          // 偏移 8
pub fn y(&self) -> i16;          // 偏移 10
pub fn width(&self) -> u16;      // 偏移 12
pub fn height(&self) -> u16;     // 偏移 14
pub fn name(&self) -> xproto::Atom;
pub fn primary(&self) -> bool;
pub fn outputs(&self) -> &[Output];
// GetMonitorsReply:
pub fn monitors(&self) -> MonitorInfoIterator<'_>;   // Iterator<Item = &MonitorInfo>，偏移 32
```

`Screen` 访问器：`root() -> Window`、`width_in_pixels() -> u16`、`height_in_pixels() -> u16`、`root_visual() -> Visualid`、`root_depth() -> u8`、`allowed_depths() -> DepthIterator`。
`Format`：`depth() -> u8`、`bits_per_pixel() -> u8`、`scanline_pad() -> u8`。

### 3.5 Windows GDI（`windows 0.62.2`，已核实）

```rust
// Win32::Graphics::Gdi
pub unsafe fn BitBlt(hdc: HDC, x: i32, y: i32, cx: i32, cy: i32,
                     hdcsrc: Option<HDC>, x1: i32, y1: i32, rop: ROP_CODE)
                     -> windows_core::Result<()>;     // 注意：吞掉了 BOOL，返回 Result
pub unsafe fn CreateCompatibleDC(hdc: Option<HDC>) -> HDC;
pub unsafe fn CreateCompatibleBitmap(hdc: HDC, cx: i32, cy: i32) -> HBITMAP;
pub unsafe fn GetWindowDC(hwnd: Option<Foundation::HWND>) -> HDC;
pub unsafe fn DeleteDC(hdc: HDC) -> windows_core::BOOL;
pub unsafe fn DeleteObject(ho: HGDIOBJ) -> windows_core::BOOL;
pub unsafe fn GetDIBits(hdc: HDC, hbm: HBITMAP, start: u32, clines: u32,
                        lpvbits: Option<*mut c_void>, lpbmi: *mut BITMAPINFO,
                        usage: DIB_USAGE) -> i32;
```

**要点**：
- `BitBlt` 的 `hdcsrc` 是 `Option<HDC>`，传 `None` 会被 `unwrap_or(core::mem::zeroed())`（即 NULL）→ 拿不到屏幕。必须传 `Some(mem_dc)`。
- `CreateCompatibleDC` 必须传 `Some(screen_dc)`，否则创建的是 1×1 单色 DC。
- **受保护窗口（DRM / `WDA_EXCLUDEFROMCAPTURE`）下 `BitBlt` 会露出壁纸或纯色**——这是 OS 层封锁，**任何用户态后端都无法捕获**（GDI 露壁纸、WGC 零帧），**换后端无效**。不要试图通过「检测纯色再重试别的后端」来绕过。
- **GDI 资源必须 RAII 释放**：`HBITMAP`/`HDC` 泄漏是进程级资源泄漏，连续触发 5 次的验收项会直接暴露。
- `windows` crate 当前 `src-tauri/Cargo.toml` 已声明 `Win32_Graphics_Gdi` 与 `Win32_UI_WindowsAndMessaging`，**无需追加**。

### 3.6 `image` crate（`0.25.10`，已核实）

```rust
pub type RgbaImage = ImageBuffer<Rgba<u8>, Vec<u8>>;   // src/images/buffer.rs:1798

// 两条可用的 PNG 编码入口：
pub fn DynamicImage::write_to<W: Write + Seek>(&self, mut w: W, format: ImageFormat) -> ImageResult<()>;
pub fn ImageBuffer::write_to<W>(&self, writer: &mut W, format: ImageFormat) -> ImageResult<()>
    where W: std::io::Write + std::io::Seek, P: PixelWithColorType;   // src/images/buffer.rs:1101

pub enum ImageFormat { Png, Jpeg, Gif, WebP, ... }     // src/io/format.rs:11
```

**注意**：`write_to` 要求 `Write + **Seek**`。写到 `Vec<u8>` 时 `std::io::Cursor<Vec<u8>>` 满足；**直接传 `Vec<u8>` 不满足**（`Vec<u8>` 无 `Seek`）。文档建议「In most cases, you should wrap your writer in a `BufWriter` for best performance」。

`Cargo.toml` 已声明 `image = { version = "0.25", default-features = false, features = ["png"] }`，**无需追加**。

### 3.7 Tauri 2.12.1 API（已核实）

**自定义协议**（`src/app.rs:2267` / `:2337`）：

```rust
pub fn register_uri_scheme_protocol<N, T, H>(self, uri_scheme: N, protocol_handler: H) -> Self
where
    N: Into<String>,
    T: Into<Cow<'static, [u8]>>,
    H: Fn(UriSchemeContext<'_, R>, http::Request<Vec<u8>>) -> http::Response<T>
        + Send + Sync + 'static;

// 异步版（可在线程池里处理后再回）
pub fn register_asynchronous_uri_scheme_protocol<N, H>(self, uri_scheme: N, protocol_handler: H) -> Self
where
    H: Fn(UriSchemeContext<'_, R>, http::Request<Vec<u8>>, UriSchemeResponder) + Send + Sync + 'static;
```

```rust
// src/app.rs:2636
pub struct UriSchemeContext<'a, R: Runtime> { /* 私有字段 */ }
impl<'a, R: Runtime> UriSchemeContext<'a, R> {
    pub fn app_handle(&self) -> &'a AppHandle<R>;
    pub fn webview_label(&self) -> &'a str;
}
```

> **`UriSchemeContext` 未实现 `Clone`**，字段私有。闭包必须是 `Fn + Send + Sync + 'static` → **不能捕获 `&AppHandle`**，要用 `Arc<ScreenshotSession>` 捕获（或用 `ctx.app_handle()` 动态取）。

**`WebviewWindowBuilder`**（`src/webview/webview_window.rs`，行号已核实）：

```rust
pub fn new<L: Into<String>>(manager: &'a M, label: L, url: WebviewUrl) -> Self;   // :101
pub fn decorations(self, bool) -> Self;      // :601
pub fn resizable(self, bool) -> Self;        // :901
pub fn shadow(self, bool) -> Self;           // :673
pub fn always_on_top(self, bool) -> Self;    // :615
pub fn skip_taskbar(self, bool) -> Self;     // :641
pub fn focused(self, bool) -> Self;          // :586
pub fn visible(self, bool) -> Self;          // :934
pub fn position(self, x: f64, y: f64) -> Self;      // :862  ⚠️ 逻辑像素
pub fn inner_size(self, width: f64, height: f64) -> Self;  // :869  ⚠️ 逻辑像素
pub fn build(self) -> crate::Result<WebviewWindow<R>>;     // :489
```

**`WebviewWindow` 上的操作**（`manager` 侧，`src/window/mod.rs`）：

```rust
pub fn set_position<Pos: Into<Position>>(&self, position: Pos) -> crate::Result<()>;  // :1963 接受物理量
pub fn set_size<S: Into<Size>>(&self, size: S) -> crate::Result<()>;                  // :1924 接受物理量
pub fn set_focus(&self) -> crate::Result<()>;                    // :1972
pub fn show(&self) -> crate::Result<()>;                         // :1881
pub fn hide(&self) -> crate::Result<()>;                         // :1886
pub fn close(&self) -> crate::Result<()>;                        // :1891
pub fn destroy(&self) -> crate::Result<()>;                      // :1896
pub fn set_always_on_top(&self, bool) -> crate::Result<()>;      // :2191
pub fn set_skip_taskbar(&self, bool) -> crate::Result<()>;       // :2296
pub fn cursor_position(&self) -> crate::Result<PhysicalPosition<f64>>;   // :2025
pub fn available_monitors(&self) -> crate::Result<Vec<Monitor>>;         // :1948
pub fn scale_factor(&self) -> crate::Result<f64>;                        // :1819
```

**`Monitor`**（`src/window/mod.rs:58`）：

```rust
pub struct Monitor { /* 私有字段 */ }
pub fn name(&self) -> Option<&String>;
pub fn size(&self) -> &PhysicalSize<u32>;              // 返回引用
pub fn position(&self) -> &PhysicalPosition<i32>;      // 返回引用
pub fn work_area(&self) -> &PhysicalRect<i32, u32>;
pub fn scale_factor(&self) -> f64;
```

**`WindowEvent`**（`src/app.rs:111`，`#[non_exhaustive]`）关键变体：

```rust
Resized(PhysicalSize<u32>),
Moved(PhysicalPosition<i32>),
CloseRequested { api: CloseRequestApi },   // #[non_exhaustive]
Destroyed,
Focused(bool),
ScaleFactorChanged { scale_factor: f64, new_inner_size: PhysicalSize<u32> },  // #[non_exhaustive]
```

> **因为是 `#[non_exhaustive]`，match 必须带 `_ => {}` 兜底**，否则 `-D warnings` 下会因非穷尽匹配报错。

**关键陷阱**：`WebviewWindowBuilder::position()/inner_size()` **只吃逻辑像素**，而 `Monitor::position()/size()` 给**物理像素**。本方案**用 builder 不带几何，建窗后用 `set_position`/`set_size`（接受物理量）**，从根上绕开该换算。

### 3.8 本机环境事实

- **本机未安装 tesseract**（`which tesseract` 无输出，`/usr/share/tesseract-ocr/*/tessdata/` 不存在）→ 阶段 3 的首要动作是实装并跑通 `tesseract - - tsv` 管道。**阶段 2 不涉及 OCR，无需安装。**
- 本机是 **Ubuntu 22.04 / WSL2**（`Linux 6.18.54.1-microsoft-standard-WSL2`）。**WSL2 下可能无真实 X server** → 阶段 2 的 Linux 手动验收（特别是「冻结帧与屏幕像素严格对齐」）**可能需要老大在真机上做**，或至少在有 X11 的环境下做。**若本机无 X server，`xcb::Connection::connect(None)` 会失败**，此时应走 `map_capture_error` 映射成可读错误（已被阶段 1 覆盖）——**这不是缺陷，而是预期行为**，验收时不要误判。

---

## 4. 阶段 1 已交付、阶段 2 必须复用的接口

**这些已实现并测试通过，直接调用，不要重写。**

### 4.1 `crates/transly-core/src/ocr/geom.rs`

```rust
pub struct NormRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

impl NormRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self;
    pub const fn zero() -> Self;
    /// ⚠️ img_w/img_h 必须是**喂给 OCR 那张图**的尺寸，而非原图尺寸
    pub fn from_pixels_in(px: u32, py: u32, pw: u32, ph: u32, img_w: u32, img_h: u32) -> Self;
    /// Vision 原点左下 → 本契约原点左上
    pub fn from_vision_norm(x: f32, y: f32, w: f32, h: f32) -> Self;
    pub fn to_vision_norm(&self) -> (f32, f32, f32, f32);
    /// 映射回指定尺寸图上的像素矩形：钳制到图内，宽高至少 1px
    pub fn to_pixels_in(&self, img_w: u32, img_h: u32) -> (u32, u32, u32, u32);
    /// 拖拽越界的**唯一收口点**
    pub fn clamped_unit(&self) -> Self;
    /// 轻点未拖拽判定（宽或高 < min_px）
    pub fn is_degenerate(&self, img_w: u32, img_h: u32, min_px: u32) -> bool;
    /// 两角点构造（支持四向拖拽，内部取 min）
    pub fn from_corners(ax: f32, ay: f32, bx: f32, by: f32) -> Self;
    pub fn translated(&self, dx: f32, dy: f32) -> Self;
}

/// 显示器的物理几何（Tauri Monitor 的扁平化快照）
pub struct MonitorRect { pub x: i32, pub y: i32, pub width: u32, pub height: u32, pub scale_factor: f64 }

/// 在显示器列表中定位包含给定**物理**点的显示器。
/// 找不到返回 None（刻意**不**「取最近显示器」——回退策略属调用方决策）
pub fn find_monitor_for_point(cursor: (i32, i32), monitors: &[MonitorRect]) -> Option<usize>;
```

### 4.2 `crates/transly-core/src/ocr/screencap.rs`

```rust
/// 以 XDG_SESSION_TYPE 为权威，仅在它缺失时回落 WAYLAND_DISPLAY
pub fn detect_wayland_session(xdg_session_type: Option<&str>, wayland_display: Option<&str>) -> bool;

/// 原始错误 → 有意义的错误种类。**绝不返回 Internal**
pub fn map_capture_error(raw: &str) -> OcrErrorKind;

/// 一次截图翻译会话（**不含图像字节**）
pub struct ScreenshotSessionDto {
    pub session_id: String,
    pub image_url: String,
    pub image_width: u32,
    pub image_height: u32,
    pub generation: u32,
    pub target_lang: String,
    pub engine: String,
}
```

### 4.3 `crates/transly-core/src/ocr/blocks.rs`（阶段 3 才用，此处备查）

```rust
pub struct PixelBox { pub x: u32, pub y: u32, pub w: u32, pub h: u32, pub text: String, pub confidence: Option<f32> }
pub fn merge_words_into_lines(words: &[PixelBox], y_tolerance: u32) -> Vec<PixelBox>;
pub fn merge_lines_into_paragraphs(lines: &[PixelBox], max_gap_px: u32, min_x_overlap: f32) -> Vec<PixelBox>;
pub fn filter_blocks(blocks: Vec<OcrBlockDto>) -> Vec<OcrBlockDto>;
pub fn plan_block_batches(blocks: &[OcrBlockDto]) -> Vec<(usize, usize)>;
pub const MIN_BLOCK_CONFIDENCE: f32 = 0.5;

pub struct OcrBlockDto {
    pub index: u32, pub text: String,
    pub x: f32, pub y: f32, pub w: f32, pub h: f32,
    pub line_count: u32, pub confidence: Option<f32>,
}
```

### 4.4 `crates/transly-core/src/ocr/mod.rs` 的错误种类

```rust
pub enum OcrErrorKind {
    NoImage, NoLanguagePack, EmptyText, UnsupportedPlatform,
    ToolMissing, CaptureFailed, PermissionDenied, TooLarge, Internal,
}

impl OcrErrorDto {
    pub fn new(kind: OcrErrorKind, message: impl Into<String>) -> Self;
    pub fn no_image() -> Self;
    pub fn empty_text() -> Self;
    pub fn no_language_pack() -> Self;
    pub fn unsupported_platform() -> Self;      // 文案按 cfg(target_os="linux") 分流
    pub fn wayland_unsupported() -> Self;       // 带 X11 指引
    pub fn tool_missing(tool: &str, hint: impl Into<String>) -> Self;
    pub fn capture_failed(msg: impl Into<String>) -> Self;
    pub fn permission_denied() -> Self;         // 文案含「授权后需重启应用」
    pub fn too_large() -> Self;
    pub fn internal(msg: impl Into<String>) -> Self;
}
```

**阶段 2 的捕获失败必须走 `map_capture_error` → `OcrErrorDto::new(...)`**，不要自己拼 `internal`。

### 4.5 `crates/transly-core/src/engine/batch_pipeline.rs`（阶段 3/4 才用）

```rust
pub struct BatchPipelineDeps<'a> {
    pub engine: &'a dyn TranslateEngine,
    pub cache: &'a TranslationCache,
    pub glossary: &'a Glossary,
    pub cache_enabled: bool,
}
pub struct BatchPipelineOutcome {
    pub results: Vec<Result<TranslationResponse>>,   // 与请求等长、同序
    pub cache_hits: usize,
}
pub async fn translate_batch_with_cache(
    deps: BatchPipelineDeps<'_>, requests: &[TranslationRequest],
) -> BatchPipelineOutcome;
```

**顺序不可调换**：源端术语库 → 查缓存 → 引擎（逐项容错）→ 写缓存 → 译文端术语库。

### 4.6 宿主层既有设施（`src-tauri`）

```rust
// commands/mod.rs
pub struct TranslationDependencies {
    pub cache: Arc<TranslationCache>,
    pub pool: Arc<EnginePool>,
    pub glossary: Arc<RwLock<Glossary>>,
    pub cache_controller: Arc<crate::cache_controller::CacheController>,
    pub stream_registry: Arc<StreamTaskRegistry>,
}

// engine_pool.rs
impl EnginePool {
    pub async fn get_engine(&self, engine_id: &str) -> Arc<dyn TranslateEngine>;
    pub fn default_engine(&self) -> Arc<dyn TranslateEngine>;
    pub async fn invalidate(&self, engine_id: &str);
}

// cache_controller.rs
impl CacheController {
    pub fn snapshot(&self) -> (bool, u64);   // (cache_enabled, generation)
    pub fn is_enabled(&self) -> bool;
}
```

**`src-tauri/src/main.rs`** 的既有范式（阶段 2 必须沿用）：
- state 通过 `.manage(...)` 注入；**Tauri 按 TypeId 存 state，同类型重复 `.manage()` 会 panic**（`main.rs:113` 有明确注释）。
- **多字段 state 必须用 newtype 包裹**（如 `ScreenshotSession(Mutex<SessionState>)`），否则与其他同类型 state 冲突。
- 第二个快捷键注册范式见 `main.rs:259-314`（`on_shortcut` + `ShortcutStatusSlot` 四段式）。
- `on_window_event` 全局钩子在 `main.rs:409`，**覆盖窗的 `Focused`/`CloseRequested` 处理要挂在这里**，或挂在窗口自身的 `on_window_event`（`WebviewWindow::on_window_event`，`webview_window.rs:1643`）。

---

## 5. 阶段 2 详细设计（可直接施工）

### 5.1 `src-tauri/src/screencap/mod.rs` —— 三段式 cfg

**必须三段式** `windows / macos / not(any(...))`。沿用既有的 `not(windows)` 写法会把 tesseract/Linux 代码编进 macOS。

```rust
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::capture_monitor;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::capture_monitor;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod linux;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub use linux::capture_monitor;
```

**统一入口签名**（三平台同签名）：

```rust
/// 捕获指定显示器的完整画面。
/// 返回 (RgbaImage, 实际尺寸)；尺寸自校验由调用方（overlay）按 §5.3 执行。
pub fn capture_monitor(monitor: &transly_core::MonitorRect)
    -> Result<image::RgbaImage, transly_core::OcrErrorDto>;
```

**平台专属 `use` 必须同样门控**，否则非目标平台在 `-D warnings` 下 unused import 编译失败（历史踩坑 commit `d3497df`）。

### 5.2 各平台实现要点

#### Linux（`screencap/linux.rs`）—— 本次唯一能在本机编译验证的分支

```rust
// 顺序很重要：先判 Wayland（无 X11 路径可言），再连 X server
if transly_core::detect_wayland_session(
    std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
) {
    return Err(transly_core::OcrErrorDto::wayland_unsupported());
}
```

- **X11 主路径纯内存**：`x::GetImage`（`ZPixmap`）直接拿像素缓冲区，**无任何 `fs`/`temp_dir` 调用**，满足零落盘。
- **显示器枚举用 randr `GetMonitors`**（见 §3.4），与 tao 的 `Monitor` 按几何匹配（不依赖 tao 的单位假设）。
- **像素解包**：按 `reply.depth()` 分支处理 24/32（4 字节 BGRX → 重排为 RGBA）。**这一步必须有单测**：把 BGRX 字节喂进去断言输出 RGBA 正确——这是唯一能捕获「R/B 通道互换」的测试，肉眼在灰度 UI 上不易察觉。
- **连接失败**（WSL2 无 X server、`DISPLAY` 未设）→ 用 `map_capture_error` 映射，**不要直接 `?` 抛裸错误**。
- 捕获区域的 `x`/`y` 是 **i16**、`width`/`height` 是 **u16**。4K 屏宽 3840 在 u16 范围内（上限 65535），**但多屏横向拼接时虚拟桌面坐标可能超 i16 范围（±32767）**——超限时应明确报错而非静默截断（`as i16` 会环绕成负数，导致捕获到错误区域）。

#### Windows（`screencap/windows.rs`）

- GDI 路径：`GetWindowDC(None)`（NULL = 整个屏幕）→ `CreateCompatibleDC(Some(screen_dc))` → `CreateCompatibleBitmap(screen_dc, cx, cy)` → `SelectObject` → `BitBlt(mem_dc, 0, 0, cx, cy, Some(screen_dc), vx, vy, SRCCOPY)` → `GetDIBits` 取 BGRA 字节 → 重排为 RGBA。
- **`x`/`y` 用虚拟桌面坐标**（`SM_XVIRTUALSCREEN`/`SM_YVIRTUALSCREEN` 相对），副屏可为负 → `GetSystemMetrics` 取；`BitBlt` 的 `x1`/`y1` 是**源 DC 坐标**，对屏幕 DC 即虚拟桌面绝对坐标。
- **必须 RAII 释放** `HBITMAP` / `HDC`（`DeleteObject` / `DeleteDC`），**含所有错误路径**。用 `Drop` guard。
- `BitBlt` 返回 `Result<()>`（已吞掉 BOOL）→ 失败时用 `map_capture_error` 映射。

#### macOS（`screencap/macos.rs`）

**权限两段式**：

```rust
// ① 捕获前预检（不受 feature 门控，objc2-core-graphics 0.3.2 直接可用）
if !CGPreflightScreenCaptureAccess() {
    // 触发系统弹窗（返回值为「本次是否已授权」，通常仍为 false）
    let _ = CGRequestScreenCaptureAccess();
    return Err(OcrErrorDto::permission_denied());
}
// ② 捕获后错误映射兜底：CGDisplayCreateImage 无权限时返回 None
```

- **无权限时 `CGDisplayCreateImage` 显式返回 `None`**（**不是**静默返回空白图——旧计划书的假设已被推翻）。故**不需要**纯色检测启发式，走错误映射即可。
- `CGDisplayCreateImage` **带 `#[deprecated]`** → 需 `#[allow(deprecated)]` 并附注释说明「Apple 建议 ScreenCaptureKit，但它是当前可用的稳定 API，且比 `CGWindowListCreateImage` 更贴合按显示器捕获；捕获封装在单一函数内，将来替换不影响上层」。
- **非主线程必须有 `autoreleasepool`**（`objc2::rc::autoreleasepool`），否则 Objective-C 对象泄漏。`spawn_blocking` 的线程是后台线程，**必须包**。
- **坐标是唯一的坑**：Vision `boundingBox()` 归一化且**原点左下** → 走 `NormRect::from_vision_norm`（**阶段 3 的事**）。捕获本身的 `CGImage` 是像素坐标，不涉及此翻转。
- `ObjC` 对象的释放：`CGDisplayCreateImage` 返回 `CFRetained<CGImage>`，**RAII 自动释放**；`CGDataProvider::data()` 返回 `CFRetained<CFData>`，同样 RAII。**但 `byte_ptr()` 拿的裸指针只在 `CFData` 存活期间有效**，拷贝必须在其作用域内完成。

### 5.3 尺寸自校验（**三平台通用，必做**）

```rust
// 捕获后断言图像尺寸 ≈ 显示器物理尺寸（允许 ±1 取整误差）
if (img.width() as i64 - monitor.width as i64).abs() > 1
    || (img.height() as i64 - monitor.height as i64).abs() > 1 {
    return Err(OcrErrorDto::capture_failed(
        "捕获尺寸与显示器不匹配，可能为系统缩放或显卡驱动限制"
    ));
}
```

**为什么必须做**：这是**唯一**能同时兜住「上游单位改了」（含 §3.1 的 macOS `Monitor::size()` 疑似双重缩放）与「三平台 DPI 差异」的写法。它只依赖「同一显示器物理尺寸应等于捕获图像尺寸」这一不变量，**不引入任何平台分支假设**。

### 5.4 `src-tauri/src/overlay/mod.rs` —— 覆盖窗生命周期

#### 会话状态

```rust
/// 必须 newtype：Tauri 按 TypeId 存 state，同类型重复 .manage() 会 panic。
#[derive(Default)]
pub struct ScreenshotSession(Mutex<SessionState>);

struct SessionState {
    generation: u32,        // ⚠️ u32 而非 u64（specta 转 BigInt 会让前端 JSON.stringify 抛错）
    phase: Phase,           // Idle | Selecting | Recognizing | Translating | Result
    focused_once: bool,     // 建窗瞬间假失焦的守卫
    png: Vec<u8>, img_w: u32, img_h: u32,
    monitor: MonitorRect, window_label: String,
    target_lang: String, engine: String,
    blocks: Vec<OcrBlockDto>,   // 供换语言/换引擎重译，无需重 OCR（阶段 4 用）
}
```

**内存纪律**：4K 帧 33MB，**不释放就是实打实的泄漏**。`RgbaImage`/PNG 必须在 commit/cancel/窗口关闭时显式释放（`clear()` + `shrink_to_fit()` 或整体替换为 `Vec::new()`），用 `Drop` guard 兜住错误路径。验收项「连续触发 5 次无内存增长」就是查这个。

#### 建窗

```rust
let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("/#shot".into()))
    .decorations(false)
    .resizable(false)
    .shadow(false)      // Linux 不支持；Windows 会给无边框窗加白边
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(true)
    .visible(false)     // 先建后定位再显示，避免左上角闪一下
    .build()?;
// 几何一律取自 find_monitor_for_point() 选出的那台显示器的 position/size（物理量）
win.set_position(tauri::PhysicalPosition::new(m.position().x, m.position().y))?;
win.set_size(tauri::PhysicalSize::new(m.size().width, m.size().height))?;
win.show()?;
win.set_focus()?;
```

⚠️ **不得改用 tao 的 `monitor_from_point()`**（见 §3.1）。

#### `get_screenshot_session()` 必须是**幂等读**，不是一次性 take

`src/main.tsx` 外层是 `React.StrictMode`，**开发模式 effect 执行两次**。沿用 `PendingFloatRequests` 的一次性 take 会让第二次挂载拿到空会话。

#### 代际闸门

异步命令带 `generation`，落地前比对，不等则丢弃（与 `src/task/useTaskRunner.ts` 同构）。

#### 卡死兜底（三层，**必做**）

webview 内 Esc 只在窗口持有焦点时有效：

1. 建窗后 `set_focus()` + 保持 `always_on_top`。
2. `on_window_event` 的 `Focused(false)`：**仅当 `phase == Selecting && focused_once == true`** 才取消（`focused_once` 由首次 `Focused(true)` 置位，避免建窗瞬间假失焦误杀）。
3. **60 秒看门狗**：超时自动 cancel（防窗口被外部 WM 挂起后永久占屏）。
4. `CloseRequested` → `api.prevent_close()` 并走 cancel 路径，保证会话状态被清理。

⚠️ `WindowEvent` 是 `#[non_exhaustive]` → **match 必须带 `_ => {}`**。

#### 托盘触发需延迟 200–250ms

菜单弹层会被截进画面。**阶段 2 若加了托盘入口**（属阶段 5，可选）需注意。

### 5.5 自定义 URI 协议

```rust
.register_uri_scheme_protocol("translyshot", {
    let session = Arc::clone(&screenshot_session);
    move |ctx, req| {
        // req.uri() 形如：
        //   Linux/macOS: translyshot://localhost/{session_id}/base.png
        //   Windows:     http://translyshot.localhost/{session_id}/base.png
        // ⚠️ 用 UriSchemeContext::app_handle() 动态取，不要捕获 &AppHandle
        // session_id 不匹配当前会话 → 404
    }
})
```

**CSP 已配好**（`tauri.conf.json`）。**URL 由 Rust 拼好放进 `ScreenshotSessionDto::image_url`**，前端不做平台判断。

### 5.6 IPC 命令（`src-tauri/src/commands/screenshot.rs`）

命令**必须注册进 `src-tauri/src/lib.rs` 的 `app_command_builder!` 宏**（当前 42 条），否则前端调用静默失败。

阶段 2 需要的最小命令集（`start_screenshot_translation` 与 `cancel_screenshot_translation` 属阶段 5 入口，但**建窗逻辑本体在阶段 2**）：

```rust
#[tauri::command]
#[specta::specta]
pub async fn get_screenshot_session(...) -> Result<ScreenshotSessionDto, OcrErrorDto>;

#[tauri::command]
#[specta::specta]
pub async fn recognize_screenshot_selection(rect: NormRect, ...) -> Result<..., OcrErrorDto>;
// 阶段 2 可先实现为「返回空块」（OCR 在阶段 3）

#[tauri::command]
#[specta::specta]
pub async fn cancel_screenshot_translation(...) -> Result<(), String>;
```

**specta 约束（硬性）**：
- **禁导出 `u64`/`i64`**（specta 转 BigInt，前端 `JSON.stringify` 抛错）→ `generation` 用 `u32`。
- **禁嵌套 `Option`**（用扁平字段）——`takePendingFloatRequest` 的注释里有明确先例。
- 返回 `OcrErrorDto` 时用 `Result<T, OcrErrorDto>`，但**前端 i18n 的 `Record<OcrErrorKind, string>`** 会编译期强制补齐中英双份文案。

**绑定生成**：改完命令后必须跑

```bash
cargo run -p src-tauri --bin export_bindings
```

它会重写 `src/bindings.ts`。**CI 用 `git diff --exit-code src/bindings.ts` 强制同步**。开发期 `main.rs:99-100` 在 `debug_assertions` 下会**自动导出**。

### 5.7 前端 `src/main.tsx`

当前：

```tsx
const isFloatWindow = window.location.hash === '#float'
ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>{isFloatWindow ? <FloatWindow /> : <App />}</React.StrictMode>,
)
```

改为三分支（`#shot`）：

```tsx
const hash = window.location.hash
const isFloatWindow = hash === '#float'
const isShotWindow = hash === '#shot'
// ...
{isShotWindow ? <ScreenshotOverlay /> : isFloatWindow ? <FloatWindow /> : <App />}
```

### 5.8 `src/ScreenshotOverlay.tsx` —— 框选交互

**i18n 与主题**：独立窗口，**必须自行读 `localStorage` 并监听 `storage` 事件**（复刻 `src/FloatWindow.tsx:24-54` 的做法）。**LESSONS 第 2 条**：所有次级独立窗口必须能读用户偏好，不得硬编码。

| 交互 | 实现 |
|---|---|
| 十字准星 | 覆盖窗根节点 `cursor-crosshair` |
| 整体压暗 | 选区 div 加 `shadow-[0_0_0_100vmax_rgba(15,23,42,0.45)]`——单节点、GPU 合成、无裁剪计算 |
| 选区边框/四角 | 1px 边框 + 四角 6px 方块（纯装饰，不做拖拽改尺寸） |
| 拖拽 | `pointerdown/move/up` + `setPointerCapture`；**除以 `innerWidth`/`innerHeight`** 得归一化值；起点取 `min` 以支持四向拖拽 |
| 钳制 | **只在 Rust 侧保留一处实现**（`clamped_unit`），前端传原值，Rust 再钳一次 |
| 尺寸提示 | 显示**物理像素**：`round(w_norm * shot_width) × round(h_norm * shot_height)`（来自会话 DTO） |
| 轻点未拖拽 | `is_degenerate(rect, w, h, 8)` → 不提交不取消，保持等待 |
| Esc | 取消 |

**砍掉的项（不要实现）**：放大镜、八向手柄调整尺寸、OCR 结果可编辑、窗口智能吸附、跨显示器框选、截图存文件历史、平台原生真全屏。

### 5.9 Capabilities

`src-tauri/capabilities/screenshot-overlay.json` **已存在**：

```jsonc
{ "identifier": "screenshot-overlay", "windows": ["screenshot-*"],
  "permissions": ["core:event:default"] }
```

只要 `core:event:default`：`#shot` 分支**只调应用自有命令**（`#[tauri::command]` **不受 ACL 管控**），窗体的显示/定位/关闭全在 Rust 侧。

**覆盖窗 label 必须是 `screenshot-*` 形式**（如 `screenshot-1`），否则 ACL 拒绝。

> **注意**：新窗口 label **必须登记进 capabilities**，否则 `plugin:*` 调用被 ACL 静默拒绝。这是本项目已记录的坑。

### 5.10 i18n 文案（中英双份，**缺一编译报错**）

`src/i18n/index.ts:11-14` 把 `I18nSchema` 显式标注为 `typeof zhCN & { ocrError: Record<OcrErrorKind, string> }`，**en.json 缺键会在编译期报错**，这是有意的门禁（中英文案漂移是回归高发区）。

阶段 2 需要新增的键（建议放 `screenshot` 块）：
- 标题/提示「拖动框选要翻译的区域」
- 「识别中…」「翻译中…」
- 「还原原文」/「显示译文」
- 「取消」（含 Esc 提示）
- 尺寸提示模板
- 「发送到主窗口」（阶段 4）
- 引擎/语言下拉标签（可复用 `translation.*`）

**另有 `src/i18n/*.test.ts` 的对称性门禁**（`pnpm run test:task` 里跑）：中英两份 JSON 键结构必须完全对称。

### 5.11 覆盖率与门禁

- `crates/transly-core` 有 **70% 行覆盖率门禁**，当前 **91.13%**。**阶段 2 不应下降**。
- `src-tauri` 无覆盖率门禁，但有 `-D warnings`。
- ⚠️ **平台专属代码在 Linux 上编不到**（Windows/macOS 分支被 cfg 剔除），**其正确性只能靠评审 + 真机验收**。故**凡是能下沉到 `transly-core` 的纯逻辑必须下沉**（例如 BGRX→RGBA 重排、尺寸自校验判定、像素预算检查），这是它们获得测试覆盖的**唯一**途径。

---

## 6. 测试与验证要求

### 6.1 必须新增的单测（放 `transly-core`，受覆盖率门禁）

| 被测 | 必须能捕获的缺陷 |
|---|---|
| BGRX→RGBA 字节重排 | **R/B 通道互换**（灰度 UI 上肉眼不易察觉） |
| 捕获尺寸自校验判定 | 容差取错（±1 应通过；差 2 应失败） |
| 显示器几何匹配（已有 `find_monitor_for_point`） | 副屏坐标为负时匹配失败回退主屏；**误用逻辑坐标参与判定** |

**LESSONS 第 11 条强制动作**：新增回归测试写完，**必须把实现改回缺陷版确认测试 FAILED**，再恢复。

### 6.2 三平台 clippy 纪律（**这是最容易踩的坑**）

1. 平台模块在 `mod.rs` 里 cfg 门控。
2. 模块内平台专属 `use` **同样门控**。
3. **三段式** `windows / macos / not(any(...))`。
4. 测试代码同样受 `--all-targets` 约束 → 纯逻辑测试放 core 可完全免疫。
5. `register_uri_scheme_protocol` 的闭包必须 `Fn + Send + Sync + 'static` → 用 `Arc<ScreenshotSession>` 捕获，**禁捕获 `&AppHandle`**。
6. **全局 `unwrap()` 禁令**：`Monitor::all()`、`topCandidates(1)`、`performRequests_error` 全走 `?` 或 `unwrap_or_default()` 并注释理由。
7. `WindowEvent` 是 `#[non_exhaustive]` → match 必须带 `_ => {}`。

### 6.3 桌面 E2E（`scripts/test-desktop-ui-e2e.mjs`）

**当前有 7 个用例**，横幅打印「全部 7 项桌面端 UI 前端真实渲染与交互 E2E 测试 100% 通过」（在脚本结尾，**阶段 2 若加用例 8 需同步改这个数字**）。

新增「用例 8：截图覆盖窗 `#shot` 框选」。**直接把 `/#shot` 当第四个路由页面打开**，视口设 1200×800（与 mock 的 `shot_width/height` 一致，使归一化值可精确断言）。

**Mock 命令必须登记**（`invoke` 的 switch 新增，约在脚本 190-375 行）：

```js
case 'get_screenshot_session': ...
case 'get_screenshot_capability': ...
case 'recognize_screenshot_selection': ...
case 'translate_screenshot_blocks': ...
case 'cancel_screenshot_translation': ...
case 'start_screenshot_translation': ...
case 'push_screenshot_text_to_main': ...
```

⚠️ **未登记的命令会静默 `return null` 且 E2E 不失败**（`default` 分支只 `console.warn`，**不被 `consoleErrors` 捕获**）——**这是最易漏的坑**。

E2E 用 **`data:` URL 顶替自定义协议**（浏览器里没有 tauri 协议；`vite build` 产物无 CSP 故 data URL 可用）。CSP 里已备 `data:`。

**断言清单**（每条都要能因真实缺陷而失败）：

1. 背景图 `src` 非空且已 `complete`。
2. `mouse.move(240,160) → down → move(720,480) → up` 后，`__INVOKE_LOGS` 中 `recognize_screenshot_selection` 入参**精确等于** `{x:0.2, y:0.2, w:0.4, h:0.4}`（ε=0.001）→ 捕获「忘了除以窗口尺寸」「坐标轴搞反」。
3. 拖到视口外 → 入参被钳制为 `x+w ≤ 1, y+h ≤ 1` → 捕获「只钳起点不钳终点」。
4. 反向拖拽 → 得同样正向矩形 → 捕获「未取 min」。
5. Esc 后 `cancel_screenshot_translation` 被调用且覆盖层消失。
6. 识别完成后存在 2 个译文块，`style.left/top` 等于 `0.1/0.2` 的百分比形式 → 捕获「块索引错位」。
7. 点「还原原文」后译文层不可见、背景图仍可见 → 捕获「toggle 卸载了背景图」。
8. 控制台零 error（现有全局守卫已覆盖）。

**按 LESSONS 第 12 条**（E2E 的「静默降级」会把真实链路失效伪装成通过）：**禁用 `if (await x.isVisible())` 式静默跳过**；横幅**必须显式标注**「截图覆盖窗用例走 Mock IPC（前端无法拉起 Tauri 运行时），不代表自定义协议链路已验证」，避免把 mock 通过读成端到端通过。

### 6.4 三平台手动验收（**唯一能验证捕获的方式**）

| 平台 | 谁做 | 内容 |
|---|---|---|
| Linux X11 | 本机（若 WSL2 有 X server）或老大真机 | 覆盖窗定位、冻结帧对齐、连续 5 次无残留/无泄漏 |
| Windows | **老大真机** | 同上；含 **150% / 200% 缩放**与**双屏不同缩放** |
| macOS | **老大真机** | 同上；含首次授权弹窗、拒绝后文案、Retina |

**最关键的肉眼判据**：在**双屏不同缩放**的机器上触发截图，把某个有辨识度的窗口拖到**光标所在屏的角落**，看覆盖窗里的冻结帧是否与原屏**逐像素对齐**。若选错显示器或 DPI 折算错，这里会立刻表现为「截到另一块屏」或「图像被拉伸/裁切」——**这是显示器匹配与尺寸自校验是否正确的唯一可靠判据**。

**不得在检查点 1、2 未通过时进入阶段 3。**

---

## 7. 提交要求

- **约定式提交**，中文正文，类型/范围参照仓库现状（如 `feat(screenshot): 阶段 2 ...`）。
- **每阶段一个提交**，不要把一个阶段的改动摊成多个碎片提交。
- **提交信息必须写清**：做了什么、**为什么这样做**（尤其是反直觉的决策）、验证结果（命令 + 关键数字）、反向验证情况。
- **不要用 `git add -A` 盲加**：先 `git status` 看清每个文件。
- **`.gitignore` 只添加不删减**（当前已含 `.claude/`）。
- 提交前跑**全部**门禁命令（§0.3），包括 `cargo run -p src-tauri --bin export_bindings` 后确认 `src/bindings.ts` 无意外 diff。

**注意**：仓库有 `lefthook` pre-commit 钩子，会自动跑 8 项检查 + 提交信息校验。钩子失败时**不要 `--no-verify` 绕过**，要修到通过。

---

## 8. 已确认的坑与已知限制（**不要重新踩**）

| 坑 | 说明 |
|---|---|
| **tao `monitor_from_point` 单位不一致** | 入参是逻辑坐标，与 `cursor_position()` 的物理返回单位不符。**不得使用**（§3.1） |
| **macOS `Monitor::size()` 疑似双重缩放** | `CGDisplayPixelsWide()` 已是物理量又乘 scale。靠尺寸自校验兜住（§5.3） |
| **`CGDisplayCreateImage` 带 `#[deprecated]`** | `-D warnings` 下**必须** `#[allow(deprecated)]`，否则编译失败（§3.2） |
| **macOS 取像素只有 `CGDataProvider → CFData` 一条路** | `CGBitmapContextCreate` 的 plain 版在该 crate 中不存在。且 `byte_ptr()` 的裸指针只在 `CFData` 存活期内有效（§3.2） |
| **`xcb` 有 build.rs** | 旧计划书说法有误，但「不需系统头文件」的结论仍成立（§3.3） |
| **`GetImageReply::length()` 是私有的** | 用 `data()` 拿字节，别调 `length()`（§3.4） |
| **GDI 资源泄漏** | `HBITMAP`/`HDC` 必须 RAII 释放，含错误路径（§5.2） |
| **受保护窗口（DRM）无法捕获** | OS 层封锁（Windows `WDA_EXCLUDEFROMCAPTURE`），**任何用户态后端都无效**，换后端无意义。给明确提示，不要静默失败 |
| **Wayland 不支持** | ① 系统级截图 API 需落临时文件，违背零落盘承诺；② compositor 不认 `setAlwaysOnTop`（GNOME/Mutter 无 `zwlr_layer_shell_v1`）。**降级并给 X11 指引**，文案**不要写「暂未支持」**（会误导用户以为永远不行） |
| **4K 帧内存泄漏** | 33MB/帧，必须显式释放；验收含「连续 5 次无内存增长」 |
| **`React.StrictMode` 双执行 effect** | `get_screenshot_session()` 必须**幂等读**，不能用一次性 take |
| **specta 不支持 `u64`/`i64`** | `generation` 用 `u32` |
| **specta 不支持嵌套 `Option`** | 用扁平字段 |
| **E2E mock 未登记命令静默返回 null** | 且 `console.warn` **不被 `consoleErrors` 捕获**，用例照样通过（§6.3） |
| **`#[non_exhaustive]` 枚举** | `WindowEvent` match 必须带 `_ => {}` |
| **`write_to` 要求 `Write + Seek`** | `Vec<u8>` 不满足，用 `Cursor<Vec<u8>>`（§3.6） |
| **WSL2 可能无 X server** | `xcb` 连接失败是**预期行为**，不是缺陷（§3.8） |
| **覆盖窗 label 必须是 `screenshot-*`** | 否则 ACL 拒绝 `plugin:*`（§5.9） |

---

## 9. 开工前自查清单

- [ ] `git log --oneline -3` 确认 HEAD 是 `c1e1dee`（阶段 1）
- [ ] `git status` 干净
- [ ] 读过 `AGENTS.md`（分层铁律 + 安全第一 + CI 门禁）
- [ ] 读过 `LESSONS.md` 第 11 条（反向验证强制动作）与第 12 条（E2E 静默降级）
- [ ] 读过本单 §3（已核实事实）与 §8（已确认的坑）
- [ ] 跑过一次基线门禁：`cargo test --workspace` 应得 **72 + 178 = 250 passed**
- [ ] 知道本机**未装 tesseract**（阶段 2 不需要）、**可能是 WSL2 无 X server**

## 10. 完成后交付要求

1. 所有门禁命令通过（§0.3），并把**实际输出**附在汇报里（不要只说「通过」）。
2. 阶段 2 的三项手动验收（§1.3）在**能做的平台**上做过，并**如实说明哪些平台未做**（含未做的原因）。
3. 反向验证（LESSONS 第 11 条）对新测试执行过，并说明**哪几处确认 FAILED**。
4. 新增的源码目录补 `README.md`（全局 `CLAUDE.md` 要求）。
5. 有踩坑价值的内容追加到 `LESSONS.md`（**简短**，避免文档沉余）。
6. 汇报里明确列出**未完成项与已知限制**，不要报「全部完成」而掩盖缺口。

---

## 附：阶段 3–6 概览（供参考，**本单不要求实施**）

| 阶段 | 内容 | 关键前置 |
|---|---|---|
| **3** 三平台 OCR（2.5d） | `ocr.rs` 拆目录，实现 `recognize_blocks`，`recognize_screenshot_selection` 打通，覆盖窗渲染块占位 | **Linux 首个动作：实装 tesseract 跑通 `tesseract - - tsv` 管道**（本机未装，这是整条 Linux 路径的前提假设，必须先证伪/证实） |
| **4** 批量翻译+叠译文（2d） | 接入 `translate_screenshot_blocks`（用 `translate_batch_with_cache`）；前端 `layout.ts`（`fitFontSize`/`toPercentStyle`/`clampRenderedBox`）；译文层 + 还原原文 + 引擎/语言下拉 + 复制 + 单块重试 | 阶段 3 完成 |
| **5** 三入口+降级探测（1.5d） | 主窗口按钮、托盘项、第三个全局快捷键（默认 `Alt+Shift+S`，复用 `src/ShortcutRecorder.tsx`）；`ShortcutStatusSlot` 加第三槽位；`get_screenshot_capability()` | 阶段 2 完成 |
| **6** E2E+文档（1.5d） | 用例 8；CHANGELOG/README/PROJECT_RULES/各 README 更新 | 全部 |

**依赖**：0→1→2→3→4 是主干；5 依赖 2；6 收尾。
