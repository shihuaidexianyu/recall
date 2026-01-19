// Recall - 备份配置管理模块
// 负责创建和管理单次备份任务的配置

use crate::store::Profile;
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// 备份配置结构体
///
/// 定义单次备份操作的所有参数，包括源路径、目标路径、
/// 是否启用内容检查、排除模式和是否为试运行模式。
#[derive(Debug, Clone)]
pub struct BackupConfig {
    /// 源路径（要备份的目录）
    pub source: PathBuf,

    /// 目标路径（备份存储位置）
    pub destination: PathBuf,

    /// 是否启用内容检查（使用哈希值比较文件，更准确但更慢）
    pub check_content: bool,

    /// 排除模式列表（Glob 风格）
    pub exclude_patterns: Vec<String>,

    /// 是否为试运行模式（不实际复制文件）
    pub dry_run: bool,
}

impl BackupConfig {
    /// 创建新的备份配置
    ///
    /// # 参数
    /// * `source` - 源路径
    /// * `destination` - 目标路径
    /// * `check_content` - 是否启用内容检查
    /// * `exclude_patterns` - 排除模式列表
    /// * `dry_run` - 是否为试运行模式
    ///
    /// # 返回
    /// * `Ok(BackupConfig)` - 创建的备份配置
    /// * `Err(anyhow::Error)` - 处理 `.recallignore` 失败
    pub fn new(
        source: PathBuf,
        destination: PathBuf,
        check_content: bool,
        exclude_patterns: Vec<String>,
        dry_run: bool,
    ) -> Result<Self> {
        let mut config = Self {
            source,
            destination,
            check_content,
            exclude_patterns,
            dry_run,
        };

        // 处理 .recallignore 文件，保持与 from_profile 一致
        config.process_recallignore()?;

        Ok(config)
    }

    /// 从配置文件（Profile）创建备份配置
    ///
    /// 此方法会处理 `.recallignore` 文件，将其中的排除模式添加到配置中。
    ///
    /// # 参数
    /// * `profile` - 保存的配置文件
    /// * `project_name` - 项目名称（用于构建目标路径）
    /// * `dry_run` - 是否为试运行模式
    ///
    /// # 返回
    /// * `Ok(BackupConfig)` - 创建的备份配置
    /// * `Err(anyhow::Error)` - 处理失败
    pub fn from_profile(profile: &Profile, project_name: &str, dry_run: bool) -> Result<Self> {
        // 将项目名称附加到目标路径
        let final_dest = profile.destination.join(project_name);

        let mut config = Self {
            source: profile.source.clone(),
            destination: final_dest,
            check_content: profile.check_content,
            exclude_patterns: profile.exclude.clone(),
            dry_run,
        };

        // 处理 .recallignore 文件
        config.process_recallignore()?;

        Ok(config)
    }

    /// 处理 `.recallignore` 文件
    ///
    /// 如果 `.recallignore` 文件不存在，会自动创建一个默认的排除文件。
    /// 然后读取该文件并将所有排除模式添加到配置中。
    ///
    /// # 返回
    /// * `Ok(())` - 处理成功
    /// * `Err(anyhow::Error)` - 处理失败
    fn process_recallignore(&mut self) -> Result<()> {
        let ignore_file_path = self.source.join(".recallignore");

        // 如果文件不存在，创建默认的排除文件
        if !ignore_file_path.exists() {
            self.create_default_ignore_file(&ignore_file_path)?;
        }

        // 读取并解析排除文件
        let content =
            fs::read_to_string(&ignore_file_path).context("Failed to read .recallignore")?;

        for line in content.lines() {
            let line = line.trim();
            // 跳过空行和注释行
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // 避免重复添加
            if !self.exclude_patterns.contains(&line.to_string()) {
                self.exclude_patterns.push(line.to_string());
            }
        }

        Ok(())
    }

    /// 创建默认的 `.recallignore` 文件
    ///
    /// 根据操作系统创建不同的默认排除内容。
    ///
    /// # 参数
    /// * `path` - 要创建的文件路径
    ///
    /// # 返回
    /// * `Ok(())` - 创建成功
    /// * `Err(anyhow::Error)` - 创建失败
    fn create_default_ignore_file(&self, path: &PathBuf) -> Result<()> {
        let mut default_content = String::from(
            "# Recall Ignore File\n# Add patterns to exclude from backup (Glob style)\n\n# --- Common ---\n.git\n.svn\n.DS_Store\nThumbs.db\n\n"
        );

        // Windows 特定的排除项
        #[cfg(windows)]
        {
            default_content.push_str(
                "# --- Windows System ---\nSystem Volume Information\n$RECYCLE.BIN\nRecovery\npagefile.sys\nhiberfil.sys\nswapfile.sys\nDumpStack.log.tmp\n"
            );
        }

        // Linux/macOS 特定的排除项
        #[cfg(not(windows))]
        {
            default_content.push_str("# --- Linux/macOS ---\n/proc\n/sys\n/dev\n");
        }

        let mut file = fs::File::create(path).context("Failed to create .recallignore")?;
        file.write_all(default_content.as_bytes())?;

        if !self.dry_run {
            println!("Created default ignore file at: {:?}", path);
        }

        Ok(())
    }
}
