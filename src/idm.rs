use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

/// IDM 官方命令列參數（已於官方文件核實）：
///   IDMan.exe /d URL [/p local_path] [/f local_file_name] [/n] [/a] [/q] [/h]
///     /d URL        下載此網址
///     /p local_path 儲存資料夾
///     /f file_name  儲存檔名
///     /n            靜默模式，不跳出確認視窗
///     /a            只加入佇列，不立即下載（本工具不使用）
///     /q            成功下載後自動結束 IDM（只對第一個 IDM 副本有效）
pub enum ProgressMsg {
    Log(String),
    Progress { downloaded: u64, total: Option<u64> },
    Finished,
    Error(String),
}

/// 依需求：使用「程式根目錄\IDM\IDMan.exe」
pub fn idman_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    exe_dir.join("IDM").join("IDMan.exe")
}

pub fn filename_from_url(url: &str) -> String {
    url.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("download.bin")
        .to_string()
}

/// 在背景執行緒中：呼叫 IDM 下載、輪詢檔案大小回報進度、完成後彈窗前置動作 + 關閉 IDM
pub fn start_download(
    url: String,
    install_dir: PathBuf,
    expected_size: Option<u64>,
    tx: Sender<ProgressMsg>,
) {
    thread::spawn(move || {
        let idman = idman_path();
        if !idman.exists() {
            let _ = tx.send(ProgressMsg::Error(format!(
                "找不到 IDM 執行檔: {}\n請確認「IDM」資料夾（內含 IDMan.exe）與本程式放在同一目錄下。",
                idman.display()
            )));
            return;
        }

        if let Err(e) = std::fs::create_dir_all(&install_dir) {
            let _ = tx.send(ProgressMsg::Error(format!("無法建立安裝路徑: {e}")));
            return;
        }

        let filename = filename_from_url(&url);
        let dest_file = install_dir.join(&filename);

        let _ = tx.send(ProgressMsg::Log(format!(
            "呼叫 IDM: \"{}\" /d {} /p \"{}\" /f {} /n",
            idman.display(),
            url,
            install_dir.display(),
            filename
        )));

        let spawned = Command::new(&idman)
            .arg("/d")
            .arg(&url)
            .arg("/p")
            .arg(&install_dir)
            .arg("/f")
            .arg(&filename)
            .arg("/n")
            .spawn();

        let mut child: Child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(ProgressMsg::Error(format!("啟動 IDM 失敗: {e}")));
                return;
            }
        };

        // 背景嘗試隱藏 IDM 跳出的視窗（best-effort，無官方 API 保證）
        #[cfg(windows)]
        {
            thread::spawn(move || {
                for _ in 0..40 {
                    crate::win::hide_idm_windows();
                    thread::sleep(Duration::from_millis(500));
                }
            });
        }

        let total = expected_size.or_else(|| head_content_length(&url));
        if let Some(t) = total {
            let _ = tx.send(ProgressMsg::Log(format!(
                "預期檔案大小: {:.2} MB",
                t as f64 / 1024.0 / 1024.0
            )));
        } else {
            let _ = tx.send(ProgressMsg::Log(
                "無法取得預期檔案大小，將以「檔案是否停止成長」判斷下載完成。".into(),
            ));
        }

        let mut last_size: u64 = 0;
        let mut stable_ticks = 0u32;

        loop {
            thread::sleep(Duration::from_millis(700));

            let current = std::fs::metadata(&dest_file).map(|m| m.len()).unwrap_or(0);
            let _ = tx.send(ProgressMsg::Progress { downloaded: current, total });

            let done_by_size = matches!(total, Some(t) if t > 0 && current >= t);

            if current == last_size && current > 0 {
                stable_ticks += 1;
            } else {
                stable_ticks = 0;
            }
            last_size = current;

            let process_exited = matches!(child.try_wait(), Ok(Some(_)));

            if done_by_size || (stable_ticks >= 4 && current > 0) {
                break;
            }
            if process_exited && current == 0 {
                let _ = tx.send(ProgressMsg::Error(
                    "IDM 已結束但未偵測到下載檔案，請確認 IDM 是否可正常執行、網址是否有效。".into(),
                ));
                return;
            }
            if process_exited && current > 0 {
                // 若剛好搭配 /q 讓 IDM 自行結束，仍視為完成
                break;
            }
        }

        let _ = tx.send(ProgressMsg::Log("下載完成，正在關閉 IDM...".into()));

        let _ = child.kill();
        let _ = child.wait();

        // 保險：強制關閉所有 IDMan.exe（注意：會連帶關閉其他正在使用中的 IDM 視窗）
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/IM", "IDMan.exe", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        let _ = tx.send(ProgressMsg::Finished);
    });
}

fn head_content_length(url: &str) -> Option<u64> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0")
        .build()
        .ok()?;
    let resp = client.head(url).send().ok()?;
    resp.headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}
