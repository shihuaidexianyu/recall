// Recall - 基于 NTFS 硬链接的增量备份工具
//
// 主程序入口，负责命令行参数解析和备份流程协调
//
// 功能特性：
// - 增量备份：仅复制修改过的文件
// - 硬链接：未修改的文件使用硬链接节省空间
// - 流水线处理：扫描和执行并行进行
// - VSS 支持（Windows）：备份被锁定的文件
// - 交互式配置管理：保存和管理备份配置

use anyhow::{Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use console::style;
use indicatif::ProgressBar;
use recall::cli::run_interactive_mode;
use recall::config::BackupConfig;
use recall::executor::BackupExecutor;
use recall::scanner::{find_latest_backup, scan_source};
use recall::utils::{format_bytes, format_duration};
use std::path::PathBuf;
use std::time::Duration;
use std::thread;
use crossbeam_channel;

/// 子命令枚举
#[derive(Subcommand, Debug)]
enum Commands {
    /// 清理旧备份
    Prune {
        /// 要保留的备份数量
        #[arg(long, default_value_t = 5)]
        keep: usize,

        /// 要清理的目标路径。如果未提供，将尝试从交互模式或配置文件推断
        /// 目前需要显式指定路径
        #[arg(value_name = "DESTINATION")]
        destination: Option<PathBuf>,
    },
}

/// 命令行参数结构体
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 子命令
    #[command(subcommand)]
    command: Option<Commands>,

    /// 源路径
    #[arg(value_name = "SOURCE")]
    source: Option<PathBuf>,

    /// 目标路径
    #[arg(value_name = "DESTINATION")]
    destination: Option<PathBuf>,

    /// 启用内容检查（使用哈希值比较文件，更准确但更慢）
    #[arg(long, global = true)]
    check_content: bool,

    /// 试运行模式（不实际复制文件）
    #[arg(long, global = true)]
    dry_run: bool,

    /// 排除模式（Glob 风格）
    #[arg(long, global = true)]
    exclude: Vec<String>,

    /// 工作线程数量
    #[arg(long, default_value_t = 4)]
    workers: usize,

    /// 启用 VSS 快照（仅 Windows）
    #[arg(long)]
    vss: bool,
}

/// 程序入口
fn main() -> Result<()> {
    let args = Args::parse();

    match &args.command {
        Some(Commands::Prune { keep, destination }) => {
            // 处理清理命令
            let dest = destination
                .as_ref()
                .or(args.destination.as_ref())
                .context("Destination path is required for prune command")?;

            // 支持全局 dry_run 参数
            recall::prune::prune_backups(dest, *keep, args.dry_run)?;
        }
        None => {
            // 执行备份
            run_backup(args)?;
        }
    }
    Ok(())
}

/// 执行备份操作
fn run_backup(args: Args) -> Result<()> {
    // 准备备份配置
    let (config, _) = if let (Some(src), Some(dest)) = (args.source, args.destination) {
        // 使用命令行参数指定的路径
        let source_abs = std::fs::canonicalize(&src).context("Failed to get absolute path of source")?;

        // 生成项目名称
        let project_name = if let Some(name) = source_abs.file_name() {
            name.to_string_lossy().to_string()
        } else {
            // 如果是驱动器根目录，生成特殊名称
            let path_str = source_abs.to_string_lossy();
             if let Some(colon_idx) = path_str.find(':') {
                if colon_idx > 0 {
                    let drive = &path_str[colon_idx - 1..colon_idx];
                    format!("{}_Drive", drive.to_uppercase())
                } else {
                    "Unknown_Drive".to_string()
                }
            } else {
                "Root_Backup".to_string()
            }
        };

        // 构建最终目标路径
        let final_destination_root = dest.join(&project_name);
        let config = BackupConfig::new(
            source_abs,
            final_destination_root.clone(),
            args.check_content,
            args.exclude,
            args.dry_run,
        );
        (config, project_name)
    } else {
        // 进入交互模式
        run_interactive_mode(args.dry_run)?
    };

    // 记录开始时间
    let start_time = std::time::Instant::now();
    let now = Local::now();
    let timestamp_folder_name = now.format("%Y-%m-%d_%H-%M-%S").to_string();

    // 打印备份信息
    println!("{}", style(format!("Recall Backup Tool v{}", env!("CARGO_PKG_VERSION"))).cyan().bold());
    println!("Source: {:?}", style(&config.source).blue());
    println!("Dest:   {:?}", style(&config.destination).blue());
    println!("Time:   {}", style(&timestamp_folder_name).yellow());
    println!("{}", style("----------------------------------------").dim());

    // 创建目标根目录（如果不存在）
    if !config.destination.exists() {
        if !config.dry_run {
            std::fs::create_dir_all(&config.destination)
                .context("Failed to create destination root")?;
        } else {
             println!("{} Would create destination root {:?}", style("Dry run:").yellow(), config.destination);
        }
    }

    // 查找最新的备份（用于增量备份）
    let latest_backup = find_latest_backup(&config.destination)?;
    if let Some(ref latest) = latest_backup {
        println!("Found previous backup: {:?}", style(latest.file_name().unwrap()).green());
    } else {
        println!("{}", style("Performing initial full backup...").yellow());
    }

    // 准备临时和最终备份目录
    let temp_folder_name = format!("{}.partial", timestamp_folder_name);
    let temp_dest_path = config.destination.join(&temp_folder_name);
    let final_dest_path = config.destination.join(&timestamp_folder_name);

    // 创建临时备份目录
    if !config.dry_run {
        std::fs::create_dir_all(&temp_dest_path).context("Failed to create temp backup dir")?;
    } else {
        println!("{} Would create temp dir {:?}", style("Dry run:").yellow(), temp_dest_path);
    }

    // === VSS 设置 ===
    #[cfg(windows)]
    let _vss_guard = if args.vss && !config.dry_run {
        println!("{}", style("Initializing VSS Snapshot...").blue());
        let sc = recall::vss::ShadowCopy::new(&config.source).context("Failed to create VSS snapshot")?;
        println!("Snapshot created at: {:?}", sc.get_snapshot_path()?);
        Some(sc)
    } else {
        None
    };

    #[cfg(not(windows))]
    if args.vss {
        println!("{}", style("Warning: VSS is only supported on Windows. Ignoring --vss").yellow());
    }

    // 准备扫描器配置（可能因 VSS 修改源路径）
    let mut scan_config = config.clone();
    #[cfg(windows)]
    if let Some(ref sc) = _vss_guard {
         // 将 config.source 映射到快照路径
         // 1. 获取源的卷根目录（如 C:\）
         let src_abs = &config.source; // 已经是规范化路径
         let _volume_root = src_abs.components().take(2).collect::<PathBuf>();

         // 简单方法：找出从卷根目录开始的相对路径
         // 例如：source = C:\Users\Data，Volume = C:\，Rel = Users\Data
         // Shadow = \\?\GLOBAL...\
         // New Source = Shadow\Users\Data

         if let Some(path_str) = src_abs.to_str() {
             if let Some(colon) = path_str.find(':') {
                  let _drive_root = &path_str[..colon+1]; // C:
                  let rel_part = &path_str[colon+1..]; // \Users\Data
                  let rel_part = rel_part.trim_start_matches('\\').trim_start_matches('/');

                  let shadow_path = sc.get_snapshot_path()?;
                  let new_source = shadow_path.join(rel_part);
                  scan_config.source = new_source;
                  println!("Backup Source mapped to VSS path: {:?}", scan_config.source);
             }
         }
    }

    // === 流水线处理开始 ===
    let (tx, rx) = crossbeam_channel::bounded(1000);

    let temp_dest_clone = temp_dest_path.clone();
    let latest_backup_clone = latest_backup.clone();

    // 使用可能指向 VSS 的 scan_config
    let config_for_scanner = scan_config.clone();

    // 启动扫描线程
    let scanner_handle = thread::spawn(move || {
        scan_source(
             &config_for_scanner,
             &temp_dest_clone,
             latest_backup_clone.as_deref(),
             tx
        )
    });

    // 在主线程执行备份任务
    let executor = BackupExecutor::new(config.dry_run);
    let stats = executor.execute(rx, args.workers)?;

    // 等待扫描完成
    if let Err(e) = scanner_handle.join().unwrap() {
        eprintln!("{} Scanner failed: {}", style("Error:").red(), e);
        return Err(e);
    }
    // === 流水线处理结束 ===

    // 提交备份（重命名临时目录并更新 current 链接）
    if !config.dry_run {
        let spinner = ProgressBar::new_spinner();
        spinner.set_message("Finalizing backup...");
        spinner.enable_steady_tick(Duration::from_millis(100));

        BackupExecutor::commit_backup(
            &temp_dest_path,
            &final_dest_path,
            &config.destination.join("current"),
        )?;
        spinner.finish_and_clear();
    } else {
        println!("{} Would rename .partial to {:?}", style("Dry run:").yellow(), final_dest_path);
        println!("{} Would update 'current' symlink", style("Dry run:").yellow());
    }

    // 打印备份统计信息
    println!("{}", style("----------------------------------------").dim());
    println!("{}", style("Backup Completed Successfully!").green().bold());
    println!("Total Files:     {}", stats.total_files);
    println!("Copied (New):    {}", style(stats.copied_new).green());
    println!("Copied (Mod):    {}", style(stats.copied_modified).yellow());
    println!("Hard Linked:     {}", style(stats.linked).dim());
    println!("Skipped:         {}", style(stats.skipped).red());
    println!("Failed:          {}", style(stats.failed).red().bold());
    println!("Data Transferred: {}", style(format_bytes(stats.bytes_copied)).cyan());
    println!(
        "Total Duration:   {}",
        style(format_duration(start_time.elapsed().as_secs())).bold()
    );

    Ok(())
}
