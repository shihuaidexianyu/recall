// Recall - 文件哈希计算模块
// 使用 XXH3 算法计算文件内容的哈希值，用于检测文件是否发生变化

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use xxhash_rust::xxh3::Xxh3;

/// 计算文件内容的 XXH3 哈希值
///
/// 此函数使用 XXH3 算法计算文件的哈希值，用于在启用内容检查时
/// 比较源文件和备份文件是否完全相同。
///
/// # 参数
/// * `path` - 要计算哈希的文件路径
///
/// # 返回
/// * `Ok(u64)` - 文件的 64 位哈希值
/// * `Err(anyhow::Error)` - 如果读取文件失败
///
/// # 实现细节
/// - 使用 1MB 的缓冲区读取文件
/// - 使用 64KB 的缓冲区进行哈希计算
/// - XXH3 是一种非常快速的非加密哈希算法，适合文件比对
pub fn calculate_hash(path: &Path) -> anyhow::Result<u64> {
    // 打开文件
    let file = File::open(path)?;

    // 创建带缓冲的读取器，1MB 缓冲区以提高性能
    let mut reader = BufReader::with_capacity(1024 * 1024, file);

    // 创建 XXH3 哈希器
    let mut hasher = Xxh3::new();

    // 64KB 的哈希计算缓冲区
    let mut buffer = [0u8; 64 * 1024];

    // 逐块读取文件并更新哈希值
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break; // 文件读取完毕
        }
        hasher.update(&buffer[..count]);
    }

    // 返回最终的哈希值
    Ok(hasher.digest())
}
