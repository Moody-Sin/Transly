//! Linux OCR：外呼 `tesseract` 子进程，**全程管道、零落盘**。
//!
//! # 为什么不落临时文件
//!
//! `tesseract` 支持用 `-` 同时代表 stdin 与 stdout，于是整条链路
//! （PNG 进、TSV 出）都在管道里完成，不产生任何临时文件——这是
//! `README.md` 对外的「全流程内存零落盘」承诺的一部分。
//!
//! # ⚠️ `tsv` 必须是**最后一个**参数
//!
//! `tsv` 是**输出格式配置名**，必须放在所有选项之后：
//!
//! ```text
//! ✅ tesseract - - -l chi_sim tsv
//! ❌ tesseract - - tsv -l chi_sim     // tsv 被当成输入文件名
//! ```
//!
//! 写错时 tesseract **不报错**，而是把 `tsv` 当作输入路径、对空输入做默认识别，
//! 输出一堆乱码（实测「你好世界」→ `Prof`/`F-`/`MR`）。详见 `LESSONS.md` 第 23 条。
//!
//! # 为什么用子进程而不是纯 Rust OCR
//!
//! 纯 Rust 的 `ocrs` 只支持拉丁字母、**不支持中文**；而系统级 OCR 在 Linux 上
//! 不存在统一接口。tesseract 是唯一覆盖中日韩的可用方案。

use std::io::Write;
use std::process::{Command, Stdio};

use transly_core::{parse_tesseract_tsv, tesseract_lang_arg, OcrErrorDto, OcrErrorKind, PixelBox};

/// 安装指引：缺少 tesseract 时必须给用户可执行的命令，而不是无信息错误。
const INSTALL_HINT: &str =
    "请安装 tesseract 及其语言包：sudo apt-get install -y tesseract-ocr tesseract-ocr-chi-sim";

pub fn recognize_words(
    image: &image::RgbaImage,
    target_lang: &str,
) -> Result<Vec<PixelBox>, OcrErrorDto> {
    // 图像编码为 PNG 到内存。`write_to` 要求 `Write + Seek`，
    // 故必须包 `Cursor`（`Vec<u8>` 本身不满足）。
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| OcrErrorDto::internal(format!("OCR 输入图编码 PNG 失败: {e}")))?;

    // ⚠️ `tsv` 在最后。见模块文档与 LESSONS 第 23 条。
    let mut child = Command::new("tesseract")
        .args(["-", "-", "-l", tesseract_lang_arg(target_lang), "tsv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            // 未安装是最常见的失败；此时给出可执行的安装命令。
            if e.kind() == std::io::ErrorKind::NotFound {
                OcrErrorDto::tool_missing("tesseract", INSTALL_HINT)
            } else {
                OcrErrorDto::new(
                    OcrErrorKind::ToolMissing,
                    format!("无法启动 tesseract 子进程: {e}"),
                )
            }
        })?;

    // 必须先把 stdin 写完并 **drop**，再读 stdout。
    // 否则：PNG 可达数百 KB，撑满管道缓冲后子进程阻塞在写 stdout、
    // 我们阻塞在写 stdin —— 双方互等成死锁。
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| OcrErrorDto::internal("无法获取 tesseract stdin"))?;
        stdin
            .write_all(&png)
            .map_err(|e| OcrErrorDto::internal(format!("向 tesseract 写入图像失败: {e}")))?;
    } // stdin 在此 drop，子进程收到 EOF

    let output = child
        .wait_with_output()
        .map_err(|e| OcrErrorDto::internal(format!("等待 tesseract 退出失败: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        // 语言包缺失是最常见的失败（如 `Failed loading language 'chi_sim'`），
        // 此时应引导安装而不是报内部错误。
        if stderr.contains("Failed loading language") || stderr.contains("Could not initialize") {
            return Err(OcrErrorDto::no_language_pack());
        }
        return Err(OcrErrorDto::new(
            OcrErrorKind::ToolMissing,
            format!("tesseract 以退出码 {code} 失败: {}", stderr.trim()),
        ));
    }

    let tsv = String::from_utf8_lossy(&output.stdout);
    parse_tesseract_tsv(&tsv).map_err(|e| {
        // 解析失败在「图里没有文字」时很常见（tesseract 仍会输出表头，
        // 故 Empty/MissingHeader 更可能是真异常），给可诊断信息。
        OcrErrorDto::internal(format!("解析 tesseract TSV 失败: {e}"))
    })
}
