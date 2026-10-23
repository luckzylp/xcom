//! Internationalisation: Simplified Chinese and English.
//!
//! # Why Chinese is the key
//!
//! The translation key is the original Simplified-Chinese string.  That keeps
//! the change mechanical (each literal is wrapped, never rewritten), keeps the
//! diff reviewable, and needs no separate id space.  It is the classic gettext
//! arrangement, and the obvious cost — a missing translation degrades to the
//! Chinese text rather than to an empty label — is the right trade for a tool
//! of this size.
//!
//! # Why the language is global
//!
//! [`tr`] reads a process-wide setting instead of taking a receiver, so a call
//! site is `tr("打开串口")` rather than `app.t("打开串口")`.  That matters more
//! than it looks: the short form can be applied mechanically and consistently
//! across hundreds of call sites, including inside closures and free functions
//! that have no access to the application struct.  The UI is single-window and
//! single-threaded, so there is nothing to race with.
//!
//! A missing translation is a defect, so [`tests::every_key_has_a_translation`]
//! scans the source for `tr(…)` literals and fails if any is absent from the
//! table.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// A language the interface can be shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    /// Simplified Chinese.
    #[default]
    ZhCn,
    /// English.
    EnUs,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::ZhCn, Lang::EnUs];

    /// Name of the language, always written in that language so it is legible
    /// to someone who cannot read the current one.
    pub fn label(self) -> &'static str {
        match self {
            Lang::ZhCn => "简体中文",
            Lang::EnUs => "English",
        }
    }
}

/// The active language, as an index into [`Lang::ALL`].
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The language currently in use.
pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::EnUs,
        _ => Lang::ZhCn,
    }
}

/// Switch the active language.
pub fn set_lang(lang: Lang) {
    let index = match lang {
        Lang::ZhCn => 0,
        Lang::EnUs => 1,
    };
    CURRENT.store(index, Ordering::Relaxed);
}

/// Translate a source string.
///
/// Chinese is returned as-is; English is looked up, falling back to the source
/// string so a missing entry is visible rather than blank.
pub fn tr(zh: &'static str) -> &'static str {
    match lang() {
        Lang::ZhCn => zh,
        Lang::EnUs => table().get(zh).copied().unwrap_or(zh),
    }
}

/// Translate and substitute the first `{}`.
pub fn tr1(zh: &'static str, a: impl std::fmt::Display) -> String {
    tr(zh).replacen("{}", &a.to_string(), 1)
}

/// Translate and substitute two `{}` placeholders, left to right.
pub fn tr2(zh: &'static str, a: impl std::fmt::Display, b: impl std::fmt::Display) -> String {
    tr1(zh, a).replacen("{}", &b.to_string(), 1)
}

/// Translate and substitute three `{}` placeholders, left to right.
pub fn tr3(
    zh: &'static str,
    a: impl std::fmt::Display,
    b: impl std::fmt::Display,
    c: impl std::fmt::Display,
) -> String {
    tr2(zh, a, b).replacen("{}", &c.to_string(), 1)
}

/// Translate and substitute an arbitrary number of `{}` placeholders.
///
/// The typed [`tr1`]/[`tr2`]/[`tr3`] helpers cover almost every message; this
/// exists for the few that interpolate pre-formatted fields (hex bytes with
/// field widths, for instance) and would otherwise need a `tr5`.
pub fn tr_fmt(zh: &'static str, args: &[String]) -> String {
    let mut out = tr(zh).to_string();
    for arg in args {
        out = out.replacen("{}", arg, 1);
    }
    out
}

/// Serialises tests that change the process-wide language.
///
/// Exposed to the rest of the crate under `cfg(test)` so a UI test that renders
/// in English cannot flip the language underneath a translation test that is
/// asserting Chinese.
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// The window title in the active language.
///
/// Both the startup path and the runtime language switch call this, so the two
/// cannot drift apart.
pub fn window_title() -> String {
    if lang() == Lang::ZhCn && !title_can_render_chinese() {
        // Better an English title than a row of tofu boxes.
        return table()
            .get("XCOM 串口调试助手")
            .copied()
            .unwrap_or("XCOM")
            .to_owned();
    }
    tr("XCOM 串口调试助手").to_owned()
}

/// Whether the font that draws the title bar can render Chinese.
///
/// On Linux the Wayland client-side decoration renders the title with a single
/// font chosen by fontconfig and has no per-glyph fallback, so a Chinese title
/// on a Latin-only font shows as boxes.  `sctk-adwaita` asks GNOME for
/// `titlebar-font` and then fontconfig for that family; we follow the same
/// path and inspect the charset fontconfig reports.
///
/// Unknown environments report `true`, so the ordinary case is unchanged and
/// only a *confirmed* lack of coverage downgrades the title.
pub fn title_can_render_chinese() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| probe_title_font_covers_cjk().unwrap_or(true))
}

#[cfg(target_os = "linux")]
fn probe_title_font_covers_cjk() -> Option<bool> {
    use std::process::Command;

    let configured = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.wm.preferences", "titlebar-font"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|text| text.trim().trim_matches('\'').to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "sans-serif".to_owned());

    let charset = Command::new("fc-match")
        .args(["-f", "%{charset}", &configured])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())?;

    if charset.trim().is_empty() {
        return None;
    }
    // 中 — if the title font cannot draw this, it cannot draw any of them.
    Some(charset_covers(&charset, 0x4E2D))
}

#[cfg(not(target_os = "linux"))]
fn probe_title_font_covers_cjk() -> Option<bool> {
    // Windows and macOS draw the title themselves with full font fallback.
    Some(true)
}

/// Whether a fontconfig charset list (`20-7f a0-127 12b …`) includes `codepoint`.
///
/// Only the Linux title-bar probe calls this in production, but it is pure
/// parsing and is unit-tested on every platform, so it stays compiled
/// everywhere; only the unused-on-Windows/macOS warning is suppressed.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn charset_covers(charset: &str, codepoint: u32) -> bool {
    charset
        .split_whitespace()
        .any(|token| match token.split_once('-') {
            Some((low, high)) => {
                match (u32::from_str_radix(low, 16), u32::from_str_radix(high, 16)) {
                    (Ok(low), Ok(high)) => low <= codepoint && codepoint <= high,
                    _ => false,
                }
            }
            None => u32::from_str_radix(token, 16) == Ok(codepoint),
        })
}

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| EN.iter().copied().collect())
}

/// Classify a locale string such as `zh_CN.UTF-8`.
///
/// Split out from [`system_lang`] so it can be tested without touching the
/// process environment.
pub fn classify(locale: Option<&str>) -> Lang {
    match locale {
        Some(value) if value.trim().to_ascii_lowercase().starts_with("zh") => Lang::ZhCn,
        _ => Lang::EnUs,
    }
}

/// Guess the language from the environment.
pub fn system_lang() -> Lang {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key)
            && !value.trim().is_empty()
        {
            return classify(Some(&value));
        }
    }
    Lang::EnUs
}

/// English translations, keyed by the Simplified-Chinese source string.
///
/// Entries are grouped by the module they come from.  Order within the list is
/// irrelevant: [`table`] builds a map.
#[rustfmt::skip]
static EN: &[(&str, &str)] = &[
    // -- top bar ------------------------------------------------------------
    ("主题",                 "Theme"),
    ("语言",                 "Language"),
    ("显示配置面板",          "Show configuration panel"),
    ("显示工具面板",          "Show tool panel"),
    ("显示发送区",            "Show send area"),
    ("展开配置面板",          "Expand configuration panel"),
    ("展开工具面板",          "Expand tool panel"),
    ("展开发送区",            "Expand send area"),
    ("自定义接收区配色",       "Custom receive colours"),
    ("字色",                 "Text"),
    ("底色",                 "Background"),
    ("关于",                 "About"),
    ("恢复默认设置",          "Restore defaults"),
    ("已恢复默认设置",         "Defaults restored"),
    ("关于 XCOM",            "About XCOM"),

    // -- theme names (src/theme.rs) ----------------------------------------
    ("黑色主题",              "Dark theme"),
    ("白色主题",              "Light theme"),
    ("跟随系统",              "Follow system"),

    // -- serial settings ----------------------------------------------------
    ("串口设置",              "Serial port"),
    ("串口",                  "Port"),
    ("（无可用串口）",         "(no ports found)"),
    ("未检测到串口",           "No serial ports detected"),
    ("刷新",                  "Refresh"),
    ("已刷新串口列表",         "Port list refreshed"),
    ("波特率",                "Baud rate"),
    ("（自定义）",            "(custom)"),
    ("自定义波特率",           "Custom baud rate"),
    ("已保存的自定义波特率",   "Saved custom baud rates"),
    ("暂无自定义波特率",       "No custom baud rates yet"),
    ("管理自定义波特率",       "Manage custom baud rates"),
    ("自定义波特率列表已满",   "The custom baud rate list is full"),
    ("该波特率已存在或为标准值", "This baud rate already exists or is standard"),
    ("添加波特率",             "Add a baud rate"),
    ("关闭",                  "Close"),
    ("删除",                  "Delete"),
    ("添加",                 "Add"),
    ("数据位",                "Data bits"),
    ("停止位",                "Stop bits"),
    ("校验位",                "Parity"),
    ("流控",                  "Flow control"),
    ("DTR / RTS",            "DTR / RTS"),
    ("打开串口",              "Open port"),
    ("关闭串口",              "Close port"),
    ("当前: {}",              "Current: {}"),
    ("产品: {}",              "Product: {}"),
    ("厂商: {}",              "Manufacturer: {}"),
    ("PID/VID: {}",            "PID/VID: {}"),
    ("SN: {}",                "SN: {}"),
    ("提示: 串口打开时无法修改通信参数，需先关闭串口",
                              "Note: close the port before changing its parameters"),

    // -- panels ------------------------------------------------------------
    ("显示设置", "Display"),
    ("16进制显示", "Hex display"),
    ("HEX 样式", "Hex style"),
    ("连续", "Continuous"),
    ("16 字节/行", "16 bytes/row"),
    ("大写", "Upper case"),
    ("每行字节", "Bytes per row"),
    ("时间戳", "Timestamp"),
    ("方向标记", "Direction markers"),
    ("接收的数据以 -> 开头，发送的数据以 <- 开头",
     "Received rows start with ->, sent rows with <-"),
    ("自动换行", "Wrap long lines"),
    ("自动滚动到最新", "Auto-scroll to newest"),
    ("行距", "Line spacing"),
    ("字体设置", "Fonts"),
    ("界面字体", "Interface font"),
    ("界面字号", "Interface font size"),
    ("接收区字体", "Receive font"),
    ("接收区字号", "Receive font size"),
    ("发送区字体", "Send font"),
    ("发送区字号", "Send font size"),
    ("自动", "Automatic"),
    ("清除接收", "Clear receive"),
    ("保存接收数据", "Save receive data"),
    ("保存原始数据", "Save raw data"),
    ("编码设置", "Encoding"),
    ("接收编码", "Receive encoding"),
    ("自动识别", "Auto-detect"),
    ("发送编码", "Send encoding"),
    ("跟随接收编码", "Follow receive"),
    ("自动识别会持续观察数据流，锁定后不再频繁切换", "Detection watches the stream and stops switching once locked"),
    ("统计", "Statistics"),
    ("发送字节", "Bytes sent"),
    ("接收字节", "Bytes received"),
    ("显示行数", "Displayed rows"),
    ("缓存字节", "Buffered bytes"),
    ("未显示字节", "Undisplayed bytes"),
    ("最大保留行数", "Max rows"),
    ("最大缓存 (MB)", "Max buffer (MB)"),
    ("继续显示", "Resume"),
    ("暂停显示", "Pause"),
    ("显示已暂停，数据仍在后台接收", "Display paused; data is still being received"),
    ("复制选中", "Copy selection"),
    ("全选", "Select all"),
    ("已选中，Ctrl+C 复制", "Selected - Ctrl+C to copy"),
    ("已复制选中内容到剪贴板", "Selection copied to clipboard"),
    ("没有可复制的内容", "Nothing to copy"),
    ("发送区", "Send"),
    ("16进制发送", "Hex send"),
    ("发送新行", "Append newline"),
    ("定时发送", "Timed send"),
    ("定时间隔", "Timed interval"),
    ("清除发送", "Clear send"),
    ("已清除发送区与收发计数", "Send area and byte counters cleared"),
    ("发送文件", "Send file"),
    ("选择要发送的文件", "Select a file to send"),
    ("终止发送", "Abort"),
    ("发送 (Ctrl+Enter)", "Send (Ctrl+Enter)"),
    ("历史", "History"),
    ("暂无历史", "No history"),
    ("输入十六进制，如 01 02 0A（可省略空格）", "Hex bytes, e.g. 01 02 0A (spaces optional)"),
    ("输入要发送的内容", "Text to send"),
    ("多条发送", "Multi-send"),
    ("协议传输", "Protocol"),
    ("首页", "First"),
    ("上一页", "Prev"),
    ("下一页", "Next"),
    ("尾页", "Last"),
    ("导入条目", "Import"),
    ("导入发送条目", "Import entries"),
    ("导出条目", "Export"),
    ("导出发送条目", "Export entries"),
    ("循环发送", "Loop sending"),
    ("关联数字键盘 (1-9,0)", "Bind number keys (1-9,0)"),
    ("新行 CR", "Newline CR"),
    ("新行 LF", "Newline LF"),
    ("启用", "On"),
    ("内容", "Command"),
    ("备注", "Note"),
    ("新行", "Newline"),
    ("指令", "command"),
    ("发送", "Send"),
    ("地址", "Address"),
    ("功能", "Function"),
    ("校验", "Checksum"),
    ("帧周期", "Frame period"),
    ("最大重发", "Max retries"),
    ("最大数据长度", "Max data length"),
    ("自动发送", "Auto send"),
    ("显示原始帧", "Show raw frames"),
    ("发送数据", "Payload"),
    ("十六进制，如 00 00 00 0A", "Hex bytes, e.g. 00 00 00 0A"),
    ("单次发送", "Send once"),
    ("文件发送", "Send file"),
    ("选择要传输的文件", "Select a file to transfer"),
    ("停止", "Stop"),
    ("用户已停止传输", "Transfer stopped by user"),
    ("响应区", "Responses"),
    ("清除", "Clear"),
    ("暂无协议交互记录", "No protocol traffic yet"),
    ("请先输入要发送的数据", "Enter the payload first"),
    ("未连接", "Disconnected"),
    ("XCOM 串口调试助手", "XCOM Serial Assistant"),
    ("使用 Rust + egui 开发的跨平台串口调试工具，可在 Windows / Linux / macOS 上运行。", "A cross-platform serial debugging tool built with Rust + egui, running on Windows / Linux / macOS."),
    ("功能：", "Features:"),
    ("• 串口参数全面可配（自定义波特率、1/1.5/2 停止位）", "• Every port parameter configurable (custom baud, 1/1.5/2 stop bits)"),
    ("• 十六进制 / 文本收发，时间戳断帧", "• Hex and text I/O with CR/LF-framed timestamps"),
    ("• 编码自动识别与手动指定", "• Automatic encoding detection with manual override"),
    ("• 单条 / 多条 / 协议传输三种发送方式", "• Single, multi and protocol send modes"),
    ("• 黑色与白色主题", "• Dark and white themes"),
    ("本软件为开源实现，界面不含任何广告。", "Open source, with no advertising in the interface."),

    ("以回车换行断帧；数据中若不含回车换行则不会显示时间戳",
     "Frames on CR/LF; without them no timestamp is shown"),

    ("设置 DTR 失败: {}", "Failed to set DTR: {}"),
    ("设置 RTS 失败: {}", "Failed to set RTS: {}"),
    ("自动识别（{}）",          "Auto-detect ({})"),
    ("当前接收: {}",          "Receiving as: {}"),
    ("已分析 {} 字节",        "{} bytes analysed"),
    ("{} · 剩余 {} 字节", "{} · {} bytes remaining"),
    ("第 {} / {} 页",    "Page {} / {}"),
    ("已导入 {} 个条目",      "Imported {} entries"),
    ("读取文件失败: {}",      "Failed to read file: {}"),
    ("已导出到 {}",            "Exported to {}"),
    ("写入失败: {}",          "Write failed: {}"),
    ("正在{}",                  "{} in progress"),
    ("{} 条",                  "{} entries"),
    ("数据格式错误: {}",      "Invalid data: {}"),
    ("已启动{}",                "{} started"),
    ("已启动文件传输: {}",    "File transfer started: {}"),
    ("编码 {}",                "Encoding {}"),
    ("发送: {}  接收: {}", "Sent: {}  Received: {}"),
    ("已暂停（缓存 {} 字节）", "Paused ({} bytes buffered)"),
    ("接收中: {}",            "Receiving: {}"),
    ("版本 {}",                "Version {}"),
    ("已保存 {} 字节到 {}", "Saved {} bytes to {}"),
    ("保存失败: {}",          "Save failed: {}"),
    ("已保存到 {}",            "Saved to {}"),

    ("文件为空", "File is empty"),
    ("已被新的发送任务中断", "Interrupted by a new send task"),
    ("设备无响应，已停止发送", "Device did not respond; sending stopped"),
    ("帧周期 {} ms 小于响应断帧时间 {} ms，建议至少 {} ms，否则会提前重发", "Frame period {} ms is below the {} ms response window; use at least {} ms or it will retransmit early"),
    ("GBK (简体中文)", "GBK (Simplified Chinese)"),
    ("GB18030 (简体中文)", "GB18030 (Simplified Chinese)"),
    ("Big5 (繁体中文)", "Big5 (Traditional Chinese)"),
    ("Shift_JIS (日文)", "Shift_JIS (Japanese)"),
    ("EUC-JP (日文)", "EUC-JP (Japanese)"),
    ("EUC-KR (韩文)", "EUC-KR (Korean)"),
    ("Windows-1252 (西欧)", "Windows-1252 (Western)"),
    ("Windows-1251 (西里尔)", "Windows-1251 (Cyrillic)"),
    ("ISO-8859-2 (中欧)", "ISO-8859-2 (Central European)"),
    ("ISO-8859-5 (西里尔)", "ISO-8859-5 (Cyrillic)"),
    ("KOI8-R (俄文)", "KOI8-R (Russian)"),

    ("自动·{}", "Auto · {}"),
    ("未找到中文字体，界面中文可能显示为方块。请安装 Noto Sans CJK / 文泉驿等字体", "No CJK font found; Chinese text may render as boxes. Install Noto Sans CJK or WenQuanYi."),
    ("请先选择串口", "Select a serial port first"),
    ("已打开 {} [{}]", "Opened {} [{}]"),
    ("串口已关闭", "Port closed"),
    ("已停止{}", "{} stopped"),
    ("串口未打开", "Port is not open"),
    ("发送内容为空", "Nothing to send"),
    ("串口未打开，无法{}", "Port is not open; cannot {}"),
    ("文件为空，未发送", "File is empty; nothing sent"),
    ("开始发送文件 {}（{} 字节）", "Sending {} ({} bytes)"),
    ("已终止文件发送（{}，剩余 {} 字节）", "File send aborted ({}; {} bytes remaining)"),
    ("已清除接收区与收发计数", "Receive area and byte counters cleared"),
    ("保存设置失败: {}", "Failed to save settings: {}"),
    ("{} 已断开", "{} disconnected"),
    ("{}（串口可能已被拔出）", "{} (the port may have been unplugged)"),
    ("已恢复显示，补入 {} 字节", "Display resumed; {} bytes appended"),
    ("串口已关闭，文件发送中止", "Port closed; file send aborted"),
    ("文件发送完成（{} 字节）", "File sent ({} bytes)"),
    ("→ 帧 #{}（第 {} 次）: {}", "-> frame #{} (attempt {}): {}"),
    ("发送协议帧", "send a protocol frame"),
    ("✓ 收到应答 #{}（第 {} 次尝试）", "ACK #{} received (attempt {})"),
    ("✗ 帧 #{} 设备无响应，已停止发送", "No response for frame #{}; sending stopped"),
    ("← 无效响应 [{}]：{}", "<- invalid response [{}]: {}"),
    ("文件传输完成，共 {} 字节", "File transfer complete, {} bytes"),
    ("地址 {} 功能 {} 序列 {} 数据[{}] {}", "addr {} func {} seq {} data[{}] {}"),
    ("当前页没有勾选的发送条目，已停止循环发送", "No entries selected on this page; looping stopped"),
    ("条目 {} 为空", "Entry {} is empty"),
    ("条目 {} 编码失败: {}", "Entry {} failed to encode: {}"),
    ("发送条目 {}", "send entry {}"),

    ("位置 {} 处的字符 {} 不是合法的十六进制字符", "Character {} at position {} is not a hex digit"),
    ("位置 {} 处剩余半个字节 {}，十六进制字符个数必须为偶数", "Dangling nibble {} at position {}; hex digits must come in pairs"),
    ("帧长度 {} 小于最小长度 {}", "Frame length {} is below the minimum of {}"),
    ("帧长度 {} 超过最大长度 {}", "Frame length {} exceeds the maximum of {}"),
    ("数据长度字段为 {}，但只剩 {} 字节", "Length field says {} bytes but only {} remain"),
    ("校验字段长度 {} 非法（应为 1 或 2）", "Checksum field is {} bytes; must be 1 or 2"),
    ("校验字段长度 {} 与所选校验方式 ({}) 不符", "Checksum field is {} bytes, but the selected method uses {}"),
    ("校验错误：期望 {}，实际 {}", "Checksum mismatch: expected {}, got {}"),
    ("无法打开 {}: {}", "Cannot open {}: {}"),
    ("无法复制串口句柄: {}", "Cannot duplicate the port handle: {}"),
    ("发送队列已满，请降低发送速率", "Send queue is full; slow down"),
    ("串口读取失败: {}", "Port read failed: {}"),
    ("发送失败: {}", "Send failed: {}"),
    ("无法确定配置目录", "Cannot determine the config directory"),
    ("创建配置目录失败: {}", "Failed to create the config directory: {}"),
    ("序列化失败: {}", "Serialisation failed: {}"),
    ("替换失败: {}", "Replace failed: {}"),
    ("解析失败: {}", "Parse failed: {}"),
    ("文件中没有条目", "The file contains no entries"),
    ("当前平台/驱动不支持 1.5 位停止位，已按 2 位停止位处理", "This platform/driver does not support 1.5 stop bits; using 2 instead"),
    ("本系统 (Linux) 仅在数据位为 5 时支持 1.5 位停止位，已按 2 位停止位处理", "On Linux, 1.5 stop bits require 5 data bits; using 2 stop bits instead"),

    ("蓝牙", "Bluetooth"),
    ("PCI", "PCI"),

    ("无限循环", "Loop forever"),
    ("循环次数", "Loops"),
    ("不限次数", "no limit"),
    ("已完成 {} / {} 次", "{} / {} passes done"),
    ("循环发送已完成 {} 次", "Loop sending finished after {} passes"),
    ("循环", "Loop"),
    ("未循环", "Idle"),
    ("循环中", "Looping"),
    ("当前条目", "Entry"),
    ("空闲", "Idle"),

    ("发送已勾选", "Send checked"),
    ("按顺序发送整个发送池里所有已勾选且非空的条目",
     "Send every checked, non-empty entry in the pool, in order"),
    ("正在发送 #{} ({} / {})", "Sending #{} ({} / {})"),
    ("没有已勾选的发送条目", "No checked entries"),
    ("已发送 {} 条已勾选条目", "Sent {} checked entries"),

    ("串口已关闭，多条发送已停止", "Port closed; multi-send stopped"),

    ("延迟", "Delay"),

    ("已停止循环发送", "Loop sending stopped"),

    ("回显发送", "Echo sent"),
    ("把发出的内容显示在接收区，便于确认是否真的发出",
     "Show what you send in the receive view, so it is clear it went out"),

    // -- parity / flow control (src/serial/config.rs) -----------------------
    ("无",                    "None"),
    ("奇校验",                "Odd"),
    ("偶校验",                "Even"),
    ("软件 (XON/XOFF)",       "Software (XON/XOFF)"),
    ("{} / {} 字节",          "{} / {} bytes"),
    ("硬件 (RTS/CTS)",        "Hardware (RTS/CTS)"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use super::test_lock as LANG_LOCK;

    struct Restore;

    impl Drop for Restore {
        fn drop(&mut self) {
            set_lang(Lang::ZhCn);
        }
    }

    #[test]
    fn table_has_no_duplicates_or_empty_entries() {
        let mut seen = std::collections::HashSet::new();
        for (key, value) in EN {
            assert!(!key.trim().is_empty(), "empty key");
            assert!(!value.trim().is_empty(), "empty translation for {key:?}");
            assert!(seen.insert(*key), "duplicate key {key:?}");
        }
        // The map must actually contain everything the list declares.
        assert_eq!(table().len(), EN.len());
    }

    #[test]
    fn every_translation_is_actually_in_english() {
        // A translation that is identical to the key is almost always a
        // forgotten entry rather than a deliberate loanword.
        for (key, value) in EN {
            if key.is_ascii() {
                continue; // e.g. "DTR / RTS" is the same in both languages.
            }
            assert_ne!(key, value, "{key:?} is untranslated");
        }
    }

    #[test]
    fn chinese_passes_through_untouched() {
        let _guard = LANG_LOCK();
        let _restore = Restore;
        set_lang(Lang::ZhCn);
        assert_eq!(tr("打开串口"), "打开串口");
        assert_eq!(tr1("当前: {}", 9600), "当前: 9600");
    }

    #[test]
    fn english_is_looked_up() {
        let _guard = LANG_LOCK();
        let _restore = Restore;
        set_lang(Lang::EnUs);
        assert_eq!(tr("打开串口"), "Open port");
        assert_eq!(tr("关闭串口"), "Close port");
        assert_ne!(tr("打开串口"), tr("关闭串口"));
    }

    #[test]
    fn unknown_keys_fall_back_to_the_source() {
        let _guard = LANG_LOCK();
        let _restore = Restore;
        set_lang(Lang::EnUs);
        assert_eq!(tr("这个键故意不存在"), "这个键故意不存在");
    }

    #[test]
    fn placeholders_are_substituted_left_to_right() {
        let _guard = LANG_LOCK();
        let _restore = Restore;
        set_lang(Lang::EnUs);
        assert_eq!(tr1("当前: {}", "115200"), "Current: 115200");
        assert_eq!(tr2("{} / {}", 1, 4), "1 / 4");
        assert_eq!(tr3("{} {} {}", "a", "b", "c"), "a b c");
        // A key without a placeholder ignores the argument.
        assert_eq!(tr1("刷新", 1), "Refresh");
    }

    #[test]
    fn locale_classification() {
        assert_eq!(classify(Some("zh_CN.UTF-8")), Lang::ZhCn);
        assert_eq!(classify(Some("zh")), Lang::ZhCn);
        assert_eq!(classify(Some("ZH_TW")), Lang::ZhCn);
        assert_eq!(classify(Some("en_US.UTF-8")), Lang::EnUs);
        assert_eq!(classify(Some("de_DE")), Lang::EnUs);
        assert_eq!(classify(Some("")), Lang::EnUs);
        assert_eq!(classify(None), Lang::EnUs);
    }

    #[test]
    fn the_window_title_follows_the_language() {
        let _guard = test_lock();
        let _restore = Restore;

        // Coverage depends on the host's installed fonts, so compare against
        // the same decision the code makes rather than a fixed string.
        set_lang(Lang::ZhCn);
        let expected = if title_can_render_chinese() {
            "XCOM 串口调试助手"
        } else {
            "XCOM Serial Assistant"
        };
        assert_eq!(window_title(), expected);

        set_lang(Lang::EnUs);
        assert_eq!(window_title(), "XCOM Serial Assistant");
    }

    #[test]
    fn charset_parsing_handles_ranges_lists_and_junk() {
        // Real fontconfig output shape.
        assert!(charset_covers("20-7f a0-127 4e00-9fff", 0x4E2D));
        assert!(!charset_covers("20-7f a0-127", 0x4E2D));
        // A bare codepoint rather than a range.
        assert!(charset_covers("20-7f 4e2d", 0x4E2D));
        assert!(!charset_covers("20-7f 4e2e", 0x4E2D));
        // Range boundaries are inclusive.
        assert!(charset_covers("4e2d-4e2d", 0x4E2D));
        assert!(!charset_covers("", 0x4E2D));
        assert!(!charset_covers("not-a-charset", 0x4E2D));
    }

    #[test]
    fn language_labels_are_in_their_own_language() {
        assert_eq!(Lang::ZhCn.label(), "简体中文");
        assert_eq!(Lang::EnUs.label(), "English");
    }

    /// Collect every `src/**/*.rs` path, excluding this module.
    fn source_files() -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name().is_some_and(|n| n != "i18n.rs")
                {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
            &mut out,
        );
        out.sort();
        out
    }

    /// Every string literal in `source` that contains a CJK character, paired
    /// with whether a `tr(…)` call directly precedes it.
    ///
    /// Line breaks are flattened first so that a call whose literal sits on the
    /// next line is still recognised.
    fn cjk_literals_with_wrapping(source: &str) -> Vec<(String, bool)> {
        let flat: String = source
            .lines()
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join(" ");
        let bytes = flat.as_bytes();
        let mut out = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'"' {
                i += 1;
                continue;
            }
            // Consume one string literal.
            let start = i;
            let mut j = i + 1;
            let mut value = String::new();
            while j < bytes.len() {
                match bytes[j] {
                    b'"' => break,
                    b'\\' if j + 1 < bytes.len() => {
                        value.push(bytes[j + 1] as char);
                        j += 2;
                    }
                    _ => {
                        let ch = flat[j..].chars().next().expect("valid utf-8 boundary");
                        value.push(ch);
                        j += ch.len_utf8();
                    }
                }
            }
            if value
                .chars()
                .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
            {
                let before = flat[..start].trim_end();
                let wrapped = ["tr_fmt(", "tr1(", "tr2(", "tr3(", "tr("]
                    .iter()
                    .any(|call| before.ends_with(call));
                out.push((value, wrapped));
            }
            i = j + 1;
        }
        out
    }

    #[test]
    fn the_ui_layer_has_no_untranslated_literals() {
        // Guards the bug class that actually shipped: a bulk edit that replaced
        // only the *first* occurrence of each string, leaving duplicates in
        // other functions still hard-coded in Chinese.  Every CJK literal in
        // `src/ui/` must go through `tr`.
        let mut offenders = Vec::new();
        for path in source_files() {
            if !path.to_string_lossy().contains("/ui/") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            // Test modules hold sample data in Chinese on purpose.
            let source = source.split("#[cfg(test)]").next().unwrap_or(&source);
            for (literal, wrapped) in cjk_literals_with_wrapping(source) {
                if !wrapped {
                    offenders.push(format!("{}: {literal:?}", path.display()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "untranslated literals in the UI layer:\n{}",
            offenders.join("\n")
        );
    }

    #[test]
    fn the_ui_guard_can_tell_wrapped_from_unwrapped() {
        // Guards the guard, including the multi-line form: a literal on the
        // line after `tr(` must still count as wrapped.
        let sample =
            "ui.button(tr(\"打开\"));\nui.button(\"关闭\");\nlet a = tr1(\n  \"第 {}\", 1);";
        let found = cjk_literals_with_wrapping(sample);
        assert!(found.contains(&("打开".to_owned(), true)), "{found:?}");
        assert!(found.contains(&("关闭".to_owned(), false)), "{found:?}");
        assert!(found.contains(&("第 {}".to_owned(), true)), "{found:?}");
    }

    /// Extract the first string literal of every `tr(`/`tr1(`/`tr2(`/`tr3(` call.
    fn literals_in(source: &str) -> Vec<String> {
        let bytes = source.as_bytes();
        let mut found = Vec::new();
        for name in ["tr1(", "tr2(", "tr3(", "tr("] {
            let mut from = 0usize;
            while let Some(offset) = source[from..].find(name) {
                let start = from + offset;
                // Require a word boundary so `attr(` does not match `tr(`.
                let preceded_by_word = start > 0
                    && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
                from = start + name.len();
                if preceded_by_word {
                    continue;
                }
                let rest = &source[from..];
                let trimmed = rest.trim_start();
                let skipped = rest.len() - trimmed.len();
                if !trimmed.starts_with('"') {
                    continue; // a variable or a formatted argument, not a literal
                }
                // Decode the literal, honouring backslash escapes.
                let mut value = String::new();
                let mut chars = trimmed[1..].chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => value.push('\n'),
                            Some('t') => value.push('\t'),
                            Some('\\') => value.push('\\'),
                            Some('"') => value.push('"'),
                            Some(other) => value.push(other),
                            None => break,
                        },
                        other => value.push(other),
                    }
                }
                let _ = skipped;
                found.push(value);
            }
        }
        found
    }

    #[test]
    fn every_key_has_a_translation() {
        let known = table();
        let mut missing = Vec::new();
        let mut checked = 0usize;
        for path in source_files() {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            for literal in literals_in(&source) {
                // Only CJK strings are translation keys; ASCII ones (format
                // fragments, units) are passed through deliberately.
                if !literal
                    .chars()
                    .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
                {
                    continue;
                }
                checked += 1;
                if !known.contains_key(literal.as_str()) {
                    missing.push(format!("{}: {literal:?}", path.display()));
                }
            }
        }
        assert!(checked > 0, "the scanner found no call sites at all");
        assert!(
            missing.is_empty(),
            "missing translations:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn the_scanner_actually_finds_call_sites() {
        // Guards the guard: a broken scanner would make the test above vacuous.
        let sample = r#"ui.label(tr("刷新")); let a = tr1("当前: {}", x);"#;
        let mut found = literals_in(sample);
        found.sort();
        assert_eq!(found, vec!["刷新", "当前: {}"]);
        // Word boundaries: `attr(` must not be mistaken for `tr(`.
        assert!(literals_in(r#"foo.attr("x")"#).is_empty());
    }
}
