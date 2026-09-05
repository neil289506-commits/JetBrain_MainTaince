#![cfg(windows)]
//! 將原本各 JetBrains 安裝程式在右鍵選單各自建立的「Open Folder as X」項目，
//! 整合成單一的「JetBrains IDEs」階層式（cascading）子選單，內容依目前偵測到的
//! 已安裝 IDE 動態產生。
//!
//! 注意：這些項目通常是各 IDE 安裝程式以系統管理員權限寫入
//! HKEY_CLASSES_ROOT（實際對應到 HKEY_LOCAL_MACHINE\SOFTWARE\Classes），
//! 所以本功能也必須以系統管理員身分執行才能新增/刪除。

use crate::installed::InstalledIde;
use winreg::enums::*;
use winreg::RegKey;

const MENU_KEY_NAME: &str = "JetBrainsIDEs";
const MENU_DISPLAY: &str = "JetBrains IDEs";

const KEYWORDS: &[&str] = &[
    "jetbrains", "intellij", "pycharm", "webstorm", "phpstorm", "clion", "goland",
    "datagrip", "rider", "rustrover", "rubymine", "android studio", "aqua",
];

/// 執行整合。回傳操作紀錄文字，失敗則回傳錯誤訊息（例如非管理員權限）。
pub fn consolidate_context_menu(installed: &[InstalledIde]) -> Result<String, String> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let mut log = String::new();

    let folder_roots = [
        ("Directory\\shell", "%1"),
        ("Directory\\Background\\shell", "%V"),
    ];

    for (base, param) in folder_roots {
        let parent = hkcr
            .open_subkey_with_flags(base, KEY_ALL_ACCESS)
            .map_err(|e| format!("無法開啟登錄機碼 {base}（請確認已用系統管理員身分執行）：{e}"))?;

        remove_old_entries(&parent, base, &mut log)?;
        write_cascade_menu(&parent, base, param, installed, &mut log)?;
    }

    if let Ok(parent) = hkcr.open_subkey_with_flags("*\\shell", KEY_ALL_ACCESS) {
        remove_old_entries(&parent, "*\\shell", &mut log)?;
    }

    Ok(log)
}

fn remove_old_entries(parent: &RegKey, base: &str, log: &mut String) -> Result<(), String> {
    let existing: Vec<String> = parent.enum_keys().filter_map(|r| r.ok()).collect();
    for key_name in existing {
        if key_name == MENU_KEY_NAME {
            continue;
        }
        if looks_like_jetbrains_entry(parent, &key_name) {
            parent
                .delete_subkey_all(&key_name)
                .map_err(|e| format!("刪除舊選單項目失敗：{base}\\{key_name}：{e}"))?;
            log.push_str(&format!("已移除舊選單項目：{base}\\{key_name}\n"));
        }
    }
    Ok(())
}

fn write_cascade_menu(
    parent: &RegKey,
    base: &str,
    param: &str,
    installed: &[InstalledIde],
    log: &mut String,
) -> Result<(), String> {
    parent
        .delete_subkey_all(MENU_KEY_NAME)
        .map_err(|e| format!("清除舊的 {MENU_KEY_NAME} 失敗：{e}"))?;

    if installed.is_empty() {
        log.push_str(&format!("{base}：未偵測到已安裝的 JetBrains IDE，未建立選單。\n"));
        return Ok(());
    }

    let (menu_key, _) = parent
        .create_subkey(MENU_KEY_NAME)
        .map_err(|e| format!("建立選單機碼失敗：{e}"))?;
    menu_key.set_value("", &"").map_err(|e| e.to_string())?;
    menu_key.set_value("MUIVerb", &MENU_DISPLAY).map_err(|e| e.to_string())?;
    menu_key.set_value("Icon", &format!("{}", installed[0].exe_path.display())).map_err(|e| e.to_string())?;
    menu_key
        .set_value("SubCommands", &"")
        .map_err(|e| e.to_string())?;
    menu_key
        .set_value("Position", &"Bottom")
        .map_err(|e| e.to_string())?;

    let (shell_key, _) = menu_key.create_subkey("shell").map_err(|e| e.to_string())?;
    for ide in installed {
        let (item_key, _) = shell_key
            .create_subkey(ide.code)
            .map_err(|e| e.to_string())?;
        item_key.set_value("", &ide.name).map_err(|e| e.to_string())?;
        item_key
            .set_value("Icon", &format!("{}", ide.exe_path.display()))
            .map_err(|e| e.to_string())?;
        item_key
            .set_value("Position", &"Bottom")
            .map_err(|e| e.to_string())?;

        let (cmd_key, _) = item_key.create_subkey("command").map_err(|e| e.to_string())?;
        let command = format!("\"{}\" \"{}\"", ide.exe_path.display(), param);
        cmd_key.set_value("", &command).map_err(|e| e.to_string())?;
    }

    log.push_str(&format!(
        "已於 {base} 建立「{MENU_DISPLAY}」選單，包含 {} 個項目。\n",
        installed.len()
    ));
    Ok(())
}

fn looks_like_jetbrains_entry(parent: &RegKey, key_name: &str) -> bool {
    let Ok(key) = parent.open_subkey(key_name) else {
        return false;
    };

    let display: String = key.get_value("").unwrap_or_default();
    let icon: String = key.get_value("Icon").unwrap_or_default();
    let command: String = key
        .open_subkey("command")
        .ok()
        .and_then(|c| c.get_value::<String, _>("").ok())
        .unwrap_or_default();

    let haystack = format!("{key_name} {display} {icon} {command}").to_lowercase();
    KEYWORDS.iter().any(|k| haystack.contains(k))
}
