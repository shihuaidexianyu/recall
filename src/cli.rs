// Recall - 命令行交互界面模块
// 提供交互式命令行界面，用于管理备份配置文件

use anyhow::{Context, Result};
use console::style;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::path::PathBuf;

use crate::config::BackupConfig;
use crate::store::{AppConfig, Profile};

/// 运行交互式模式
///
/// 此函数提供交互式命令行界面，允许用户：
/// - 选择已保存的配置文件（Profile）
/// - 创建新的配置文件
/// - 删除配置文件
/// - 退出程序
///
/// # 参数
/// * `dry_run` - 是否为试运行模式
///
/// # 返回
/// * `Ok((BackupConfig, String))` - 备份配置和项目名称
/// * `Err(anyhow::Error)` - 操作失败
pub fn run_interactive_mode(dry_run: bool) -> Result<(BackupConfig, String)> {
    // 加载应用配置
    let mut app_config = AppConfig::load()?;
    let theme = ColorfulTheme::default();

    // 显示欢迎信息
    println!(
        "{}",
        style(format!("Recall Backup Tool v{}", env!("CARGO_PKG_VERSION")))
            .cyan()
            .bold()
    );
    println!(
        "{}",
        style("----------------------------------------").dim()
    );

    loop {
        // 获取所有配置文件名称并排序
        let mut profiles: Vec<String> = app_config.profiles.keys().cloned().collect();
        profiles.sort();

        // 构建菜单选项，显示 profile 详情
        let mut choices: Vec<String> = profiles
            .iter()
            .map(|name| {
                if let Some(profile) = app_config.profiles.get(name) {
                    let src = profile.source.to_string_lossy();
                    let dst = profile.destination.to_string_lossy();
                    let check = if profile.check_content { "✓" } else { "✗" };
                    format!("{} ({} → {}) [{}]", name, src, dst, check)
                } else {
                    name.clone()
                }
            })
            .collect();

        choices.push(">> Create New Profile".to_string());
        if !profiles.is_empty() {
            choices.push(">> Delete Profile".to_string());
        }
        choices.push(">> Exit".to_string());

        // 显示选择菜单
        let selection = Select::with_theme(&theme)
            .with_prompt("Select a backup profile")
            .default(0)
            .items(&choices)
            .interact()?;

        let choice = &choices[selection];

        if choice == ">> Exit" {
            // 用户选择退出
            std::process::exit(0);
        } else if choice == ">> Create New Profile" {
            // 创建新配置文件
            create_new_profile(&mut app_config)?;
            continue;
        } else if choice == ">> Delete Profile" {
            // 删除配置文件
            delete_profile(&mut app_config)?;
            continue;
        } else {
            // 用户选择了一个配置文件
            // selection 索引对应 profiles 数组
            let profile_name = &profiles[selection];
            let profile = app_config.profiles.get(profile_name).unwrap();

            // 获取源目录的绝对路径
            let src_abs = std::fs::canonicalize(&profile.source)
                .context("Source path in profile does not exist")?;

            // 生成项目名称
            let project_name = get_project_name(&src_abs);

            // 从配置文件创建备份配置
            let config = BackupConfig::from_profile(profile, &project_name, dry_run);
            return Ok((config?, project_name));
        }
    }
}

/// 创建新的配置文件（Profile）
///
/// 引导用户输入配置文件的各项参数，并保存到配置文件中。
///
/// # 参数
/// * `config` - 可变的应用配置引用
///
/// # 返回
/// * `Ok(())` - 创建成功
/// * `Err(anyhow::Error)` - 创建失败
fn create_new_profile(config: &mut AppConfig) -> Result<()> {
    let theme = ColorfulTheme::default();

    // 获取配置文件名称
    let name: String = Input::with_theme(&theme)
        .with_prompt("Profile Name")
        .interact_text()?;

    // 获取源路径
    let source: String = Input::with_theme(&theme)
        .with_prompt("Source Path")
        .interact_text()?;

    // 获取备份根路径
    let dest: String = Input::with_theme(&theme)
        .with_prompt("Backup Root Path")
        .interact_text()?;

    // 询问是否启用内容检查
    let check_content = Confirm::with_theme(&theme)
        .with_prompt("Enable Content Check (Slower but safer)?")
        .default(false)
        .interact()?;

    // 创建新的配置文件
    let profile = Profile {
        source: PathBuf::from(source),
        destination: PathBuf::from(dest),
        check_content,
        exclude: vec![],
    };

    // 保存到配置文件
    config.profiles.insert(name, profile);
    config.save()?;
    println!("Profile saved successfully!");
    Ok(())
}

/// 删除配置文件（Profile）
///
/// 显示配置文件列表供用户选择删除。
///
/// # 参数
/// * `config` - 可变的应用配置引用
///
/// # 返回
/// * `Ok(())` - 删除成功（或用户取消）
/// * `Err(anyhow::Error)` - 删除失败
fn delete_profile(config: &mut AppConfig) -> Result<()> {
    // 获取并排序所有配置文件名称
    let mut profiles: Vec<String> = config.profiles.keys().cloned().collect();
    if profiles.is_empty() {
        println!("{}", style("No profiles available to delete.").yellow());
        return Ok(());
    }
    profiles.sort();

    // 显示选择菜单
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a profile to DELETE")
        .items(&profiles)
        .interact()?;

    let profile_name = &profiles[selection];

    // 确认删除
    if Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "Are you sure you want to delete profile '{}'?",
            style(profile_name).red().bold()
        ))
        .default(false)
        .interact()?
    {
        // 删除配置文件
        config.profiles.remove(profile_name);
        config.save()?;
        println!(
            "{} '{}' has been deleted.",
            style("Success:").green(),
            profile_name
        );
    } else {
        println!("Operation cancelled.");
    }

    Ok(())
}

/// 从路径获取项目名称
///
/// 使用路径的最后一部分作为项目名称。
/// 例如：`C:\Users\Data` -> `Data`
/// 对于驱动器根路径：`D:\` -> `D_Drive`
///
/// # 参数
/// * `path` - 源路径
///
/// # 返回
/// 项目名称字符串
fn get_project_name(path: &std::path::Path) -> String {
    if let Some(name) = path.file_name() {
        name.to_string_lossy().to_string()
    } else {
        // 如果是驱动器根目录，生成特殊名称
        let path_str = path.to_string_lossy();
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
    }
}

