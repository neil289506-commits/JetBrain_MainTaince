#![cfg(windows)]
//! 盡力嘗試隱藏 IDM 彈出的視窗，達到「無安裝程式螢幕」的無人值守效果。
//! IDM 官方命令列並無「完全隱藏視窗」的開關，/n 只能關閉確認對話框，
//! 因此這裡用 Win32 EnumWindows 尋找標題包含 IDM 名稱的視窗並呼叫 ShowWindow(SW_HIDE)。
//! 這是盡力而為（best-effort），不同 IDM 版本視窗標題可能不同，如無效請自行調整比對字串。

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{BOOL, HANDLE, HWND, LPARAM};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, IsWindowVisible, ShowWindow, SW_HIDE, SW_SHOWNORMAL,
};

pub fn hide_idm_windows() {
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(0));
    }
}

unsafe extern "system" fn enum_proc(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    if IsWindowVisible(hwnd).as_bool() {
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
            if title.contains("internet download manager") || title.contains("idman") {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
    BOOL(1) // 非 0 = 繼續列舉
}

/// 檢查目前程式是否以系統管理員權限執行。
/// 整合右鍵選單需要寫入 HKEY_CLASSES_ROOT（對應 HKLM），因此需要管理員權限。
pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
        .is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// 以「系統管理員身分」重新啟動本程式（跳出 UAC 提示），並結束目前這個非提權的行程。
pub fn relaunch_as_admin() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    run_elevated(&exe, "")
}

/// 以「系統管理員身分」執行任意一個外部程式（跳出一次 UAC 提示，只提權這個子行程，
/// 不影響本程式本身），常用於呼叫需要管理員權限的解除安裝程式。
/// 注意：ShellExecuteW 在不同版本的 `windows` crate 中，部分參數是 `Option<HWND>` /
/// `PCWSTR::null()` 的寫法可能略有差異；若編譯錯誤，請依編譯器訊息微調參數型別。
pub fn run_elevated(exe: &std::path::Path, args: &str) -> Result<(), String> {
    let exe_hstr = HSTRING::from(exe.to_string_lossy().to_string());
    let args_hstr = HSTRING::from(args);
    let verb = HSTRING::from("runas");

    unsafe {
        let result = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(exe_hstr.as_ptr()),
            PCWSTR(args_hstr.as_ptr()),
            None,
            SW_SHOWNORMAL,
        );
        // ShellExecuteW 回傳值 <= 32 代表失敗（含使用者在 UAC 對話框按下「否」）
        if (result.0 as isize) <= 32 {
            return Err("使用者取消了系統管理員授權，或啟動失敗。".into());
        }
    }
    Ok(())
}
