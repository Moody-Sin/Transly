//! 归一化几何契约（截图翻译的坐标地基）。
//!
//! # 为什么必须是归一化坐标
//!
//! 截图的**物理像素**与覆盖窗的 **CSS 逻辑像素**在任何 DPI 下都不相等
//! （Windows 200% 缩放、macOS Retina）。一旦像素坐标过 IPC，缩放系数就必须
//! 前后端同步维护，DPI 差异必然漂移。归一化到 `[0,1]²` 后前端只需
//! `left: x*100%`，全链路零缩放系数。
//!
//! # 坐标系约定（唯一契约，全仓共用）
//!
//! - 原点在**左上角**，x 向右、y 向下；
//! - 分量取值范围 `[0,1]`，表示相对于**参考图**的占比；
//! - `x + w <= 1`、`y + h <= 1` 由 [`NormRect::clamped_unit`] 保证。
//!
//! ⚠️ macOS Vision 的 `boundingBox()` 返回**原点左下、y 向上**的归一化矩形，
//! 与本法相反，必须经 [`NormRect::from_vision_norm`] 转换（缺这一步会让整块
//! 译文垂直镜像，且肉眼不易察觉）。

use serde::{Deserialize, Serialize};

/// 归一化矩形。原点左上，分量 ∈ `[0,1]`。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// 由**某张参考图内**的像素矩形归一化。
    ///
    /// ⚠️ `img_w`/`img_h` 必须是**喂给 OCR 的那张图**的尺寸，而非原图尺寸。
    /// Windows OCR 在 `clamp_dimensions` 下采样后，`OcrWord::BoundingRect()`
    /// 返回的是**下采样图**的像素；只要分母用下采样后的尺寸，下采样被自动吸收，
    /// 无需返算。用错分母会让所有块的坐标整体偏移。
    pub fn from_pixels_in(px: u32, py: u32, pw: u32, ph: u32, img_w: u32, img_h: u32) -> Self {
        if img_w == 0 || img_h == 0 {
            return Self::zero();
        }
        let w = img_w as f32;
        let h = img_h as f32;
        Self::new(px as f32 / w, py as f32 / h, pw as f32 / w, ph as f32 / h)
    }

    /// 由 macOS Vision 的归一化矩形转换为本契约。
    ///
    /// Vision 原点在**左下、y 向上**；本契约原点在**左上、y 向下**。
    /// 换算：`y_top = 1 - y_bottom - h`（Vision 的 `y` 是矩形**下边缘**）。
    pub fn from_vision_norm(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::new(x, 1.0 - y - h, w, h)
    }

    /// 反向：本契约 → Vision 归一化（供测试与将来的回写路径使用）。
    pub fn to_vision_norm(&self) -> (f32, f32, f32, f32) {
        (self.x, 1.0 - self.y - self.h, self.w, self.h)
    }

    /// 把**子矩形内**的归一化矩形映射回父图（本矩形所在）的归一化坐标。
    ///
    /// # 为什么需要它
    ///
    /// 框选截图时只把选区那一块**裁剪**出来送 OCR（面积小、识别快、上下文聚焦），
    /// 于是块坐标是**相对裁剪图**的归一化值。但块最终要渲染在整张冻结帧上，
    /// 故必须按「选区在父图中的位置与尺寸」把坐标还原回全图。
    /// **不还原会让所有译文块挤在选区左上角，且选区越靠右下、偏移越大**
    /// ——偏移量恰好等于选区原点的归一化坐标，在右下角框选时肉眼可见。
    ///
    /// `self` 是选区在父图中的位置（调用方应已 `clamped_unit`），
    /// `inner` 是子图内的归一化矩形。
    ///
    /// 退化选区（宽或高为 0/负/NaN）没有可映射的内部坐标空间，
    /// 返回 [`NormRect::zero`]：负宽高会产出反向矩形，NaN 会污染后续全部
    /// 几何计算（与 [`NormRect::clamped_unit`] 对 NaN 的立场一致），
    /// 两者都比「什么都没有」更难排查。
    pub fn map_from_subrect(&self, inner: &NormRect) -> NormRect {
        // 写成 `!(w > 0.0 && h > 0.0)` 而非 `w <= 0.0`：NaN 与任何值比较均为 false，
        // 故这一种写法同时兜住 0、负值、NaN 三种退化输入
        if !(self.w > 0.0 && self.h > 0.0) {
            return NormRect::zero();
        }
        Self::new(
            self.x + inner.x * self.w,
            self.y + inner.y * self.h,
            inner.w * self.w,
            inner.h * self.h,
        )
    }

    /// 映射回**指定尺寸图**上的像素矩形：钳制到图内，且宽高至少 1px。
    ///
    /// 取整后必须钳制，否则 `x+w` 恰好落在边界上时会越界导致裁剪 panic。
    pub fn to_pixels_in(&self, img_w: u32, img_h: u32) -> (u32, u32, u32, u32) {
        if img_w == 0 || img_h == 0 {
            return (0, 0, 0, 0);
        }
        let c = self.clamped_unit();

        let px = (c.x * img_w as f32).floor().max(0.0) as u32;
        let py = (c.y * img_h as f32).floor().max(0.0) as u32;

        // 先算右/下边界再相减，避免 x+w 取整后越界
        let right = ((c.x + c.w) * img_w as f32).ceil().max(0.0) as u32;
        let bottom = ((c.y + c.h) * img_h as f32).ceil().max(0.0) as u32;

        let right = right.min(img_w);
        let bottom = bottom.min(img_h);
        let px = px.min(img_w.saturating_sub(1));
        let py = py.min(img_h.saturating_sub(1));

        let pw = right.saturating_sub(px).max(1).min(img_w - px);
        let ph = bottom.saturating_sub(py).max(1).min(img_h - py);
        (px, py, pw, ph)
    }

    /// 钳制到单位正方形内：`x,y >= 0`、`x+w <= 1`、`y+h <= 1`。
    ///
    /// 拖拽越界的**唯一收口点**：前端传原值，Rust 侧再钳一次。
    pub fn clamped_unit(&self) -> Self {
        // 先处理 NaN/Inf，再钳制（`clamp` 对 NaN 会返回 NaN）
        let x = sanitize(self.x);
        let y = sanitize(self.y);
        let w = sanitize(self.w);
        let h = sanitize(self.h);

        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        // 宽度不能超出右边界，且不为负
        let w = w.clamp(0.0, 1.0 - x);
        let h = h.clamp(0.0, 1.0 - y);
        Self::new(x, y, w, h)
    }

    /// 是否为「退化」矩形（轻点未拖拽）。
    ///
    /// 在 `img_w × img_h` 的图上，宽或高小于 `min_px` 即视为退化。
    /// 用于区分「用户点了一下」与「用户框选了一个极小区域」。
    pub fn is_degenerate(&self, img_w: u32, img_h: u32, min_px: u32) -> bool {
        if img_w == 0 || img_h == 0 {
            return true;
        }
        let w_px = self.w.abs() * img_w as f32;
        let h_px = self.h.abs() * img_h as f32;
        // 严格小于：恰好等于 min_px 视为有效选区
        w_px < min_px as f32 || h_px < min_px as f32
    }

    /// 由两个角点构造（支持四向拖拽）。
    ///
    /// 传入的起点/终点顺序任意，内部取 `min` 归一化为左上原点。
    pub fn from_corners(ax: f32, ay: f32, bx: f32, by: f32) -> Self {
        let (x0, x1) = if ax <= bx { (ax, bx) } else { (bx, ax) };
        let (y0, y1) = if ay <= by { (ay, by) } else { (by, ay) };
        Self::new(x0, y0, x1 - x0, y1 - y0)
    }

    /// 叠加平移（渲染时用，不改变尺寸）
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// 把 NaN/±Inf 归零。`f32::clamp` 对 NaN 返回 NaN，会让后续所有计算被污染。
fn sanitize(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// 显示器的物理几何（Tauri `Monitor` 的扁平化快照）。
///
/// 刻意不直接依赖 `tauri::Monitor`：core 禁止 Tauri 依赖，且扁平结构可单测。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct MonitorRect {
    /// 物理坐标（虚拟桌面原点左上，副屏可为负）
    pub x: i32,
    pub y: i32,
    /// 物理尺寸
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

/// 在显示器列表中定位包含给定**物理**点的显示器。
///
/// # 为什么不用平台自带的 `monitor_from_point`
///
/// tao 在 Linux 上的坐标单位自相矛盾：`cursor_position()` 返回**物理**量
/// （`LogicalPosition::to_physical(scale)`），而 `monitor_from_point(x, y)`
/// 把入参**原样喂给 GDK** 的 `monitor_at_point()`（GDK 吃**逻辑**坐标）。
/// 实测 `GDK_SCALE=2` 下 `Monitor::size()` 报 2560×1440、`cursor_position()`
/// 报 (1800,900)，而逻辑框只有 1280×720 —— 两者不可能同时成立。
/// 因此本函数全程用物理量做判定，不依赖任何平台 API 的单位假设。
///
/// 找不到时返回 `None`，由调用方决定回退策略（通常取主显示器）。
///
/// ⚠️ 刻意**不**「取最近的显示器」：光标落在虚拟桌面缝隙时，猜一台
/// 会让覆盖窗盖到不含光标的屏上，比直接失败更糟。回退策略属于调用方
/// 的产品决策，不该藏在这里。
pub fn find_monitor_for_point(cursor: (i32, i32), monitors: &[MonitorRect]) -> Option<usize> {
    if monitors.is_empty() {
        return None;
    }

    let (cx, cy) = cursor;
    monitors.iter().position(|m| {
        let (x0, y0) = (m.x as i64, m.y as i64);
        let x1 = x0 + m.width as i64;
        let y1 = y0 + m.height as i64;
        let (cx, cy) = (cx as i64, cy as i64);
        // 右/下边界取开区间，避免相邻显示器边界点同时命中两台
        cx >= x0 && cx < x1 && cy >= y0 && cy < y1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn test_from_pixels_in_basic() {
        let r = NormRect::from_pixels_in(100, 50, 200, 100, 1000, 500);
        assert!(approx(r.x, 0.1));
        assert!(approx(r.y, 0.1));
        assert!(approx(r.w, 0.2));
        assert!(approx(r.h, 0.2));
    }

    /// 关键回归：分母必须是**喂给 OCR 那张图**的尺寸，而非原图。
    ///
    /// Windows OCR 在 `clamp_dimensions` 下采样后，`OcrWord::BoundingRect()`
    /// 返回下采样图的像素。若误用原图尺寸作分母，所有块坐标会整体偏移。
    #[test]
    fn test_from_pixels_in_uses_downsampled_denominator() {
        // 原图 5200×2600 被下采样到 2600×1300（clamp_dimensions(5200,2600,2600)）
        let (nw, nh) = crate::ocr::clamp_dimensions(5200, 2600, 2600);
        assert_eq!((nw, nh), (2600, 1300));

        // 下采样图中心处一个 260×130 的块
        let r = NormRect::from_pixels_in(650, 325, 260, 130, nw, nh);

        // 用下采样尺寸作分母 → 与用原图像素表达同一区域的结果应一致
        let expected = NormRect::from_pixels_in(1300, 650, 520, 260, 5200, 2600);
        assert!(approx(r.x, expected.x), "x 偏移：{} vs {}", r.x, expected.x);
        assert!(approx(r.y, expected.y));
        assert!(approx(r.w, expected.w));
        assert!(approx(r.h, expected.h));

        // 若误用原图尺寸作分母（缺陷版），结果会缩水一半
        let wrong = NormRect::from_pixels_in(650, 325, 260, 130, 5200, 2600);
        assert!(
            !approx(wrong.x, expected.x),
            "分母写错却不影响结果，测试无效"
        );
    }

    #[test]
    fn test_from_pixels_in_zero_image() {
        let r = NormRect::from_pixels_in(0, 0, 10, 10, 0, 0);
        assert_eq!(r, NormRect::zero());
    }

    /// 关键回归：Vision 的 y 轴必须翻转。
    /// 缺这行会让整块译文垂直镜像，且肉眼不易察觉。
    #[test]
    fn test_from_vision_norm_flips_y() {
        // Vision 报告左上角一个 0.2×0.1 的框：y 是**下边缘**，故 y = 1 - 0.1 - 0.1 = 0.8
        let r = NormRect::from_vision_norm(0.1, 0.8, 0.2, 0.1);
        assert!(approx(r.x, 0.1));
        assert!(approx(r.y, 0.1), "y 应翻转为 0.1，实际 {}", r.y);
        assert!(approx(r.w, 0.2));
        assert!(approx(r.h, 0.1));

        // 底部元素应落到 y 接近 1
        let bottom = NormRect::from_vision_norm(0.0, 0.0, 0.5, 0.05);
        assert!(approx(bottom.y, 0.95), "实际 {}", bottom.y);
    }

    #[test]
    fn test_vision_norm_roundtrip() {
        let orig = NormRect::new(0.25, 0.35, 0.4, 0.2);
        let (vx, vy, vw, vh) = orig.to_vision_norm();
        let back = NormRect::from_vision_norm(vx, vy, vw, vh);
        assert!(approx(back.x, orig.x));
        assert!(approx(back.y, orig.y));
        assert!(approx(back.w, orig.w));
        assert!(approx(back.h, orig.h));
    }

    /// 选区就是整张图时，映射必须是恒等变换。
    #[test]
    fn test_map_from_subrect_full_image_is_identity() {
        let full = NormRect::new(0.0, 0.0, 1.0, 1.0);
        let inner = NormRect::new(0.1, 0.2, 0.3, 0.4);
        let m = full.map_from_subrect(&inner);
        assert!(approx(m.x, inner.x));
        assert!(approx(m.y, inner.y));
        assert!(approx(m.w, inner.w));
        assert!(approx(m.h, inner.h));
    }

    /// 关键回归：选区在**右下角**时必须加上选区原点偏移、并按选区尺寸缩放。
    ///
    /// 漏加 `self.x`/`self.y` 会让所有块挤在选区左上角（本例应落在 0.8 而非 0.2）；
    /// 漏乘 `self.w`/`self.h` 会让块尺寸按整图比例算（本例宽应为 0.2 而非 0.5）。
    /// 选区越靠右下，漏加偏移的错位越明显。
    #[test]
    fn test_map_from_subrect_bottom_right_selection() {
        let sel = NormRect::new(0.6, 0.6, 0.4, 0.4);

        // 子图右下角的块 → 回到父图右下角
        let corner = sel.map_from_subrect(&NormRect::new(0.5, 0.5, 0.5, 0.5));
        assert!(approx(corner.x, 0.8), "漏加选区原点：{}", corner.x);
        assert!(approx(corner.y, 0.8), "漏加选区原点：{}", corner.y);
        assert!(approx(corner.w, 0.2), "漏乘选区宽：{}", corner.w);
        assert!(approx(corner.h, 0.2), "漏乘选区高：{}", corner.h);

        // 子图铺满选区 → 恰好还原成选区本身
        let whole = sel.map_from_subrect(&NormRect::new(0.0, 0.0, 1.0, 1.0));
        assert!(approx(whole.x, sel.x));
        assert!(approx(whole.y, sel.y));
        assert!(approx(whole.w, sel.w));
        assert!(approx(whole.h, sel.h));
    }

    /// 选区跨中（非整数倍缩放）时同样必须平移 + 缩放。
    #[test]
    fn test_map_from_subrect_centered_selection() {
        let sel = NormRect::new(0.25, 0.5, 0.5, 0.25);
        let m = sel.map_from_subrect(&NormRect::new(0.2, 0.4, 0.4, 0.4));
        assert!(approx(m.x, 0.35));
        assert!(approx(m.y, 0.6));
        assert!(approx(m.w, 0.2));
        assert!(approx(m.h, 0.1));
    }

    /// 关键回归：完整链路——裁剪后按**裁剪图**归一化，再用 `map_from_subrect`
    /// 还原，结果必须与「直接按全图像素归一化」完全一致。
    ///
    /// 这是 §4.4 的真实路径（裁剪 → OCR → 归一化 → 映射回全图）。
    /// 归一化分母若误用全图尺寸，或漏做 `map_from_subrect`，本断言都会失败。
    #[test]
    fn test_map_from_subrect_round_trips_cropped_normalization() {
        let (img_w, img_h) = (1000u32, 800u32);
        let sel = NormRect::new(0.5, 0.25, 0.3, 0.5);
        let (cx, cy, cw, ch) = sel.to_pixels_in(img_w, img_h);
        assert_eq!((cx, cy, cw, ch), (500, 200, 300, 400), "裁剪矩形与预期不符");

        // 裁剪图内一个块：像素 (60,40,120,80)
        let inner = NormRect::from_pixels_in(60, 40, 120, 80, cw, ch);
        let mapped = sel.map_from_subrect(&inner);

        // 同一个块按全图像素直接归一化（独立参照）
        let direct = NormRect::from_pixels_in(cx + 60, cy + 40, 120, 80, img_w, img_h);
        assert!(
            approx(mapped.x, direct.x),
            "x：{} vs {}",
            mapped.x,
            direct.x
        );
        assert!(
            approx(mapped.y, direct.y),
            "y：{} vs {}",
            mapped.y,
            direct.y
        );
        assert!(
            approx(mapped.w, direct.w),
            "w：{} vs {}",
            mapped.w,
            direct.w
        );
        assert!(
            approx(mapped.h, direct.h),
            "h：{} vs {}",
            mapped.h,
            direct.h
        );
    }

    /// 边界：选区宽或高为 0/负/NaN 时返回零矩形，绝不 panic。
    ///
    /// 这类退化选区在 `to_pixels_in` 里会被钳成 1px，不应产出负宽高矩形
    /// 或 NaN 污染后续几何计算。
    #[test]
    fn test_map_from_subrect_degenerate_selection_returns_zero() {
        let inner = NormRect::new(0.1, 0.1, 0.2, 0.2);

        for sel in [
            NormRect::new(0.3, 0.3, 0.0, 0.4),      // 零宽
            NormRect::new(0.3, 0.3, 0.4, 0.0),      // 零高
            NormRect::zero(),                       // 完全退化
            NormRect::new(0.1, 0.1, -0.2, 0.3),     // 负宽
            NormRect::new(0.1, 0.1, 0.2, f32::NAN), // NaN 高（`w <= 0.0` 写法会漏过）
        ] {
            assert_eq!(
                sel.map_from_subrect(&inner),
                NormRect::zero(),
                "退化选区 {sel:?} 应返回零矩形"
            );
        }

        // 选区有效、内部矩形退化 → 落在选区原点上的零尺寸矩形（而非全图零矩形）
        let sel = NormRect::new(0.4, 0.5, 0.2, 0.2);
        assert_eq!(
            sel.map_from_subrect(&NormRect::zero()),
            NormRect::new(0.4, 0.5, 0.0, 0.0)
        );
    }

    #[test]
    fn test_to_pixels_in_basic() {
        let r = NormRect::new(0.1, 0.2, 0.3, 0.4);
        let (px, py, pw, ph) = r.to_pixels_in(1000, 500);
        assert_eq!(px, 100);
        assert_eq!(py, 100);
        assert_eq!(pw, 300);
        assert_eq!(ph, 200);
    }

    /// 关键回归：取整后越界必须被钳制，否则裁剪会 panic。
    #[test]
    fn test_to_pixels_in_clamps_after_rounding() {
        // 右下角一个矩形，x+w 恰好为 1.0
        let r = NormRect::new(0.9, 0.9, 0.1, 0.1);
        let (px, py, pw, ph) = r.to_pixels_in(999, 999);
        assert!(px + pw <= 999, "右边界越界：{}+{} > 999", px, pw);
        assert!(py + ph <= 999);

        // 浮点累积导致 x+w 略大于 1 的场景
        let r2 = NormRect::new(0.9999999, 0.9999999, 0.0000001, 0.0000001);
        let (px2, py2, pw2, ph2) = r2.to_pixels_in(100, 100);
        assert!(px2 + pw2 <= 100);
        assert!(py2 + ph2 <= 100);
        assert!(pw2 >= 1 && ph2 >= 1, "宽高至少 1px");
    }

    #[test]
    fn test_to_pixels_in_zero_size() {
        let r = NormRect::new(0.1, 0.1, 0.1, 0.1);
        assert_eq!(r.to_pixels_in(0, 0), (0, 0, 0, 0));
    }

    /// 关键回归：只钳 x 不钳 x+w 是常见缺陷。
    #[test]
    fn test_clamped_unit_clamps_both_edges() {
        let r = NormRect::new(0.8, 0.9, 0.5, 0.5).clamped_unit();
        assert!(approx(r.x, 0.8));
        assert!(r.x + r.w <= 1.0 + EPS, "右边界未钳制：{}", r.x + r.w);
        assert!(r.y + r.h <= 1.0 + EPS, "下边界未钳制：{}", r.y + r.h);
        assert!(approx(r.w, 0.2), "宽应被裁剪为 0.2，实际 {}", r.w);
        assert!(approx(r.h, 0.1));

        // 负坐标归零
        let neg = NormRect::new(-0.5, -0.5, 0.3, 0.3).clamped_unit();
        assert!(approx(neg.x, 0.0));
        assert!(approx(neg.y, 0.0));
        assert!(approx(neg.w, 0.3), "负原点不应改变宽度");
    }

    #[test]
    fn test_clamped_unit_handles_nan() {
        let r = NormRect::new(f32::NAN, f32::INFINITY, f32::NAN, f32::NEG_INFINITY).clamped_unit();
        assert!(r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite());
        assert!(r.x >= 0.0 && r.y >= 0.0 && r.w >= 0.0 && r.h >= 0.0);
    }

    /// 关键回归：8px 边界的 off-by-one。
    #[test]
    fn test_is_degenerate_boundary() {
        // 在 1000×1000 图上，0.008 = 8px
        let exactly_8 = NormRect::new(0.1, 0.1, 0.008, 0.008);
        assert!(
            !exactly_8.is_degenerate(1000, 1000, 8),
            "恰好 8px 应视为有效"
        );

        let just_under = NormRect::new(0.1, 0.1, 0.0079, 0.5);
        assert!(
            just_under.is_degenerate(1000, 1000, 8),
            "不足 8px 应视为退化"
        );

        // 高度不足同样退化
        let thin = NormRect::new(0.1, 0.1, 0.5, 0.001);
        assert!(thin.is_degenerate(1000, 1000, 8));

        // 零尺寸图应视为退化
        assert!(NormRect::zero().is_degenerate(0, 0, 8));
    }

    #[test]
    fn test_from_corners_normalizes_reverse_drag() {
        // 正向拖拽
        let fwd = NormRect::from_corners(0.2, 0.2, 0.6, 0.5);
        // 反向拖拽（四个方向）
        let rev1 = NormRect::from_corners(0.6, 0.2, 0.2, 0.5);
        let rev2 = NormRect::from_corners(0.2, 0.5, 0.6, 0.2);
        let rev3 = NormRect::from_corners(0.6, 0.5, 0.2, 0.2);

        for (name, r) in [("rev1", rev1), ("rev2", rev2), ("rev3", rev3)] {
            assert!(approx(r.x, fwd.x), "{name} x 不一致");
            assert!(approx(r.y, fwd.y), "{name} y 不一致");
            assert!(approx(r.w, fwd.w), "{name} w 不一致");
            assert!(approx(r.h, fwd.h), "{name} h 不一致");
        }
    }

    #[test]
    fn test_find_monitor_for_point_single() {
        let m = MonitorRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
        };
        assert_eq!(find_monitor_for_point((500, 500), &[m]), Some(0));
        // 右/下边界是开区间
        assert_eq!(find_monitor_for_point((1920, 500), &[m]), None);
        assert_eq!(find_monitor_for_point((1919, 1079), &[m]), Some(0));
    }

    /// 关键回归：副屏坐标为负时不能匹配失败回退主屏。
    #[test]
    fn test_find_monitor_for_point_negative_secondary() {
        // 主屏 1920×1080 @ (0,0)，副屏 1920×1080 @ (-1920,0)
        let monitors = [
            MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale_factor: 1.0,
            },
            MonitorRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
                scale_factor: 1.0,
            },
        ];
        assert_eq!(find_monitor_for_point((-100, 500), &monitors), Some(1));
        assert_eq!(find_monitor_for_point((100, 500), &monitors), Some(0));
        // 边界：x=-1920 属于副屏
        assert_eq!(find_monitor_for_point((-1920, 0), &monitors), Some(1));
        // 边界：x=0 属于主屏（副屏右边界 -1）
        assert_eq!(find_monitor_for_point((0, 0), &monitors), Some(0));
    }

    /// 关键回归：混合 DPI 下不能靠「逻辑坐标」匹配。
    ///
    /// 主屏 scale=1（1920×1080 逻辑=物理），副屏 scale=2（物理 3840×2160 @ x=1920）。
    /// 若误用逻辑坐标（副屏宽 1920），落在 x=2500 的光标会被判为「不在副屏」。
    #[test]
    fn test_find_monitor_for_point_mixed_dpi_uses_physical() {
        let monitors = [
            MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale_factor: 1.0,
            },
            MonitorRect {
                x: 1920,
                y: 0,
                width: 3840, // 物理宽度
                height: 2160,
                scale_factor: 2.0,
            },
        ];
        // 光标在副屏靠右处（物理 2500）——按逻辑宽度 1920 会判为越界
        assert_eq!(find_monitor_for_point((2500, 500), &monitors), Some(1));
        assert_eq!(find_monitor_for_point((3800, 2000), &monitors), Some(1));
        // 副屏右边界
        assert_eq!(find_monitor_for_point((5759, 2159), &monitors), Some(1));
        assert_eq!(find_monitor_for_point((5760, 2159), &monitors), None);
    }

    /// 光标落在虚拟桌面缝隙（所有显示器之外）时返回 `None`。
    ///
    /// 刻意不「取最近者」：猜一台会让覆盖窗盖到不含光标的屏上，
    /// 比明确失败更糟。回退主显示器是调用方的决策。
    #[test]
    fn test_find_monitor_for_point_gap_returns_none() {
        let monitors = [
            MonitorRect {
                x: 0,
                y: 0,
                width: 1000,
                height: 1000,
                scale_factor: 1.0,
            },
            MonitorRect {
                x: 2000,
                y: 0,
                width: 1000,
                height: 1000,
                scale_factor: 1.0,
            },
        ];
        // x=1500 落在缝隙中（1000..2000 之间）
        assert_eq!(find_monitor_for_point((1500, 500), &monitors), None);
        // 明显落在某台内仍然正常命中
        assert_eq!(find_monitor_for_point((500, 500), &monitors), Some(0));
        assert_eq!(find_monitor_for_point((2500, 500), &monitors), Some(1));
        // 纵向缝隙
        assert_eq!(find_monitor_for_point((500, 1500), &monitors), None);
    }

    #[test]
    fn test_find_monitor_for_point_empty() {
        assert_eq!(find_monitor_for_point((0, 0), &[]), None);
    }

    #[test]
    fn test_translated() {
        let r = NormRect::new(0.1, 0.1, 0.2, 0.2).translated(0.05, -0.05);
        assert!(approx(r.x, 0.15));
        assert!(approx(r.y, 0.05));
        assert!(approx(r.w, 0.2), "平移不改变尺寸");
    }
}
