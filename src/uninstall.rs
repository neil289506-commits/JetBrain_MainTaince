use crate::installed::InstalledIde;
use std::path::PathBuf;

/// JetBrains 標準安裝程式（NSIS）都會在安裝目錄的 bin 資料夾留下 Uninstall.exe，
/// 官方文件確認支援 `/S` 做完全靜默（無畫面）解除安裝：
/// https://www.jetbrains.com/help/pycharm/uninstall.html#silent
///
/// 我們偵測到的 `exe_path`（例如 bin\pycharm64.exe）通常跟 Uninstall.exe 在同一個
/// bin 資料夾下，所以直接用它的上層目錄去找。
pub fn find_jetbrains_uninstaller(ide: &InstalledIde) -> Option<PathBuf> {
    let candidate = ide.exe_path.parent()?.join("Uninstall.exe");
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

/// 以系統管理員權限靜默執行：`bin\Uninstall.exe /S`。
/// 只會跳出一次 UAC 提示（僅提權這個解除安裝程式本身，不需要把整個 GUI 都用管理員重開），
/// 執行過程完全不會有畫面，符合無人值守需求。
#[cfg(windows)]
pub fn uninstall_silently(ide: &InstalledIde) -> Result<String, String> {
    let uninstaller = find_jetbrains_uninstaller(ide).ok_or_else(|| {
        format!(
            "在 {} 找不到官方的 bin\\Uninstall.exe（可能是 Toolbox 安裝或非標準目錄結構）。",
            ide.install_dir.display()
        )
    })?;

    crate::win::run_elevated(&uninstaller, "/S")?;

    Ok(format!(
        "已以系統管理員身分靜默執行：\"{}\" /S\n\
         （執行過程不會有任何畫面；完成後請按「重新偵測已安裝的 IDE」確認 {} 已從清單移除。）",
        uninstaller.display(),
        ide.name
    ))
}

#[cfg(not(windows))]
pub fn uninstall_silently(_ide: &InstalledIde) -> Result<String, String> {
    Err("靜默解除安裝僅支援 Windows。".into())
}
