// Recall - 文件操作和同步动作定义
// 定义了备份过程中的各种操作类型和相关数据结构

use std::path::PathBuf;

/// 同步动作枚举
/// 定义了在备份过程中可以对文件执行的各种操作
#[derive(Debug, Clone)]
pub enum SyncAction {
    /// 复制新文件（文件在源目录存在，但从未备份过）
    CopyNew,

    /// 复制已修改的文件（文件内容发生变化）
    CopyModified,

    /// 创建硬链接（文件未变化，使用 NTFS 硬链接节省空间）
    Link(PathBuf),

    /// 创建符号链接（源文件是符号链接）
    MakeSymlink(PathBuf),

    /// 创建目录
    CreateDir,

    /// 跳过（不需要处理）
    Skip,
}

/// 文件任务结构体
/// 表示单个文件的备份任务，包含所有必要的路径信息
#[derive(Debug, Clone)]
pub struct FileTask {
    /// 相对于源目录的路径
    pub rel_path: PathBuf,

    /// 源文件的完整路径（可能是 VSS 快照路径）
    pub src_path: PathBuf,

    /// 目标备份路径（当前备份目录）
    pub dest_path: PathBuf,

    /// 上一次备份的路径（用于增量备份和硬链接）
    pub old_path: Option<PathBuf>,
}

impl FileTask {
    /// 创建新的文件任务
    ///
    /// # 参数
    /// * `rel_path` - 相对于源目录的路径
    /// * `src_path` - 源文件的完整路径
    /// * `dest_path` - 目标备份路径
    /// * `old_path` - 上一次备份的路径（可选）
    pub fn new(
        rel_path: PathBuf,
        src_path: PathBuf,
        dest_path: PathBuf,
        old_path: Option<PathBuf>,
    ) -> Self {
        Self {
            rel_path,
            src_path,
            dest_path,
            old_path,
        }
    }
}

/// 备份统计信息结构体
/// 记录备份操作的各项统计数据
#[derive(Debug, Default, Clone)]
pub struct BackupStats {
    /// 处理的文件总数
    pub total_files: u64,

    /// 复制的新文件数量
    pub copied_new: u64,

    /// 复制的已修改文件数量
    pub copied_modified: u64,

    /// 硬链接的文件数量
    pub linked: u64,

    /// 跳过的文件数量
    pub skipped: u64,

    /// 失败的文件数量
    pub failed: u64,

    /// 传输的总字节数
    pub bytes_copied: u64,
}

impl BackupStats {
    /// 创建新的空统计信息
    pub fn new() -> Self {
        Self::default()
    }
}
