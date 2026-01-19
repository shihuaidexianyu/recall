// Recall - 源文件扫描模块
// 负责扫描源目录并决定每个文件需要进行何种同步操作

use crate::actions::{FileTask, SyncAction};
use crate::config::BackupConfig;
use crate::hasher::calculate_hash;
use crate::utils::{matches_exclude_pattern, to_verbatim_path};
use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use crossbeam_channel::Sender;
use glob::Pattern;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// 查找最新的备份目录
///
/// 在目标目录中查找最新的备份文件夹（按时间戳排序）。
///
/// # 参数
/// * `destination` - 备份目标根目录
///
/// # 返回
/// * `Ok(Some(PathBuf))` - 找到的最新备份路径
/// * `Ok(None)` - 没有找到有效备份
/// * `Err(anyhow::Error)` - 读取目录失败
pub fn find_latest_backup(destination: &Path) -> Result<Option<PathBuf>> {
    if !destination.exists() {
        return Ok(None);
    }

    let mut backups: Vec<PathBuf> = fs::read_dir(destination)
        .context("can not read the backup dir")?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && !path.to_string_lossy().ends_with(".partial")
                && path.file_name().is_some_and(|n| n != "current")
                && is_valid_backup_folder_name(path)
        })
        .collect();

    backups.sort();

    Ok(backups.last().cloned())
}

/// 检查目录名称是否是有效的备份文件夹名称
///
/// 有效格式：`YYYY-MM-DD_HH-MM-SS`
fn is_valid_backup_folder_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%d_%H-%M-%S").is_ok())
        .unwrap_or(false)
}

/// 扫描源目录并生成文件任务
///
/// 遍历源目录中的所有文件和目录，为每个条目创建文件任务，
/// 决定需要执行的操作（复制、硬链接等），并通过通道发送。
///
/// # 参数
/// * `config` - 备份配置
/// * `current_backup_dir` - 当前备份的目标目录
/// * `latest_backup` - 最新备份路径（用于增量备份）
/// * `tx` - 任务发送通道
///
/// # 返回
/// * `Ok(())` - 扫描完成
/// * `Err(anyhow::Error)` - 扫描失败
///
/// # 流程
/// 1. 编译排除模式
/// 2. 遍历源目录（跳过排除的文件）
/// 3. 为每个文件/目录创建任务并决定操作
/// 4. 通过通道发送任务
pub fn scan_source(
    config: &BackupConfig,
    current_backup_dir: &Path,
    latest_backup: Option<&Path>,
    tx: Sender<(FileTask, SyncAction)>,
) -> Result<()> {
    // 编译 Glob 模式以提高性能
    let compiled_patterns: Vec<Pattern> = config
        .exclude_patterns
        .iter()
        .filter_map(|s| match Pattern::new(s) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("Warning: Invalid glob pattern '{}': {}", s, e);
                None
            }
        })
        .collect();

    // 创建目录遍历器，不跟随符号链接
    let walker = WalkDir::new(&config.source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let path = e.path();
            if let Ok(rel) = path.strip_prefix(&config.source) {
                !matches_exclude_pattern(rel, &compiled_patterns)
            } else {
                true
            }
        });

    // 遍历所有条目
    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                eprintln!("警告: 无法访问 {:?}: {}", err.path(), err);
                continue;
            }
        };

        let path = entry.path();

        // 计算相对路径
        let rel_path = path
            .strip_prefix(&config.source)
            .context("无法计算相对路径")?
            .to_path_buf();

        // 转换为逐字路径（Windows 支持长路径）
        let src_path = to_verbatim_path(path);
        let dest_path = to_verbatim_path(&current_backup_dir.join(&rel_path));
        let old_path = latest_backup.map(|lb| to_verbatim_path(&lb.join(&rel_path)));

        // 创建文件任务并决定操作
        let task = FileTask::new(rel_path, src_path, dest_path, old_path);
        let action = decide_action(&task, config);

        // 通过通道发送任务
        if tx.send((task, action)).is_err() {
            break; // 接收端已关闭，停止扫描
        }
    }

    Ok(())
}

/// 决定对文件执行何种同步操作
///
/// 根据源文件和旧备份的状态比较，决定需要执行的操作。
///
/// # 决策逻辑
/// 1. **首次备份**：如果 `old_path` 为 `None`，直接复制新文件
/// 2. **文件不存在**：如果旧备份中不存在该文件，复制新文件
/// 3. **目录**：总是创建目录
/// 4. **符号链接**：重新创建符号链接
/// 5. **大小不同**：文件已修改，需要复制
/// 6. **权限不同**（Unix）：文件已修改，需要复制
/// 7. **修改时间**：
///    - 如果差异小于 1 秒，认为未修改
///    - 如果未修改且未启用内容检查，使用硬链接
/// 8. **内容检查**（启用时）：
///    - 计算源文件和旧文件的哈希值
///    - 哈希相同：使用硬链接
///    - 哈希不同：复制修改的文件
///
/// # 参数
/// * `task` - 文件任务
/// * `config` - 备份配置
///
/// # 返回
/// 需要执行的同步操作
pub fn decide_action(task: &FileTask, config: &BackupConfig) -> SyncAction {
    let old_path = match &task.old_path {
        Some(p) => p,
        None => {
            // 首次备份逻辑
            if task.src_path.is_dir() {
                 return SyncAction::CreateDir;
            }
            if let Ok(meta) = fs::symlink_metadata(&task.src_path) {
                if meta.is_symlink() {
                     if let Ok(target) = fs::read_link(&task.src_path) {
                         return SyncAction::MakeSymlink(target);
                     }
                }
            }
            return SyncAction::CopyNew
        },
    };

    // 如果旧备份中不存在该文件
    if !old_path.exists() {
        if task.src_path.is_dir() {
            return SyncAction::CreateDir;
        }
        if let Ok(meta) = fs::symlink_metadata(&task.src_path) {
            if meta.is_symlink() {
                if let Ok(target) = fs::read_link(&task.src_path) {
                    return SyncAction::MakeSymlink(target);
                }
            }
        }
        return SyncAction::CopyNew;
    }

    // 获取源文件元数据
    let src_meta = match fs::symlink_metadata(&task.src_path) {
        Ok(m) => m,
        Err(_) => return SyncAction::Skip,
    };

    // 处理目录
    if task.src_path.is_dir() {
        return SyncAction::CreateDir;
    }

    // 处理符号链接
    if src_meta.is_symlink() {
         if let Ok(target) = fs::read_link(&task.src_path) {
             // 检查旧路径是否也是指向相同目标的符号链接
             if let Ok(old_meta) = fs::symlink_metadata(old_path) {
                 if old_meta.is_symlink() {
                      if let Ok(old_target) = fs::read_link(old_path) {
                          if target == old_target {
                              return SyncAction::Link(old_path.clone());
                          }
                      }
                 }
             }
             return SyncAction::MakeSymlink(target);
         }
         return SyncAction::Skip; // 读取链接失败
    }

    // 获取旧文件元数据
    let old_meta = match fs::metadata(old_path) {
        Ok(m) => m,
        Err(_) => return SyncAction::CopyNew,
    };

    // 文件大小不同，已修改
    if src_meta.len() != old_meta.len() {
        return SyncAction::CopyModified;
    }

    // Unix: 检查权限
    #[cfg(unix)]
    {
        let src_mode = src_meta.permissions().mode();
        let old_mode = old_meta.permissions().mode();
        if src_mode != old_mode {
             return SyncAction::CopyModified;
        }
    }

    // 检查修改时间
    let src_mtime = src_meta.modified().ok();
    let old_mtime = old_meta.modified().ok();

    let mtime_match = match (src_mtime, old_mtime) {
        (Some(src), Some(old)) => {
            let diff = if src > old {
                src.duration_since(old).unwrap_or(Duration::ZERO)
            } else {
                old.duration_since(src).unwrap_or(Duration::ZERO)
            };
            diff.as_millis() < 1000 // 差异小于 1 秒认为相同
        }
        _ => false,
    };

    // 如果修改时间匹配且未启用内容检查，使用硬链接
    if mtime_match && !config.check_content {
        return SyncAction::Link(old_path.clone());
    }

    // 如果启用内容检查，比较哈希值
    if config.check_content {
        let src_hash = calculate_hash(&task.src_path);
        let old_hash = calculate_hash(old_path);

        match (src_hash, old_hash) {
            (Ok(s), Ok(o)) if s == o => {
                return SyncAction::Link(old_path.clone());
            }
            (Ok(_), Ok(_)) => {
                return SyncAction::CopyModified;
            }
            _ => {
                return SyncAction::CopyModified;
            }
        }
    }

    SyncAction::CopyModified
}
