// Recall - 卷影复制服务（VSS）模块（仅 Windows）
// 负责创建和管理 Windows 卷影副本，用于备份被锁定的文件

use anyhow::{bail, Result};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null_mut, NonNull};
use winapi::shared::winerror::{FAILED, RPC_E_CHANGED_MODE};
use winapi::um::combaseapi::{CoInitializeEx, CoUninitialize};
use winapi::um::fileapi::GetVolumePathNameW;
use winapi::um::objbase::COINIT_MULTITHREADED;
use winapi::um::vsbackup::{CreateVssBackupComponents, IVssBackupComponents, VssFreeSnapshotProperties};
use winapi::um::vss::{
    IVssAsync, VSS_BT_COPY, VSS_CTX_BACKUP, VSS_ID, VSS_OBJECT_SNAPSHOT, VSS_SNAPSHOT_PROP,
};
use winapi::um::winnt::HRESULT;

/// 卷影副本（Shadow Copy）包装器
///
/// 此结构体封装了 Windows VSS（卷影复制服务）功能。
/// VSS 允许创建卷的快照，用于备份正在使用或被锁定的文件。
///
/// # 注意
/// 此功能仅适用于 Windows 平台。
pub struct ShadowCopy {
    components: NonNull<IVssBackupComponents>,
    snapshot_id: VSS_ID,
    device_path: PathBuf,
    com_initialized: bool,
}

impl ShadowCopy {
    /// 创建新的卷影副本
    ///
    /// # 参数
    /// * `source_path` - 需要创建快照的源路径
    ///
    /// # 返回
    /// * `Ok(ShadowCopy)` - 卷影副本对象
    /// * `Err(anyhow::Error)` - 创建失败
    pub fn new(source_path: &Path) -> Result<Self> {
        unsafe {
            let mut com_initialized = false;
            let mut components_ptr: *mut IVssBackupComponents = null_mut();

            let result = (|| -> Result<Self> {
                let hr = CoInitializeEx(null_mut(), COINIT_MULTITHREADED);
                if hr == RPC_E_CHANGED_MODE {
                    bail!("COM already initialized with a different threading model");
                }
                check_hr(hr, "CoInitializeEx")?;
                com_initialized = true;

                check_hr(
                    CreateVssBackupComponents(&mut components_ptr),
                    "CreateVssBackupComponents",
                )?;
                let components = NonNull::new(components_ptr)
                    .ok_or_else(|| anyhow::anyhow!("CreateVssBackupComponents returned null"))?;

                check_hr(
                    (*components.as_ptr()).InitializeForBackup(null_mut()),
                    "InitializeForBackup",
                )?;
                check_hr(
                    (*components.as_ptr()).SetBackupState(false, false, VSS_BT_COPY, false),
                    "SetBackupState",
                )?;
                check_hr(
                    (*components.as_ptr()).SetContext(VSS_CTX_BACKUP as i32),
                    "SetContext",
                )?;

                let mut async_ptr: *mut IVssAsync = null_mut();
                check_hr(
                    (*components.as_ptr()).GatherWriterMetadata(&mut async_ptr),
                    "GatherWriterMetadata",
                )?;
                wait_async(async_ptr, "GatherWriterMetadata")?;
                check_hr((*components.as_ptr()).FreeWriterMetadata(), "FreeWriterMetadata")?;

                let mut snapshot_set_id = zero_guid();
                check_hr(
                    (*components.as_ptr()).StartSnapshotSet(&mut snapshot_set_id),
                    "StartSnapshotSet",
                )?;

                let volume_root = get_volume_root(source_path)?;
                let mut volume_root_wide = to_wide_null(&volume_root);

                let mut snapshot_id = zero_guid();
                check_hr(
                    (*components.as_ptr()).AddToSnapshotSet(
                        volume_root_wide.as_mut_ptr(),
                        zero_guid(),
                        &mut snapshot_id,
                    ),
                    "AddToSnapshotSet",
                )?;

                let mut prepare_async: *mut IVssAsync = null_mut();
                check_hr(
                    (*components.as_ptr()).PrepareForBackup(&mut prepare_async),
                    "PrepareForBackup",
                )?;
                wait_async(prepare_async, "PrepareForBackup")?;

                let mut snapshot_async: *mut IVssAsync = null_mut();
                check_hr(
                    (*components.as_ptr()).DoSnapshotSet(&mut snapshot_async),
                    "DoSnapshotSet",
                )?;
                wait_async(snapshot_async, "DoSnapshotSet")?;

                let mut props: VSS_SNAPSHOT_PROP = std::mem::zeroed();
                check_hr(
                    (*components.as_ptr()).GetSnapshotProperties(snapshot_id, &mut props),
                    "GetSnapshotProperties",
                )?;
                let device_path = wide_ptr_to_string(props.m_pwszSnapshotDeviceObject)?;
                VssFreeSnapshotProperties(&mut props);

                Ok(Self {
                    components,
                    snapshot_id,
                    device_path: PathBuf::from(device_path),
                    com_initialized,
                })
            })();

            if result.is_err() {
                if !components_ptr.is_null() {
                    let _ = (*components_ptr).Release();
                }
                if com_initialized {
                    CoUninitialize();
                }
            }

            result
        }
    }

    /// 获取快照的路径
    ///
    /// # 返回
    /// * `Ok(PathBuf)` - 快照的访问路径
    pub fn get_snapshot_path(&self) -> Result<PathBuf> {
        Ok(self.device_path.clone())
    }
}

/// 自动释放卷影副本
///
/// 当 ShadowCopy 对象被销毁时，自动清理 VSS 资源。
impl Drop for ShadowCopy {
    fn drop(&mut self) {
        unsafe {
            let components_ptr = self.components.as_ptr();
            if !components_ptr.is_null() {
                let mut async_ptr: *mut IVssAsync = null_mut();
                let hr = (*components_ptr).BackupComplete(&mut async_ptr);
                if !async_ptr.is_null() && !FAILED(hr) {
                    let _ = wait_async(async_ptr, "BackupComplete");
                } else if !async_ptr.is_null() {
                    let _ = (*async_ptr).Release();
                }

                let mut deleted = 0;
                let mut non_deleted = zero_guid();
                let _ = (*components_ptr).DeleteSnapshots(
                    self.snapshot_id,
                    VSS_OBJECT_SNAPSHOT,
                    0,
                    &mut deleted,
                    &mut non_deleted,
                );

                let _ = (*components_ptr).Release();
            }

            if self.com_initialized {
                CoUninitialize();
            }
        }
    }
}

fn to_wide_null(value: impl AsRef<OsStr>) -> Vec<u16> {
    let mut wide: Vec<u16> = value.as_ref().encode_wide().collect();
    wide.push(0);
    wide
}

fn get_volume_root(path: &Path) -> Result<String> {
    let wide = to_wide_null(path);
    let mut buffer = vec![0u16; 32768];
    let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
    if ok != 0 {
        let len = buffer.iter().position(|&ch| ch == 0).unwrap_or(0);
        let root = String::from_utf16_lossy(&buffer[..len]);
        if !root.is_empty() {
            return Ok(root);
        }
    }

    let path_str = path.to_string_lossy();
    if let Some(colon) = path_str.find(':') {
        let mut root = path_str[..=colon].to_string();
        if !root.ends_with('\\') {
            root.push('\\');
        }
        Ok(root)
    } else {
        bail!("Invalid path format for VSS volume root: {}", path_str);
    }
}

fn zero_guid() -> VSS_ID {
    VSS_ID {
        Data1: 0,
        Data2: 0,
        Data3: 0,
        Data4: [0; 8],
    }
}

fn check_hr(hr: HRESULT, context: &str) -> Result<()> {
    if FAILED(hr) {
        bail!("{} failed: HRESULT 0x{:08X}", context, hr as u32);
    }
    Ok(())
}

unsafe fn wait_async(async_ptr: *mut IVssAsync, context: &str) -> Result<()> {
    if async_ptr.is_null() {
        bail!("{} returned null async handle", context);
    }

    let result = (|| {
        let hr_wait = (*async_ptr).Wait(u32::MAX);
        if FAILED(hr_wait) {
            bail!("{} wait failed: HRESULT 0x{:08X}", context, hr_wait as u32);
        }

        let mut hr_result: HRESULT = 0;
        let mut reserved = 0;
        let hr_status = (*async_ptr).QueryStatus(&mut hr_result, &mut reserved);
        if FAILED(hr_status) {
            bail!("{} status query failed: HRESULT 0x{:08X}", context, hr_status as u32);
        }
        if FAILED(hr_result) {
            bail!("{} failed: HRESULT 0x{:08X}", context, hr_result as u32);
        }

        Ok(())
    })();

    let _ = (*async_ptr).Release();
    result
}

unsafe fn wide_ptr_to_string(ptr: *const u16) -> Result<String> {
    if ptr.is_null() {
        bail!("Snapshot device path pointer is null");
    }

    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    Ok(String::from_utf16_lossy(slice))
}
