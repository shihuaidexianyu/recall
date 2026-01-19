// Recall - 备份执行器模块
// 负责执行实际的文件操作（复制、硬链接、创建目录等）

use crate::actions::{BackupStats, FileTask, SyncAction};
use anyhow::{Context, Result};
use filetime::FileTime;
use crossbeam_channel::Receiver;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

/// 备份执行器
///
/// 负责执行文件同步操作，支持多线程并行处理。
pub struct BackupExecutor {
    /// 是否为试运行模式
    dry_run: bool,
}

impl BackupExecutor {
    /// 创建新的备份执行器
    ///
    /// # 参数
    /// * `dry_run` - 是否为试运行模式
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    /// 执行备份任务
    ///
    /// 从通道接收任务并使用线程池并行处理。
    /// 显示进度条并在完成后返回统计信息。
    ///
    /// # 参数
    /// * `rx` - 任务接收通道
    /// * `workers` - 工作线程数量
    ///
    /// # 返回
    /// * `Ok(BackupStats)` - 备份统计信息
    /// * `Err(anyhow::Error)` - 执行失败
    pub fn execute(
        &self,
        rx: Receiver<(FileTask, SyncAction)>,
        workers: usize,
    ) -> Result<BackupStats> {
        // 线程安全的统计信息
        let stats = Mutex::new(BackupStats::new());

        // 创建进度条样式
        let style = ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {pos} files processed ({eta}) {msg}")?
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");

        let pb = ProgressBar::new_spinner();
        pb.set_style(style);
        pb.set_message("Backup in progress...");

        let start_time = Instant::now();

        // 创建线程池
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .context("Failed to build thread pool")?;

        // 使用线程池并行处理任务
        pool.install(|| {
            rx.into_iter().par_bridge().for_each(|(task, action)| {
                let res = self.process_task(&task, &action);

                let mut s = stats.lock().unwrap();
                s.total_files += 1;

                // 根据操作类型和结果更新统计信息
                match res {
                    Ok(bytes) => match action {
                        SyncAction::CopyNew => {
                            s.copied_new += 1;
                            s.bytes_copied += bytes;
                        }
                        SyncAction::CopyModified => {
                            s.copied_modified += 1;
                            s.bytes_copied += bytes;
                        }
                        SyncAction::Link(_) => s.linked += 1,
                        SyncAction::MakeSymlink(_) => s.linked += 1,
                        SyncAction::CreateDir => s.total_files -= 1, // 目录不计入文件数
                        SyncAction::Skip => s.skipped += 1,
                    },
                    Err(e) => {
                        pb.println(format!("Failed: {:?} - {}", task.rel_path, e));
                        s.failed += 1;
                    }
                }
                pb.inc(1);
            });
        });

        pb.finish_with_message(format!(
            "Backup completed in {:.2}s",
            start_time.elapsed().as_secs_f64()
        ));

        Ok(stats.into_inner().unwrap())
    }

    /// 处理单个文件任务
    ///
    /// 根据同步动作类型执行相应的文件操作。
    ///
    /// # 参数
    /// * `task` - 文件任务
    /// * `action` - 要执行的同步动作
    ///
    /// # 返回
    /// * `Ok(u64)` - 复制的字节数（仅复制操作返回非零值）
    /// * `Err(anyhow::Error)` - 操作失败
    fn process_task(&self, task: &FileTask, action: &SyncAction) -> Result<u64> {
        // 试运行模式不执行实际操作
        if self.dry_run {
            return Ok(0);
        }

        match action {
            SyncAction::CopyNew | SyncAction::CopyModified => {
                // 复制文件
                if let Some(parent) = task.dest_path.parent() {
                     fs::create_dir_all(parent).with_context(|| {
                         format!("Failed to create parent dir for {:?}", task.dest_path)
                     })?;
                }
                let bytes = fs::copy(&task.src_path, &task.dest_path).with_context(|| {
                    format!("Failed to copy {:?} to {:?}", task.src_path, task.dest_path)
                })?;

                // 保留源文件的时间戳
                let src_meta = fs::metadata(&task.src_path)?;
                let mtime = FileTime::from_last_modification_time(&src_meta);
                let atime = FileTime::from_last_access_time(&src_meta);

                let dest_meta = fs::metadata(&task.dest_path)?;
                let mut perms = dest_meta.permissions();
                let original_readonly = perms.readonly();

                // 如果文件是只读的，需要先取消只读才能设置时间戳
                if original_readonly {
                    perms.set_readonly(false);
                    fs::set_permissions(&task.dest_path, perms.clone()).with_context(|| {
                        format!("Failed to unset readonly for {:?}", task.dest_path)
                    })?;
                }

                filetime::set_file_times(&task.dest_path, atime, mtime)
                    .with_context(|| format!("Failed to set time for {:?}", task.dest_path))?;

                if original_readonly {
                    perms.set_readonly(true);
                    fs::set_permissions(&task.dest_path, perms)?;
                }

                Ok(bytes)
            }
            SyncAction::Link(old_path) => {
                // 创建硬链接（节省空间）
                if let Some(parent) = task.dest_path.parent() {
                     fs::create_dir_all(parent)?;
                }
                fs::hard_link(old_path, &task.dest_path).with_context(|| {
                    format!("Failed to link {:?} to {:?}", old_path, task.dest_path)
                })?;
                Ok(0)
            }
            SyncAction::MakeSymlink(target) => {
                // 创建符号链接
                 if let Some(parent) = task.dest_path.parent() {
                     fs::create_dir_all(parent)?;
                 }
                 #[cfg(unix)]
                 std::os::unix::fs::symlink(target, &task.dest_path)
                    .with_context(|| format!("Failed to symlink {:?} -> {:?}", task.dest_path, target))?;

                 #[cfg(windows)]
                 {
                     // Windows 需要区分目录符号链接和文件符号链接
                     // 由于目标可能是相对路径或不存在的文件，我们检查源路径来判断
                     let is_dir = fs::metadata(&task.src_path).map(|m| m.is_dir()).unwrap_or(false);
                     if is_dir {
                         std::os::windows::fs::symlink_dir(target, &task.dest_path)
                            .with_context(|| format!("Failed to symlink_dir {:?} -> {:?}", task.dest_path, target))?;
                     } else {
                         std::os::windows::fs::symlink_file(target, &task.dest_path)
                            .with_context(|| format!("Failed to symlink_file {:?} -> {:?}", task.dest_path, target))?;
                     }
                 }
                 Ok(0)
            }
            SyncAction::CreateDir => {
                // 创建目录
                fs::create_dir_all(&task.dest_path).with_context(|| {
                    format!("Failed to create dir {:?}", task.dest_path)
                })?;
                Ok(0)
            }
            SyncAction::Skip => Ok(0),
        }
    }

    /// 提交备份（重命名临时目录并更新 current 符号链接）
    ///
    /// 备份过程中使用 `.partial` 后缀的临时目录，
    /// 完成后重命名为最终目录名，并更新 `current` 符号链接。
    ///
    /// # 参数
    /// * `temp_path` - 临时备份目录（带 .partial 后缀）
    /// * `final_path` - 最终备份目录名
    /// * `link_path` - current 符号链接路径
    ///
    /// # 返回
    /// * `Ok(())` - 提交成功
    /// * `Err(anyhow::Error)` - 提交失败
    pub fn commit_backup(temp_path: &Path, final_path: &Path, link_path: &Path) -> Result<()> {
        // 确保目标目录不存在
        if final_path.exists() {
            return Err(anyhow::anyhow!(
                "Destination backup folder already exists: {:?}",
                final_path
            ));
        }

        // 重命名临时目录为最终目录名
        fs::rename(temp_path, final_path)
            .with_context(|| format!("Failed to rename {:?} to {:?}", temp_path, final_path))?;

        // 删除旧的 current 链接
        if link_path.exists() || fs::symlink_metadata(link_path).is_ok() {
            match fs::symlink_metadata(link_path) {
                Ok(meta) => {
                    if meta.is_dir() {
                        fs::remove_dir(link_path).ok();
                    } else {
                        fs::remove_file(link_path).ok();
                    }
                }
                Err(_) => {
                    if link_path.is_dir() {
                        fs::remove_dir(link_path).ok();
                    } else {
                        fs::remove_file(link_path).ok();
                    }
                }
            }
        }

        // 创建新的 current 符号链接
        #[cfg(windows)]
        {
            // Windows 需要使用目录符号链接
            if let Err(e) = std::os::windows::fs::symlink_dir(final_path, link_path) {
                eprintln!("Warning: Failed to create 'current' symlink: {}", e);
                eprintln!("(Note: Creating directory symlinks on Windows requires Developer Mode or Admin rights)");
            }
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(final_path, link_path)
                .with_context(|| "Failed to create symlink on Unix")?;
        }

        Ok(())
    }
}
