// Recall - 工具函数模块
// 提供路径处理、模式匹配、格式化等辅助功能

use glob::Pattern;
use std::path::{Path, PathBuf};

/// 将路径转换为 Windows 逐字路径格式
///
/// Windows 逐字路径（Verbatim Path）使用 `\\?\` 前缀，可以绕过 Windows API 的路径长度限制（MAX_PATH = 260 字符），
/// 并禁用路径解析，提供更可靠的路径处理。
///
/// # 参数
/// * `path` - 要转换的路径
///
/// # 返回
/// * 在 Windows 上：返回带有 `\\?\` 前缀的逐字路径
/// * 在其他平台上：返回原始路径
///
/// # 示例
/// ```
/// // Windows 上的转换
/// // C:\Users\Example -> \\?\C:\Users\Example
/// ```
#[cfg(windows)]
pub fn to_verbatim_path(path: &Path) -> PathBuf {
    let p = path.to_string_lossy();

    // 如果已经是逐字路径格式，直接返回
    if p.starts_with(r"\\?\") {
        return path.to_path_buf();
    }

    // 获取绝对路径
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    // 规范化路径分隔符，统一使用反斜杠
    let normalized = abs_path.to_string_lossy().replace("/", r"\");

    // 添加逐字路径前缀
    PathBuf::from(format!(r"\\?\{}", normalized))
}

/// 在非 Windows 平台上，逐字路径功能不需要
///
/// Linux/macOS 系统没有路径长度限制问题，因此直接返回原始路径
#[cfg(not(windows))]
pub fn to_verbatim_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// 移除逐字路径前缀
///
/// 将带有 `\\?\` 前缀的路径转换回普通路径格式，便于显示和处理。
///
/// # 参数
/// * `path` - 可能包含逐字前缀的路径
///
/// # 返回
/// 移除前缀后的路径，如果没有前缀则返回原路径
pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let p = path.to_string_lossy();
    if let Some(stripped) = p.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// 检查路径是否匹配任一排除模式
///
/// 使用 Glob 风格的模式匹配来判断文件路径是否应该被排除。
///
/// # 参数
/// * `rel_path` - 要检查的相对路径
/// * `patterns` - Glob 模式列表
///
/// # 返回
/// * `true` - 路径匹配至少一个排除模式
/// * `false` - 路径不匹配任何排除模式
///
/// # 示例
/// ```
/// // 模式 "*.log" 可以匹配 "file.log", "dir/file.log"
/// // 模式 "node_modules" 可以匹配任何目录名为 node_modules 的路径
/// ```
pub fn matches_exclude_pattern(rel_path: &Path, patterns: &[Pattern]) -> bool {
    let path_str = rel_path.to_string_lossy();

    for pattern in patterns {
        if pattern.matches(&path_str) {
            return true;
        }
    }

    false
}

/// 格式化字节数为人类可读的单位
///
/// 将字节数自动转换为 B、KB、MB、GB 或 TB 单位。
///
/// # 参数
/// * `bytes` - 要格式化的字节数
///
/// # 返回
/// 格式化后的字符串，保留两位小数
///
/// # 示例
/// ```
/// format_bytes(1024)      // "1.00 KB"
/// format_bytes(1048576)   // "1.00 MB"
/// format_bytes(500)       // "500 B"
/// ```
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 格式化秒数为人类可读的时间长度
///
/// 将秒数转换为 "Xh Ym Zs" 或 "Xm Ys" 或 "Xs" 格式。
///
/// # 参数
/// * `secs` - 要格式化的秒数
///
/// # 返回
/// 格式化后的时间字符串
///
/// # 示例
/// ```
/// format_duration(3661)  // "1h 1m 1s"
/// format_duration(125)   // "2m 5s"
/// format_duration(45)    // "45s"
/// ```
pub fn format_duration(secs: u64) -> String {
    if secs >= 3600 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;
        format!("{}h {}m {}s", hours, mins, secs)
    } else if secs >= 60 {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}
