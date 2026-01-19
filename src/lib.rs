// Recall - 基于 NTFS 硬链接的增量备份工具
// 模块声明文件

/// 文件操作和同步动作相关模块
pub mod actions;

/// 命令行交互界面模块
pub mod cli;

/// 备份清理模块（删除旧备份）
pub mod prune;

/// 卷影复制服务（VSS）模块（仅 Windows）
#[cfg(windows)]
pub mod vss;

/// 备份配置管理模块
pub mod config;

/// 备份执行器模块
pub mod executor;

/// 文件哈希计算模块
pub mod hasher;

/// 源文件扫描模块
pub mod scanner;

/// 配置文件存储模块
pub mod store;

/// 工具函数模块
pub mod utils;
