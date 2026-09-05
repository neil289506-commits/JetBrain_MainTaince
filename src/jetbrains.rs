use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

/// JetBrains 官方 Product Code 對照表（對應原 Python 腳本 PRODUCT_CODES）
pub const PRODUCT_CODES: &[(&str, &str)] = &[
    ("IU", "IntelliJ IDEA Ultimate"),
    ("IC", "IntelliJ IDEA Community"),
    ("PS", "PhpStorm"),
    ("WS", "WebStorm"),
    ("PY", "PyCharm Professional"),
    ("PC", "PyCharm Community"),
    ("RM", "RubyMine"),
    ("CL", "CLion"),
    ("GO", "GoLand"),
    ("DB", "DataGrip"),
    ("RD", "Rider"),
    ("RR", "RustRover"),
    ("QA", "Aqua"),
    ("AI", "Android Studio"),
];

/// 友善的作業系統 / 架構名稱對照（對應原 Python 腳本 OS_NAMES）
pub const OS_NAMES: &[(&str, &str)] = &[
    ("windows", "Windows (x86_64)"),
    ("windowsARM64", "Windows (ARM64)"),
    ("mac", "macOS (Intel)"),
    ("macM1", "macOS (Apple Silicon)"),
    ("linux", "Linux (x86_64)"),
    ("linuxARM64", "Linux (ARM64)"),
];

#[derive(Debug, Clone, Deserialize)]
pub struct DownloadEntry {
    pub link: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub build: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub downloads: HashMap<String, DownloadEntry>,
}

type ReleasesResponse = HashMap<String, Vec<ReleaseInfo>>;

/// 根據使用者輸入模糊比對 IDE 名稱或代碼（對應原 Python search_ide）
pub fn search_ide(query: &str) -> Vec<&'static str> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return PRODUCT_CODES.iter().map(|(c, _)| *c).collect();
    }
    PRODUCT_CODES
        .iter()
        .filter(|(code, name)| name.to_lowercase().contains(&q) || q == code.to_lowercase())
        .map(|(code, _)| *code)
        .collect()
}

/// 向 JetBrains 官方 API 請求最新 Release 資訊（對應原 Python fetch_ide_info）
pub fn fetch_latest_release(code: &str) -> Result<Option<ReleaseInfo>> {
    let url = format!(
        "https://data.services.jetbrains.com/products/releases?code={code}&latest=true&type=release"
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0")
        .build()?;

    let resp = client.get(&url).send()?.error_for_status()?;
    let data: ReleasesResponse = resp.json()?;

    Ok(data.get(code).and_then(|v| v.first().cloned()))
}
