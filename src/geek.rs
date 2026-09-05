use std::path::PathBuf;
use std::process::Command;

/// 依需求：使用「程式根目錄\Geek.exe」（注意，是根目錄，不是子資料夾）
pub fn geek_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("Geek.exe")
}

/// 開啟 Geek Uninstaller 讓使用者移除指定的 IDE。
///
/// 重要限制：Geek Uninstaller 免費版官方並未公開任何靜默 / 命令列自動移除參數
/// （官方文件僅提到 `/store_apps` 這種與自動化無關的旗標），因此本功能只能做到
/// 「開啟 Geek.exe，方便使用者快速找到並確認移除」，無法做到完全無人值守的靜默解除安裝。
/// 若之後改用付費版 Uninstall Tool（Pro）等具備自動化命令列的版本，可以在這裡擴充參數。
pub fn launch_uninstaller(app_display_name: &str) -> Result<String, String> {
    let geek = geek_path();
    if !geek.exists() {
        return Err(format!(
            "找不到 Geek 解除安裝工具：{}\n請確認 Geek.exe 與本程式放在同一目錄下。",
            geek.display()
        ));
    }

    Command::new(&geek)
        .spawn()
        .map_err(|e| format!("啟動 Geek.exe 失敗：{e}"))?;

    Ok(format!(
        "已開啟 Geek Uninstaller，請在其視窗中找到「{app_display_name}」並手動確認移除\
        （免費版無官方靜默參數，無法全自動完成）。"
    ))
}
