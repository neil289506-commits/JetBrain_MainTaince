#![windows_subsystem = "windows"] // 不跳出黑色主控台視窗，App 內建 Console 面板取代

mod contextmenu;
mod geek;
mod idm;
mod installed;
mod jetbrains;
mod uninstall;
#[cfg(windows)]
mod win;

use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};
use idm::ProgressMsg;
use installed::{detect_installed, DetectSource, InstalledIde};
use jetbrains::{fetch_latest_release, search_ide, ReleaseInfo, OS_NAMES, PRODUCT_CODES};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

#[derive(PartialEq, Clone, Copy)]
enum Step {
    SelectIde,
    Loading,
    SelectArch,
    Downloading,
    Done,
    Failed,
}

struct DownloadApp {
    step: Step,
    query: String,
    matches: Vec<&'static str>,

    release: Option<ReleaseInfo>,
    load_rx: Option<Receiver<Result<Option<ReleaseInfo>, String>>>,

    selected_arch: Option<String>,
    install_dir: Option<PathBuf>,

    progress_rx: Option<Receiver<ProgressMsg>>,
    downloaded: u64,
    total: Option<u64>,
    log: Vec<String>,

    error: Option<String>,

    /// 目前偵測到已安裝的 JetBrains IDE 清單
    installed: Vec<InstalledIde>,
    /// 一般性操作提示（開啟/解除安裝/整合選單的結果）
    notices: Vec<String>,
}

impl Default for DownloadApp {
    fn default() -> Self {
        Self {
            step: Step::SelectIde,
            query: String::new(),
            matches: search_ide(""),
            release: None,
            load_rx: None,
            selected_arch: None,
            install_dir: None,
            progress_rx: None,
            downloaded: 0,
            total: None,
            log: Vec::new(),
            error: None,
            installed: detect_installed(),
            notices: Vec::new(),
        }
    }
}

impl DownloadApp {
    fn pick_ide(&mut self, code: &'static str) {
        self.step = Step::Loading;
        self.error = None;
        let (tx, rx) = channel();
        self.load_rx = Some(rx);
        std::thread::spawn(move || {
            let result = fetch_latest_release(code).map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }

    fn start_download(&mut self) {
        let release = match &self.release {
            Some(r) => r.clone(),
            None => return,
        };
        let arch = match &self.selected_arch {
            Some(a) => a.clone(),
            None => return,
        };
        let dir = match &self.install_dir {
            Some(d) => d.clone(),
            None => return,
        };
        let entry = match release.downloads.get(&arch) {
            Some(e) => e.clone(),
            None => return,
        };

        let (tx, rx) = channel();
        self.progress_rx = Some(rx);
        self.log.clear();
        self.downloaded = 0;
        self.total = entry.size;
        self.step = Step::Downloading;

        idm::start_download(entry.link.clone(), dir, entry.size, tx);
    }

    fn open_ide(&mut self, ide: &InstalledIde) {
        match std::process::Command::new(&ide.exe_path).spawn() {
            Ok(_) => self.notices.push(format!("已啟動 {}", ide.name)),
            Err(e) => self.error = Some(format!("啟動 {} 失敗：{e}", ide.name)),
        }
    }

    /// 優先呼叫 JetBrains 官方留下的 `bin\Uninstall.exe /S` 做真正的靜默解除安裝；
    /// 若找不到（例如 Toolbox 安裝的目錄結構不同），才退回用 Geek.exe 讓使用者手動確認。
    fn uninstall_ide(&mut self, ide: &InstalledIde) {
        match uninstall::uninstall_silently(ide) {
            Ok(msg) => {
                self.notices.push(msg);
                self.error = None;
                return;
            }
            Err(e) => {
                self.notices.push(format!("官方靜默解除安裝無法使用（{e}），改用 Geek.exe 備援。"));
            }
        }

        match geek::launch_uninstaller(ide.name) {
            Ok(msg) => self.notices.push(msg),
            Err(e) => self.error = Some(e),
        }
    }

    #[cfg(windows)]
    fn consolidate_menu(&mut self) {
        match contextmenu::consolidate_context_menu(&self.installed) {
            Ok(log) => {
                for line in log.lines() {
                    self.notices.push(line.to_string());
                }
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
}

impl eframe::App for DownloadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 輪詢版本查詢結果
        if let Some(rx) = &self.load_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(Some(info)) => {
                        self.release = Some(info);
                        self.step = Step::SelectArch;
                    }
                    Ok(None) => {
                        self.error = Some("查無此 IDE 的正式版本資訊。".into());
                        self.step = Step::SelectIde;
                    }
                    Err(e) => {
                        self.error = Some(format!("查詢失敗: {e}"));
                        self.step = Step::SelectIde;
                    }
                }
                self.load_rx = None;
            }
        }

        // 輪詢下載進度
        if let Some(rx) = &self.progress_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ProgressMsg::Log(l) => self.log.push(l),
                    ProgressMsg::Progress { downloaded, total } => {
                        self.downloaded = downloaded;
                        if total.is_some() {
                            self.total = total;
                        }
                    }
                    ProgressMsg::Finished => {
                        self.step = Step::Done;
                        rfd::MessageDialog::new()
                            .set_title("下載完成")
                            .set_description("IDE 安裝檔已下載完成，IDM 已自動關閉。")
                            .set_level(rfd::MessageLevel::Info)
                            .show();
                    }
                    ProgressMsg::Error(e) => {
                        self.error = Some(e);
                        self.step = Step::Failed;
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("JetBrains IDE 自動下載工具");

            ui.horizontal(|ui| {
                if ui.button("重新偵測已安裝的 IDE").clicked() {
                    self.installed = detect_installed();
                    let registry_count = self
                        .installed
                        .iter()
                        .filter(|i| i.source == DetectSource::Registry)
                        .count();
                    self.notices.push(format!(
                        "偵測到 {} 個已安裝的 IDE（其中 {} 個有登錄檔紀錄確認，其餘僅為資料夾比對推測）。",
                        self.installed.len(),
                        registry_count
                    ));
                }

                #[cfg(windows)]
                {
                    if win::is_elevated() {
                        if ui
                            .button("整合右鍵選單「JetBrains IDEs」")
                            .on_hover_text("清除各自獨立的「Open in X」項目，改成單一階層式子選單")
                            .clicked()
                        {
                            self.consolidate_menu();
                        }
                    } else if ui
                        .button("以系統管理員身分重新啟動以整合右鍵選單")
                        .clicked()
                    {
                        match win::relaunch_as_admin() {
                            Ok(_) => std::process::exit(0),
                            Err(e) => self.error = Some(e),
                        }
                    }
                }
            });

            if !self.notices.is_empty() {
                egui::ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                    for n in &self.notices {
                        ui.small(n);
                    }
                });
            }

            ui.separator();

            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::RED, err);
                ui.separator();
            }

            match self.step {
                Step::SelectIde => self.ui_select_ide(ui),
                Step::Loading => {
                    ui.spinner();
                    ui.label("正在查詢最新版本資訊...");
                }
                Step::SelectArch => self.ui_select_arch(ui),
                Step::Downloading => self.ui_downloading(ui),
                Step::Done => self.ui_done(ui),
                Step::Failed => self.ui_failed(ui),
            }
        });

        if self.step == Step::Downloading {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }
}

impl DownloadApp {
    fn ui_select_ide(&mut self, ui: &mut egui::Ui) {
        ui.label("輸入 IDE 名稱或代碼進行搜尋（例如：pycharm、IU、webstorm）");
        if ui.text_edit_singleline(&mut self.query).changed() {
            self.matches = search_ide(&self.query);
        }
        ui.separator();
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for code in self.matches.clone() {
                let name = PRODUCT_CODES
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map(|(_, n)| *n)
                    .unwrap_or(code);

                let installed = self.installed.iter().find(|i| i.code == code).cloned();

                ui.horizontal(|ui| {
                    if let Some(ide) = installed {
                        let badge = match ide.source {
                            DetectSource::Registry => "✅ 已安裝（登錄檔確認）".to_string(),
                            DetectSource::Filesystem => "❓ 疑似已安裝（僅資料夾比對，未在登錄檔找到紀錄）".to_string(),
                            DetectSource::Toolbox => "❓ 疑似已安裝（Toolbox，僅供參考）".to_string(),
                        };
                        ui.label(format!("{name} ({code})　{badge}"));
                        if ui.button("開啟").clicked() {
                            self.open_ide(&ide);
                        }
                        if ui.button("解除安裝").clicked() {
                            self.uninstall_ide(&ide);
                        }
                        if ui.small_button("下載其他版本...").clicked() {
                            self.pick_ide(code);
                        }
                    } else if ui.button(format!("{name} ({code})")).clicked() {
                        self.pick_ide(code);
                    }
                });
            }
            if self.matches.is_empty() {
                ui.label("找不到符合的 IDE。");
            }
        });
    }

    fn ui_select_arch(&mut self, ui: &mut egui::Ui) {
        let release = match self.release.clone() {
            Some(r) => r,
            None => return,
        };

        ui.label(format!(
            "版本: {}    建置: {}    日期: {}",
            release.version.clone().unwrap_or_default(),
            release.build.clone().unwrap_or_default(),
            release.date.clone().unwrap_or_default(),
        ));
        ui.separator();

        ui.label("選擇下載架構：");
        let mut keys: Vec<&String> = release.downloads.keys().collect();
        keys.sort();
        for key in keys {
            let entry = &release.downloads[key];
            let display = OS_NAMES
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| *v)
                .unwrap_or(key.as_str());
            let size_str = entry
                .size
                .map(|s| format!("{:.2} MB", s as f64 / 1024.0 / 1024.0))
                .unwrap_or_else(|| "未知大小".into());

            let checked = self.selected_arch.as_deref() == Some(key.as_str());
            if ui.radio(checked, format!("{display}   ({size_str})")).clicked() {
                self.selected_arch = Some(key.clone());
            }
        }

        ui.separator();
        ui.label("安裝（下載儲存）路徑：");
        ui.horizontal(|ui| {
            let path_str = self
                .install_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "尚未選擇".into());
            ui.label(path_str);
            if ui.button("瀏覽...").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.install_dir = Some(dir);
                }
            }
        });

        ui.separator();
        let ready = self.selected_arch.is_some() && self.install_dir.is_some();
        if ui.add_enabled(ready, egui::Button::new("開始下載")).clicked() {
            self.start_download();
        }
        if ui.button("返回重新選擇 IDE").clicked() {
            self.step = Step::SelectIde;
            self.release = None;
            self.selected_arch = None;
        }
    }

    fn ui_downloading(&mut self, ui: &mut egui::Ui) {
        ui.label("下載中，請稍候...（IDM 已在背景以無人值守模式執行）");
        let progress = match self.total {
            Some(t) if t > 0 => (self.downloaded as f32 / t as f32).min(1.0),
            _ => 0.0,
        };
        ui.add(egui::ProgressBar::new(progress).show_percentage());
        ui.label(format!(
            "{:.2} MB / {}",
            self.downloaded as f64 / 1024.0 / 1024.0,
            self.total
                .map(|t| format!("{:.2} MB", t as f64 / 1024.0 / 1024.0))
                .unwrap_or_else(|| "未知".into())
        ));

        ui.separator();
        ui.label("Console 輸出：");
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.log {
                    ui.monospace(line);
                }
            });
    }

    fn ui_done(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(egui::Color32::GREEN, "下載完成！");
        if let Some(dir) = &self.install_dir {
            ui.label(format!("檔案已儲存至：{}", dir.display()));
        }
        if ui.button("再下載一個").clicked() {
            *self = DownloadApp::default();
        }
    }

    fn ui_failed(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(egui::Color32::RED, "發生錯誤，請參考上方訊息。");
        if ui.button("重試").clicked() {
            self.step = Step::SelectIde;
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([560.0, 660.0]),
        ..Default::default()
    };
    eframe::run_native(
        "JetBrains IDE 自動下載工具",
        options,
        Box::new(|cc| {
            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert(
                "noto_sans_tc_medium".to_owned(),
                FontData::from_static(include_bytes!("../assets/NotoSansTC-Medium.ttf")),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "noto_sans_tc_medium".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("noto_sans_tc_medium".to_owned());
            cc.egui_ctx.set_fonts(fonts);

            Box::new(DownloadApp::default())
        }),
    )
}
