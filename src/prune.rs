// Recall - 备份清理模块
// 提供查找和删除旧备份的功能，帮助管理磁盘空间

use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use console::style;
use std::fs;
use std::path::{Path, PathBuf};

/// 查找目标目录中所有有效的备份文件夹
///
/// 有效备份文件夹的定义：
/// - 是目录
/// - 名称不以 `.partial` 结尾（未完成的备份）
/// - 名称不是 `current`（符号链接）
/// - 名称格式为 `%Y-%m-%d_%H-%M-%S`（时间戳格式）
///
/// # 参数
/// * `destination` - 备份目标根目录
///
/// # 返回
/// * `Ok(Vec<PathBuf>)` - 按时间顺序排列的备份路径（最旧的在前）
/// * `Err(anyhow::Error)` - 读取目录失败
pub fn find_all_backups(destination: &Path) -> Result<Vec<PathBuf>> {
    if !destination.exists() {
        return Ok(Vec::new());
    }

    let mut backups: Vec<PathBuf> = fs::read_dir(destination)
        .context("Cannot read destination directory")?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && !path.to_string_lossy().ends_with(".partial")
                && path.file_name().is_some_and(|n| n != "current")
                && is_valid_backup_folder_name(path)
        })
        .collect();

    // 按名称排序，最旧的在前（名称本身就是时间戳）
    backups.sort();

    Ok(backups)
}

/// 检查目录名称是否是有效的备份文件夹名称
///
/// 有效格式：`YYYY-MM-DD_HH-MM-SS`（如 `2024-01-15_14-30-00`）
///
/// # 参数
/// * `path` - 要检查的路径
///
/// # 返回
/// * `true` - 是有效的备份文件夹名称
/// * `false` - 不是有效的备份文件夹名称
fn is_valid_backup_folder_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%d_%H-%M-%S").is_ok())
        .unwrap_or(false)
}

/// 清理旧备份，保留最新的 `keep` 个备份
///
/// # 参数
/// * `destination` - 备份目标根目录
/// * `keep` - 要保留的最新备份数量
/// * `dry_run` - 是否为试运行模式（不实际删除）
///
/// # 返回
/// * `Ok(())` - 清理完成
/// * `Err(anyhow::Error)` - 清理过程中出现错误
///
/// # 示例
/// ```ignore
/// // 保留最新的 5 个备份
/// prune_backups(Path::new("D:/Backups/MyProject"), 5, false)?;
/// ```
pub fn prune_backups(destination: &Path, keep: usize, dry_run: bool) -> Result<()> {
    let backups = find_all_backups(destination)?;

    // 如果备份数量不超过保留数量，无需清理
    if backups.len() <= keep {
        println!(
            "Found {} backup(s), keeping {}. Nothing to prune.",
            backups.len(),
            keep
        );
        return Ok(());
    }

    let to_delete_count = backups.len() - keep;
    let to_delete = &backups[..to_delete_count];

    println!(
        "Found {} backup(s). Will delete {} oldest, keeping {} newest.",
        backups.len(),
        to_delete_count,
        keep
    );

    // 删除旧的备份
    for path in to_delete {
        if dry_run {
            println!(
                "{} Would delete: {:?}",
                style("Dry run:").yellow(),
                path.file_name().unwrap()
            );
        } else {
            println!("Deleting: {:?}", style(path.file_name().unwrap()).red());
            fs::remove_dir_all(path)
                .with_context(|| format!("Failed to delete backup {:?}", path))?;
        }
    }

    if !dry_run {
        println!(
            "{}",
            style(format!("Pruned {} old backup(s).", to_delete_count))
                .green()
                .bold()
        );
    }

    Ok(())
}
