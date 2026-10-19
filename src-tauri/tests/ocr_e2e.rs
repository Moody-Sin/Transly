//! 端到端验证：造一张**已知布局**的图，断言 OCR 块的坐标落在正确象限。
//!
//! 这是唯一能捕获「坐标轴搞反」「y 翻转漏做」「归一化分母用错」的测试——
//! 纯坐标单测覆盖不到「引擎返回的像素框」这一段真实链路。
//!
//! 仅在 Linux（tesseract）上运行；Windows/macOS 走系统 API，
//! 且无法保证 CI 机器装了语言包，故用 cfg 限定。

#![cfg(all(not(target_os = "windows"), not(target_os = "macos")))]

use image::{Rgba, RgbaImage};

/// 用内置 5x7 点阵字体画文字，避免依赖系统字体与外部工具。
fn draw_text(img: &mut RgbaImage, x: u32, y: u32, text: &str, scale: u32) {
    let glyph = |c: char| -> [&'static str; 7] {
        match c {
            'A' => [
                "01110", "10001", "10001", "11111", "10001", "10001", "10001",
            ],
            'B' => [
                "11110", "10001", "11110", "10001", "10001", "10001", "11110",
            ],
            'C' => [
                "01111", "10000", "10000", "10000", "10000", "10000", "01111",
            ],
            'D' => [
                "11110", "10001", "10001", "10001", "10001", "10001", "11110",
            ],
            'E' => [
                "11111", "10000", "11110", "10000", "10000", "10000", "11111",
            ],
            'H' => [
                "10001", "10001", "10001", "11111", "10001", "10001", "10001",
            ],
            'L' => [
                "10000", "10000", "10000", "10000", "10000", "10000", "11111",
            ],
            'O' => [
                "01110", "10001", "10001", "10001", "10001", "10001", "01110",
            ],
            'R' => [
                "11110", "10001", "10001", "11110", "10100", "10010", "10001",
            ],
            'W' => [
                "10001", "10001", "10001", "10101", "10101", "11011", "10001",
            ],
            _ => ["00000"; 7],
        }
    };
    let black = Rgba([0, 0, 0, 255]);
    let mut cx = x;
    for ch in text.chars() {
        for (ry, row) in glyph(ch).iter().enumerate() {
            for (rx, bit) in row.chars().enumerate() {
                if bit != '1' {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = cx + rx as u32 * scale + sx;
                        let py = y + ry as u32 * scale + sy;
                        if px < img.width() && py < img.height() {
                            img.put_pixel(px, py, black);
                        }
                    }
                }
            }
        }
        cx += 6 * scale;
    }
}

#[test]
fn test_recognize_blocks_places_text_in_correct_quadrants() {
    // 1000x600 白底图，左上角 "HELLO"、右下角 "WORLD"
    let mut img = RgbaImage::from_pixel(1000, 600, Rgba([255, 255, 255, 255]));
    draw_text(&mut img, 40, 40, "HELLO", 5);
    draw_text(&mut img, 640, 440, "WORLD", 5);

    let blocks = src_tauri::ocr::recognize_blocks(&img, "en")
        .expect("OCR 应识别出文字（需本机装 tesseract + eng 语言包）");

    assert!(!blocks.is_empty(), "应至少识别出一个块");

    let upper = blocks
        .iter()
        .find(|b| b.text.to_uppercase().contains("HEL"))
        .unwrap_or_else(|| panic!("未识别到左上角文字，实际块: {blocks:?}"));
    assert!(
        upper.x < 0.4 && upper.y < 0.4,
        "左上角文字必须落在左上象限，实际 x={} y={}（坐标轴搞反或分母用错）",
        upper.x,
        upper.y
    );

    let lower = blocks
        .iter()
        .find(|b| b.text.to_uppercase().contains("WOR"))
        .unwrap_or_else(|| panic!("未识别到右下角文字，实际块: {blocks:?}"));
    assert!(
        lower.x > 0.5 && lower.y > 0.5,
        "右下角文字必须落在右下象限，实际 x={} y={}（y 未翻转或分母用错）",
        lower.x,
        lower.y
    );

    // 相对位置必须正确：右下块同时位于左上块的右侧与下方。
    // 这一条能捕获「y 轴未翻转」——那会让两块的 y 关系倒置。
    assert!(
        lower.x > upper.x && lower.y > upper.y,
        "右下块必须同时位于左上块的右侧与下方（实际 upper=({},{}), lower=({},{})）",
        upper.x,
        upper.y,
        lower.x,
        lower.y
    );
}

/// 关键回归：**同一水平线上的多栏排版不得横向合并成单块**。
///
/// 若聚合只看垂直容差而忽略水平间距，两栏的首行会被捏成一行。
#[test]
fn test_recognize_blocks_splits_horizontal_columns() {
    // 1000x300 白底图，左侧 "HELLO" (x=40, y=40)，右侧 "WORLD" (x=600, y=40)
    let mut img = RgbaImage::from_pixel(1000, 300, Rgba([255, 255, 255, 255]));
    draw_text(&mut img, 40, 40, "HELLO", 5);
    draw_text(&mut img, 600, 40, "WORLD", 5);

    let blocks = src_tauri::ocr::recognize_blocks(&img, "en").expect("OCR 应识别出文字");

    assert_eq!(
        blocks.len(),
        2,
        "左右两栏应识别为 2 个独立块而非横向合并: {blocks:?}"
    );
    let left = blocks
        .iter()
        .find(|b| b.text.contains("HEL"))
        .expect("左栏块");
    let right = blocks
        .iter()
        .find(|b| b.text.contains("WOR"))
        .expect("右栏块");
    assert!(!left.text.contains("WOR"), "左栏不应包含右栏内容");
    assert!(!right.text.contains("HEL"), "右栏不应包含左栏内容");
    assert!(left.x < 0.3 && right.x > 0.5);
}

/// 关键回归：**裁剪后必须把块坐标映射回整图**。
///
/// 这是 [`src_tauri::overlay::recognize_in_region`] 的核心不变量。
/// 漏掉 `map_from_subrect` 时，所有块会挤在选区左上角——而只测
/// `recognize_blocks`（不裁剪）的用例**完全覆盖不到**这一段。
#[test]
fn test_recognize_in_region_maps_crop_coords_back_to_full_image() {
    use transly_core::NormRect;

    // 整图 1000x600，只在**右下角**区域画字。
    let mut img = RgbaImage::from_pixel(1000, 600, Rgba([255, 255, 255, 255]));
    draw_text(&mut img, 700, 480, "WORLD", 5);

    // 选区恰好框住那块字（归一化：x 0.65~0.95, y 0.75~0.95）。
    let region = NormRect::new(0.65, 0.75, 0.30, 0.20);

    let blocks =
        src_tauri::overlay::recognize_in_region(&img, region, "en").expect("选区内的文字应被识别");

    assert!(!blocks.is_empty(), "应识别出至少一个块");

    for b in &blocks {
        // 不做映射时，块的 x/y 会是「裁剪图内的相对值」（接近 0），
        // 于是这里必然失败——这正是本用例要钉住的行为。
        assert!(
            b.x >= 0.6 && b.y >= 0.7,
            "块必须被映射回整图右下角，实际 x={} y={}（漏了 map_from_subrect）",
            b.x,
            b.y
        );
        // 且必须落在选区范围内（允许 OCR 框略溢出边界）
        assert!(
            b.x <= region.x + region.w + 0.05 && b.y <= region.y + region.h + 0.05,
            "块不应大幅溢出选区，实际 x={} y={}",
            b.x,
            b.y
        );
    }
}
