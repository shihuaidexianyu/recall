// Recall - 配置文件存储模块
// 负责管理用户配置文件的加载和保存

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 备份配置文件（Profile）
///
/// 定义单个备份任务的所有配置参数，保存在全局配置文件中。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    /// 源路径（要备份的目录）
    pub source: PathBuf,

    /// 备份目标根路径
    pub destination: PathBuf,

    /// 是否启用内容检查（使用哈希值比较文件，更准确但更慢）
    pub check_content: bool,

    /// 排除模式列表（Glob 风格）
    pub exclude: Vec<String>,
}

/// 应用程序全局配置
///
/// 包含所有用户定义的备份配置文件（Profile）。
/// 配置文件存储在系统标准配置目录中。
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// 配置文件集合，键为配置文件名称
    pub profiles: HashMap<String, Profile>,
}

impl AppConfig {
    /// 从配置文件加载应用配置
    ///
    /// # 返回
    /// * `Ok(AppConfig)` - 加载的配置，如果文件不存在则返回空配置
    /// * `Err(anyhow::Error)` - 如果配置文件存在但解析失败
    pub fn load() -> Result<Self> {
        let path = Self::get_config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        toml::from_str(&content).context("Failed to parse config file")
    }

    /// 保存配置到文件
    ///
    /// 如果配置目录不存在，会自动创建。
    ///
    /// # 返回
    /// * `Ok(())` - 配置保存成功
    /// * `Err(anyhow::Error)` - 保存失败
    pub fn save(&self) -> Result<()> {
        let path = Self::get_config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content).context("Failed to write config file")
    }

    /// 获取配置文件的路径
    ///
    /// 使用 `directories` crate 获取平台标准的配置目录：
    /// - Windows: `C:\Users\<用户>\AppData\Roaming\recall\config.toml`
    /// - macOS: `~/Library/Application Support/recall/config.toml`
    /// - Linux: `~/.config/recall/config.toml`
    ///
    /// # 返回
    /// * `Ok(PathBuf)` - 配置文件的完整路径
    /// * `Err(anyhow::Error)` - 无法确定配置目录
    fn get_config_path() -> Result<PathBuf> {
        let proj_dirs =
            ProjectDirs::from("", "", "recall").context("Could not determine config directory")?;
        Ok(proj_dirs.config_dir().join("config.toml"))
    }
}
