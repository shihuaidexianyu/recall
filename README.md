# Recall

基于 NTFS 硬链接的增量备份工具，类似于 macOS Time Machine 的 Windows 替代方案。

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows-blue.svg)]()

## ✨ 特性

- **🔗 硬链接去重** - 未修改的文件使用 NTFS 硬链接，极大节省磁盘空间
- **📈 增量备份** - 仅复制新增或修改的文件，备份速度快
- **🔒 VSS 支持** - 使用 Windows 卷影复制服务备份被锁定的文件（需管理员权限）
- **⚡ 并行处理** - 多线程扫描和复制，充分利用硬件性能
- **🖥️ 交互式界面** - 友好的 TUI 界面管理/编辑备份配置
- **🛡️ 内容校验** - 可选的 XXH3 哈希校验，确保数据完整性
- **🗑️ 自动清理** - 支持保留指定数量的备份，自动删除旧版本

## 📦 安装

### 从源码编译

```bash
git clone https://github.com/shihuaidexianyu/recall.git
cd recall
cargo build --release
```

编译后的可执行文件位于 `target/release/recall.exe`

### 系统要求

- Windows 10/11
- NTFS 文件系统（源和目标）
- 管理员权限（使用 VSS 功能时）

## 🚀 快速开始

### 交互模式（推荐）

直接运行程序进入交互式界面：

```bash
recall
```

界面会显示所有保存的备份配置，可以：

- 选择已有配置执行备份
- 创建新的备份配置
- 修改已有配置
- 删除不需要的配置

交互式配置可设置：

- 内容校验开关
- VSS 开关（Windows）
- 工作线程数
- 排除模式（逗号分隔）

### 命令行模式

```bash
# 基本备份
recall "D:\Projects" "E:\Backups"

# 启用内容校验（更安全但更慢）
recall "D:\Projects" "E:\Backups" --check-content

# 启用 VSS 快照（备份被锁定的文件）
recall "D:\Projects" "E:\Backups" --vss

# 试运行（不实际复制文件）
recall "D:\Projects" "E:\Backups" --dry-run

# 指定工作线程数
recall "D:\Projects" "E:\Backups" --workers 8

# 排除特定模式
recall "D:\Projects" "E:\Backups" --exclude "*.tmp" --exclude "node_modules"
```

### 清理旧备份

```bash
# 保留最近 5 个备份
recall prune "E:\Backups\Projects" --keep 5

# 试运行，查看哪些会被删除
recall prune "E:\Backups\Projects" --keep 3 --dry-run
```

## 📁 备份结构

备份目录结构如下：

```
E:\Backups\
└── Projects\                    # 项目名称（自动从源路径生成）
    ├── 2024-01-15_10-30-00\    # 完整备份快照
    ├── 2024-01-16_10-30-00\    # 增量备份（未修改文件为硬链接）
    ├── 2024-01-17_10-30-00\
    └── current -> 2024-01-17_10-30-00  # 指向最新备份的符号链接
```

## ⚙️ 配置

### 排除文件 (.recallignore)

在源目录创建 `.recallignore` 文件来排除不需要备份的文件：

```gitignore
# 版本控制
.git
.svn

# 缓存和临时文件
*.tmp
*.log
node_modules
__pycache__

# Windows 系统文件
System Volume Information
$RECYCLE.BIN
Thumbs.db
```

首次运行会自动创建包含常用排除项的默认文件。

### 配置文件存储

用户配置保存在：

- Windows: `%APPDATA%\recall\config.toml`

配置内容包含：源路径、目标路径、内容校验、VSS、工作线程数、排除列表。

## 🔧 命令行参数

```
recall [OPTIONS] [SOURCE] [DESTINATION]
recall prune [OPTIONS] <DESTINATION>

参数:
  [SOURCE]        源路径（要备份的目录）
  [DESTINATION]   目标路径（备份存储位置）

选项:
  --check-content    启用内容校验（使用哈希值比较）
  --dry-run          试运行模式（不实际复制）
  --exclude <PATTERN> 排除模式（可多次指定）
  --workers <N>      工作线程数量 [默认: 4]
  --vss              启用 VSS 快照（仅 Windows）
  -h, --help         显示帮助信息
  -V, --version      显示版本信息

Prune 子命令:
  --keep <N>         保留的备份数量 [默认: 5]
```

## 📊 工作原理

1. **扫描阶段** - 遍历源目录，与最新备份对比
2. **决策阶段** - 对每个文件判断操作：
   - **新文件** → 复制
   - **已修改** → 复制（可选哈希校验）
   - **未修改** → 创建硬链接
3. **执行阶段** - 多线程并行处理文件操作
4. **提交阶段** - 原子性重命名临时目录，更新 current 链接

## 🛠️ VSS 说明

- 仅在 Windows 上可用，且需要管理员权限运行。
- 通过对源路径所在卷创建快照，整个源目录都从快照读取（不是只覆盖部分文件）。
- 用于解决文件被占用但是仍需备份的情况。

## 🛠️ 技术栈

- **Rust** - 高性能系统编程语言
- **crossbeam-channel** - 高性能并发通道
- **rayon** - 数据并行库
- **xxhash-rust** - 超快速哈希算法
- **indicatif** - 进度条显示
- **dialoguer** - 交互式命令行界面
- **winapi** - Windows API 绑定（VSS）

## 📄 许可证

MIT License

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

---

**Recall** - 让备份像呼吸一样自然 🌟
