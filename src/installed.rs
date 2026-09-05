use crate::jetbrains::PRODUCT_CODES;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InstalledIde {
    pub code: &'static str,
    pub name: &'static str,
    pub install_dir: PathBuf,
    pub exe_path: PathBuf,
    /// 這筆偵測結果的可信度來源
    pub source: DetectSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectSource {
    /// 在 Windows「解除安裝的程式」登錄檔（Uninstall 機碼）裡找到對應紀錄，可信度高
    Registry,
    /// 只在 Program Files\JetBrains 底下用資料夾名稱比對到，登錄檔沒有紀錄，僅供參考
    Filesystem,
    /// 在 JetBrains Toolbox 的 apps 目錄底下比對到，僅供參考
    Toolbox,
}

struct DetectRule {
    code: &'static str,
    /// 名稱（小寫，資料夾名稱或登錄檔 DisplayName 皆適用）必須包含的所有關鍵字
    include: &'static [&'static str],
    /// 不可包含的關鍵字（用來跟同系列的其他版本區分，如 Community）
    exclude: &'static [&'static str],
    /// 執行檔主檔名，會尋找 bin\<stem>64.exe 或 bin\<stem>.exe
    exe_stem: &'static str,
}

/// 依 JetBrains 標準命名習慣做比對（PRODUCT_CODES 顯示名稱 / 安裝資料夾 / 登錄檔
/// DisplayName 三者的命名慣例基本一致，所以同一套規則可以共用）。
const DETECT_RULES: &[DetectRule] = &[
    DetectRule { code: "IU", include: &["intellij"], exclude: &["community"], exe_stem: "idea" },
    DetectRule { code: "IC", include: &["intellij", "community"], exclude: &[], exe_stem: "idea" },
    DetectRule { code: "PY", include: &["pycharm"], exclude: &["community"], exe_stem: "pycharm" },
    DetectRule { code: "PC", include: &["pycharm", "community"], exclude: &[], exe_stem: "pycharm" },
    DetectRule { code: "PS", include: &["phpstorm"], exclude: &[], exe_stem: "phpstorm" },
    DetectRule { code: "WS", include: &["webstorm"], exclude: &[], exe_stem: "webstorm" },
    DetectRule { code: "RM", include: &["rubymine"], exclude: &[], exe_stem: "rubymine" },
    DetectRule { code: "CL", include: &["clion"], exclude: &[], exe_stem: "clion" },
    DetectRule { code: "GO", include: &["goland"], exclude: &[], exe_stem: "goland" },
    DetectRule { code: "DB", include: &["datagrip"], exclude: &[], exe_stem: "datagrip" },
    DetectRule { code: "RD", include: &["rider"], exclude: &[], exe_stem: "rider" },
    DetectRule { code: "RR", include: &["rustrover"], exclude: &[], exe_stem: "rustrover" },
    DetectRule { code: "QA", include: &["aqua"], exclude: &[], exe_stem: "aqua" },
    DetectRule { code: "AI", include: &["android studio"], exclude: &[], exe_stem: "studio" },
];

/// 偵測已安裝的 JetBrains IDE。
/// 優先順序（先找到的優先採用，同一個 code 只保留第一筆）：
///   1. Windows「解除安裝的程式」登錄檔紀錄（最可靠：真的有安裝記錄，不是憑資料夾名稱亂猜）
///   2. Program Files\JetBrains 資料夾名稱比對（備援：登錄檔沒找到時才用）
///   3. JetBrains Toolbox apps 目錄（備援：Toolbox 安裝的不會出現在標準解除安裝清單裡）
pub fn detect_installed() -> Vec<InstalledIde> {
    let mut found: Vec<InstalledIde> = Vec::new();

    #[cfg(windows)]
    detect_from_registry(&mut found);

    detect_from_filesystem(&mut found);
    detect_from_toolbox(&mut found);

    found
}

/// 掃描 HKLM / HKCU 底下的「解除安裝的程式」登錄機碼，比對 DisplayName + Publisher。
#[cfg(windows)]
fn detect_from_registry(found: &mut Vec<InstalledIde>) {
    use winreg::enums::*;
    use winreg::RegKey;

    const UNINSTALL_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_PATH_WOW64: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    let roots = [
        (RegKey::predef(HKEY_LOCAL_MACHINE), UNINSTALL_PATH),
        (RegKey::predef(HKEY_LOCAL_MACHINE), UNINSTALL_PATH_WOW64),
        (RegKey::predef(HKEY_CURRENT_USER), UNINSTALL_PATH),
    ];

    for (root, path) in roots {
        let Ok(uninstall_key) = root.open_subkey(path) else { continue };

        for sub_name in uninstall_key.enum_keys().filter_map(|r| r.ok()) {
            let Ok(entry) = uninstall_key.open_subkey(&sub_name) else { continue };

            let display_name: String = entry.get_value("DisplayName").unwrap_or_default();
            if display_name.is_empty() {
                continue;
            }
            let publisher: String = entry.get_value("Publisher").unwrap_or_default();

            // 先確認發行商 / 名稱有 JetBrains 字樣，避免比對到不相關的軟體
            let confidence_haystack = format!("{display_name} {publisher}").to_lowercase();
            if !confidence_haystack.contains("jetbrains") {
                continue;
            }

            let display_lower = display_name.to_lowercase();
            for rule in DETECT_RULES {
                let include_ok = rule.include.iter().all(|k| display_lower.contains(k));
                let exclude_ok = rule.exclude.iter().all(|k| !display_lower.contains(k));
                if !(include_ok && exclude_ok) {
                    continue;
                }

                let install_location: String = entry.get_value("InstallLocation").unwrap_or_default();
                let uninstall_string: String = entry.get_value("UninstallString").unwrap_or_default();

                let install_dir = if !install_location.trim().is_empty() {
                    Some(PathBuf::from(install_location.trim()))
                } else {
                    install_dir_from_uninstall_string(&uninstall_string)
                };

                let Some(install_dir) = install_dir else { continue };
                let Some(exe) = find_exe_recursive(&install_dir, rule.exe_stem, 2) else { continue };

                push_unique(found, rule.code, install_dir, exe, DetectSource::Registry);
            }
        }
    }
}

/// UninstallString 常見格式如 `"C:\...\bin\Uninstall.exe"`（可能含引號），
/// 上兩層（bin 的上層）就是安裝目錄。
#[cfg(windows)]
fn install_dir_from_uninstall_string(s: &str) -> Option<PathBuf> {
    let cleaned = s.trim().trim_matches('"');
    if cleaned.is_empty() {
        return None;
    }
    let path = PathBuf::from(cleaned);
    let bin_dir = path.parent()?;
    let install_dir = bin_dir.parent()?;
    Some(install_dir.to_path_buf())
}

/// 備援：掃描 Program Files\JetBrains、Program Files (x86)\JetBrains 的資料夾名稱。
/// 只有在該 code 尚未被登錄檔掃描到時才會採用（見 push_unique）。
fn detect_from_filesystem(found: &mut Vec<InstalledIde>) {
    let mut standalone_bases = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        standalone_bases.push(PathBuf::from(pf).join("JetBrains"));
    }
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        standalone_bases.push(PathBuf::from(pf86).join("JetBrains"));
    }

    for base in standalone_bases {
        let Ok(entries) = std::fs::read_dir(&base) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name_lower = entry.file_name().to_string_lossy().to_lowercase();

            for rule in DETECT_RULES {
                let include_ok = rule.include.iter().all(|k| dir_name_lower.contains(k));
                let exclude_ok = rule.exclude.iter().all(|k| !dir_name_lower.contains(k));
                if include_ok && exclude_ok {
                    if let Some(exe) = find_exe_recursive(&path, rule.exe_stem, 2) {
                        push_unique(found, rule.code, path.clone(), exe, DetectSource::Filesystem);
                    }
                }
            }
        }
    }
}

/// 備援：JetBrains Toolbox 的 apps 目錄（Toolbox 安裝的程式不會出現在標準的
/// 「解除安裝的程式」清單，也沒有一致的資料夾命名，只能用 exe 主檔名粗略比對）。
fn detect_from_toolbox(found: &mut Vec<InstalledIde>) {
    let Ok(local) = std::env::var("LOCALAPPDATA") else { return };
    let apps_dir = PathBuf::from(local).join("JetBrains").join("Toolbox").join("apps");
    let Ok(entries) = std::fs::read_dir(&apps_dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        for rule in DETECT_RULES {
            if found.iter().any(|f| f.code == rule.code) {
                continue;
            }
            if let Some(exe) = find_exe_recursive(&path, rule.exe_stem, 6) {
                push_unique(found, rule.code, path.clone(), exe, DetectSource::Toolbox);
            }
        }
    }
}

/// 加入清單，同一個 code 只保留第一筆（呼叫順序已確保 Registry 優先於其他來源）。
fn push_unique(
    found: &mut Vec<InstalledIde>,
    code: &'static str,
    install_dir: PathBuf,
    exe_path: PathBuf,
    source: DetectSource,
) {
    if found.iter().any(|f| f.code == code) {
        return;
    }
    let name = PRODUCT_CODES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or(code);
    found.push(InstalledIde { code, name, install_dir, exe_path, source });
}

fn find_exe_recursive(dir: &Path, stem: &str, max_depth: u32) -> Option<PathBuf> {
    if max_depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    let want_64 = format!("{stem}64.exe");
    let want_plain = format!("{stem}.exe");

    for e in entries.flatten() {
        let p = e.path();
        if p.is_file() {
            if let Some(fname) = p.file_name().and_then(|f| f.to_str()) {
                let fname_lower = fname.to_lowercase();
                if fname_lower == want_64 || fname_lower == want_plain {
                    return Some(p);
                }
            }
        } else if p.is_dir() {
            subdirs.push(p);
        }
    }

    for sd in subdirs {
        if let Some(found) = find_exe_recursive(&sd, stem, max_depth - 1) {
            return Some(found);
        }
    }
    None
}
