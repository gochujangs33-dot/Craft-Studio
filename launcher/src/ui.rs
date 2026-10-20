//! Launcher UI: modern centered card redesign, multi-window launching, dynamic fallback icons, bilingual support.

use crate::apps::{self, AppEntry, Registry, Runner, State, Status};
use crate::theme::*;
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, ComboBox, Frame, Id, LayerId, Layout, Margin, Order, Rect, RichText,
    ScrollArea, Sense, Stroke, StrokeKind, TextEdit, TextureHandle, UiBuilder,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const ICONS: &[(&str, &[u8])] = &[
    ("photocraft", include_bytes!("../assets/icons/photocraft.png")),
    ("vectorcraft", include_bytes!("../assets/icons/vectorcraft.png")),
    ("filmcraft", include_bytes!("../assets/icons/filmcraft.png")),
    ("lightcraft", include_bytes!("../assets/icons/lightcraft.png")),
    ("pdfcraft", include_bytes!("../assets/icons/pdfcraft.png")),
    ("effectcraft", include_bytes!("../assets/icons/effectcraft.png")),
    ("designcraft", include_bytes!("../assets/icons/designcraft.png")),
    ("wordcraft", include_bytes!("../assets/icons/wordcraft.png")),
    ("deckcraft", include_bytes!("../assets/icons/deckcraft.png")),
    ("gridcraft", include_bytes!("../assets/icons/gridcraft.png")),
    ("cadcraft", include_bytes!("../assets/icons/cadcraft.png")),
    ("soundcraft", include_bytes!("../assets/icons/soundcraft.png")),
    ("figcraft", include_bytes!("../assets/icons/figcraft.png")),
    ("hop", include_bytes!("../assets/icons/hop.png")),
];

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Launcher,
    Suite,
}

#[derive(PartialEq, Clone, Copy)]
enum Filter {
    All,
    Classic,
    New,
    Running,
}

enum Action {
    Open(usize),
    Folder(PathBuf),
    Url(String),
    Rescan,
    CheckUpdates,
    Update(usize),
    UpdateAll,
}

pub struct Launcher {
    exe_dir: PathBuf,
    registry: Registry,
    apps_dir: PathBuf,
    saves_dir: PathBuf,
    saves_dir_input: String,
    custom_apps_dir: PathBuf,
    entries: Vec<AppEntry>,
    icons: HashMap<String, TextureHandle>,
    logo: Option<TextureHandle>,
    runner: Runner,
    updater: crate::updater::UpdateManager,
    state: State,
    saved_state: State,
    tab: Tab,
    filter: Filter,
    search: String,
    show_settings: bool,
    show_updates: bool,
    show_progress_modal: bool,
    toasts: Vec<(String, Color32, Instant)>,
    last_poll: Instant,
    pending_files: Vec<PathBuf>,
    launch_after_install: Option<String>,
    companions: HashMap<String, crate::companion::CompanionState>,
    pub active_companion_app: String,
    pub embedded_ai_open: bool,
    manual_check_requested: bool,
}

fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8], max_w: Option<u32>) -> Option<TextureHandle> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if let Some(mw) = max_w {
        if img.width() > mw {
            let h = (img.height() as f32 * mw as f32 / img.width() as f32).round() as u32;
            img = img.resize_exact(mw, h.max(1), image::imageops::FilterType::Lanczos3);
        }
    }
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

impl Launcher {
    pub fn new(cc: &eframe::CreationContext<'_>, exe_dir: PathBuf, file_args: Vec<PathBuf>) -> Self {
        apply(&cc.egui_ctx);
        let registry = apps::load_registry(&exe_dir);
        let apps_dir = apps::find_apps_dir(&exe_dir);
        let state = State::load();
        let saves_dir = state
            .custom_saves_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| apps::find_saves_dir(&exe_dir));
        let saves_dir_input = saves_dir.to_string_lossy().to_string();
        let custom_apps_dir = apps::find_custom_apps_dir(&exe_dir);
        let entries = apps::scan(&registry, &apps_dir, &exe_dir, &saves_dir);
        let icons = ICONS
            .iter()
            .filter_map(|(id, b)| load_texture(&cc.egui_ctx, id, b, None).map(|t| (id.to_string(), t)))
            .collect();
        let logo = load_texture(&cc.egui_ctx, "logo", include_bytes!("../assets/logo-white.png"), Some(480));
        apps::log(&format!(
            "launcher started; apps dir = {}; saves dir = {}; total apps = {}",
            apps_dir.display(),
            saves_dir.display(),
            entries.len()
        ));
        let mut companions: HashMap<String, crate::companion::CompanionState> = HashMap::new();
        for e in &entries {
            let id = e.meta.id.clone();
            let name = e.meta.name().to_string();
            companions.insert(id.clone(), crate::companion::CompanionState::new_for_app(&id, &name));
        }

        let mut launcher = Self {
            exe_dir: exe_dir.clone(),
            registry,
            apps_dir,
            saves_dir,
            saves_dir_input,
            custom_apps_dir,
            entries,
            icons,
            logo,
            runner: Runner::default(),
            updater: crate::updater::UpdateManager::new(exe_dir.clone()),
            saved_state: state.clone(),
            state,
            tab: Tab::Launcher,
            filter: Filter::All,
            search: String::new(),
            show_settings: false,
            show_updates: false,
            show_progress_modal: false,
            toasts: Vec::new(),
            last_poll: Instant::now(),
            pending_files: file_args,
            launch_after_install: None,
            companions,
            active_companion_app: "photocraft".to_string(),
            embedded_ai_open: true,
            manual_check_requested: false,
        };

        // 3M-style Startup Auto-Check: automatically match versions from GitHub on launch
        let repos: Vec<(String, String)> =
            launcher.entries.iter().map(|e| (e.meta.id.clone(), e.meta.repo.clone())).collect();
        launcher.updater.check_all_updates(repos);

        launcher
    }

    fn available_update_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| {
                if let (Some(cur), Some(latest)) = (&e.version, self.updater.latest_version(&e.meta.id)) {
                    crate::updater::is_newer(&latest, cur)
                } else {
                    false
                }
            })
            .count()
    }

    fn is_ko(&self) -> bool {
        self.state.language == "ko"
    }

    fn toast(&mut self, msg: impl Into<String>, color: Color32) {
        self.toasts.push((msg.into(), color, Instant::now()));
    }

    fn set_saves_dir(&mut self, new_path: PathBuf) {
        if let Err(e) = std::fs::create_dir_all(&new_path) {
            let msg = if self.is_ko() {
                format!("폴더를 생성하거나 접근할 수 없습니다: {e}")
            } else {
                format!("Failed to access or create directory: {e}")
            };
            self.toast(msg, ERR);
            return;
        }
        self.saves_dir = new_path.clone();
        self.saves_dir_input = new_path.to_string_lossy().to_string();
        self.state.custom_saves_dir = Some(self.saves_dir_input.clone());
        self.state.save();
        self.saved_state = self.state.clone();

        for e in &mut self.entries {
            let app_save = self.saves_dir.join(e.meta.name());
            let _ = std::fs::create_dir_all(&app_save);
            e.save_dir = app_save;
        }
        let msg = if self.is_ko() {
            format!("작업 파일 저장 위치를 변경했습니다: {}", self.saves_dir.display())
        } else {
            format!("Saved files folder updated: {}", self.saves_dir.display())
        };
        self.toast(msg, OK);
    }

    fn reset_saves_dir(&mut self) {
        let def = apps::find_saves_dir(&self.exe_dir);
        self.saves_dir = def.clone();
        self.saves_dir_input = def.to_string_lossy().to_string();
        self.state.custom_saves_dir = None;
        self.state.save();
        self.saved_state = self.state.clone();

        for e in &mut self.entries {
            let app_save = self.saves_dir.join(e.meta.name());
            let _ = std::fs::create_dir_all(&app_save);
            e.save_dir = app_save;
        }
        let msg = if self.is_ko() {
            "작업 파일 저장 위치를 기본값으로 초기화했습니다."
        } else {
            "Reset saved files folder to default."
        };
        self.toast(msg, OK);
    }

    fn browse_saves_dir(&mut self) {
        #[cfg(windows)]
        {
            let script = r#"[System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms') | Out-Null; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = '작업 파일 저장 위치(SavedFiles) 선택'; $f.ShowNewFolderButton = $true; if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { Write-Output $f.SelectedPath }"#;
            let output = std::process::Command::new("powershell")
                .creation_flags(0x08000000)
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .output();
            if let Ok(out) = output {
                let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path_str.is_empty() {
                    self.set_saves_dir(PathBuf::from(path_str));
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            let script = r#"POSIX path of (choose folder with prompt "작업 파일 저장 위치(SavedFiles) 선택")"#;
            let output = std::process::Command::new("osascript")
                .args(["-e", script])
                .output();
            if let Ok(out) = output {
                let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path_str.is_empty() {
                    self.set_saves_dir(PathBuf::from(path_str));
                }
            }
        }
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            if let Ok(out) = std::process::Command::new("zenity")
                .args(["--file-selection", "--directory", "--title=Select SavedFiles Directory"])
                .output()
            {
                let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path_str.is_empty() {
                    self.set_saves_dir(PathBuf::from(path_str));
                }
            }
        }
    }

    fn refresh_status(&mut self) {
        self.runner.poll();
        for e in &mut self.entries {
            e.status = match (&e.exe, self.runner.is_running(&e.meta.id)) {
                (None, _) => Status::NotInstalled,
                (Some(_), true) => Status::Running,
                (Some(_), false) => Status::Installed,
            };
        }
    }

    /// Launches an independent instance (supports opening multiple windows of the same or different apps).
    fn launch(&mut self, ctx: &egui::Context, idx: usize, file: Option<&Path>) {
        let (id, name, exe, repo, dir, save_dir) = {
            let e = &self.entries[idx];
            (e.meta.id.clone(), e.meta.name(), e.exe.clone(), e.meta.repo.clone(), e.dir.clone(), e.save_dir.clone())
        };
        let Some(exe) = exe else {
            self.launch_after_install = Some(id.clone());
            // Automatically trigger in-app download and installation then auto-run!
            if self.updater.update_app(&id, &repo, &dir) {
                let msg = if self.is_ko() {
                    format!("{name} 실행을 위해 준비 중입니다. 완료 즉시 자동 실행됩니다...")
                } else {
                    format!("Preparing {name}... Will launch automatically when ready.")
                };
                self.toast(msg, OK);
                self.show_progress_modal = true;
            } else {
                let msg = if self.is_ko() {
                    format!("{name} 실행 준비가 이미 진행 중입니다.")
                } else {
                    format!("{name} preparation is already in progress.")
                };
                self.toast(msg, OK);
            }
            return;
        };

        // Always launch as a new independent process (multi-window multitasking)
        match self.runner.launch(&id, &exe, &save_dir, file) {
            Ok(pid) => {
                apps::log(&format!("launched {id} (pid {pid}) exe={exe:?} file={file:?} save_dir={save_dir:?}"));
                self.active_companion_app = id.clone();
                self.embedded_ai_open = true;
                let comp = self.companions.entry(id.clone()).or_insert_with(|| crate::companion::CompanionState::new_for_app(&id, &name));
                comp.is_open = true;
                comp.is_minimized = false;
                comp.api_key = self.state.gemini_api_key.clone();
                comp.provider = crate::companion::AiProvider::from_u8(self.state.ai_provider);
                self.state.last_opened.insert(id.clone(), apps::now_secs());
                let run_count = self.runner.pids(&id).len();
                let what = if self.is_ko() {
                    if run_count > 1 {
                        format!("{name} 새 창 실행 (현재 {run_count}개 창)")
                    } else {
                        format!("{name} 실행 완료")
                    }
                } else {
                    if run_count > 1 {
                        format!("Opened new window for {name} ({run_count} running)")
                    } else {
                        format!("Launched {name}")
                    }
                };
                self.toast(what, OK);
                if self.state.minimize_on_open {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            }
            Err(err) => {
                apps::log(&format!("failed to open {id}: {err}"));
                let msg = if self.is_ko() {
                    format!("{name} 실행 실패: {err}")
                } else {
                    format!("Could not open {name}: {err}")
                };
                self.toast(msg, ERR);
            }
        }
        self.refresh_status();
    }

    fn open_file(&mut self, ctx: &egui::Context, path: &Path) {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        match self.entries.iter().position(|e| e.meta.extensions.iter().any(|x| *x == ext)) {
            Some(idx) => {
                if ext == "indd" || ext == "indt" {
                    let tip = if self.is_ko() {
                        "💡 인디자인 파일은 어도비 인디자인에서 '내보내기(Export) ▸ IDML(*.idml)'로 저장된 파일이 가장 정확하게 호환됩니다."
                    } else {
                        "💡 InDesign files open best when exported as InDesign Markup (.idml)."
                    };
                    self.toast(tip, MUTED);
                }
                self.launch(ctx, idx, Some(path));
            }
            None => {
                let msg = if self.is_ko() {
                    format!(".{ext} 확장자를 열 수 있는 Craft 앱이 없습니다.")
                } else {
                    format!("No Craft app opens .{ext} files")
                };
                self.toast(msg, ERR);
            }
        }
    }

    fn apply_action(&mut self, ctx: &egui::Context, a: Action) {
        match a {
            Action::Open(i) => self.launch(ctx, i, None),
            Action::Folder(p) => {
                let _ = std::fs::create_dir_all(&p);
                #[cfg(windows)]
                let res = std::process::Command::new("explorer").arg(&p).spawn();
                #[cfg(target_os = "macos")]
                let res = std::process::Command::new("open").arg(&p).spawn();
                #[cfg(all(not(windows), not(target_os = "macos")))]
                let res = std::process::Command::new("xdg-open").arg(&p).spawn();
                if let Err(e) = res {
                    self.toast(format!("Could not open folder: {e}"), ERR);
                }
            }
            Action::Url(u) => ctx.open_url(egui::OpenUrl::new_tab(u)),
            Action::Rescan => {
                self.entries = apps::scan(&self.registry, &self.apps_dir, &self.exe_dir, &self.saves_dir);
                self.refresh_status();
                let n = self.entries.len();
                let msg = if self.is_ko() {
                    format!("앱 및 저장 폴더 다시 스캔: 총 {n}개 프로그램 등록됨")
                } else {
                    format!("Rescanned apps: {n} apps loaded")
                };
                self.toast(msg, MUTED);
            }
            Action::CheckUpdates => {
                self.manual_check_requested = true;
                let repos: Vec<(String, String)> =
                    self.entries.iter().map(|e| (e.meta.id.clone(), e.meta.repo.clone())).collect();
                self.updater.check_all_updates(repos);
                let msg = if self.is_ko() {
                    "최신 버전 정보를 확인하고 있습니다..."
                } else {
                    "Checking GitHub for latest app releases..."
                };
                self.toast(msg, OK);
            }
            Action::Update(i) => {
                let e = &self.entries[i];
                let name = e.meta.name();
                if self.runner.is_running(&e.meta.id) {
                    let msg = if self.is_ko() {
                        format!("{name} 실행 중입니다. 프로그램을 종료한 후 업데이트를 진행해주세요.")
                    } else {
                        format!("{name} is currently running. Please close it before updating.")
                    };
                    self.toast(msg, ERR);
                } else {
                    self.show_progress_modal = true;
                    self.updater.update_app(&e.meta.id, &e.meta.repo, &e.dir);
                    let msg = if self.is_ko() {
                        format!("{name} 최신 버전 다운로드 및 업데이트 진행 중...")
                    } else {
                        format!("Updating {name} to latest version...")
                    };
                    self.toast(msg, OK);
                }
            }
            Action::UpdateAll => {
                let mut started = 0;
                for (_i, e) in self.entries.iter().enumerate() {
                    if !self.runner.is_running(&e.meta.id) && !self.updater.is_updating(&e.meta.id) {
                        self.updater.update_app(&e.meta.id, &e.meta.repo, &e.dir);
                        started += 1;
                    }
                }
                if started > 0 {
                    self.show_progress_modal = true;
                }
                let msg = if self.is_ko() {
                    format!("{started}개 프로그램 일괄 업데이트 시작...")
                } else {
                    format!("Started updating {started} apps...")
                };
                self.toast(msg, OK);
            }
        }
    }

    fn visible(&self) -> Vec<usize> {
        let q = self.search.trim().to_lowercase();
        let lang = &self.state.language;
        let mut v: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                let pass = match self.filter {
                    Filter::All => true,
                    Filter::Classic => e.meta.group == "classic",
                    Filter::New => e.meta.group == "new",
                    Filter::Running => e.status == Status::Running,
                };
                pass && (q.is_empty()
                    || e.meta.name().to_lowercase().contains(&q)
                    || e.meta.tagline(lang).to_lowercase().contains(&q)
                    || e.meta.like(lang).to_lowercase().contains(&q)
                    || e.meta.extensions.iter().any(|x| *x == q.trim_start_matches('.')))
            })
            .collect();
        match self.state.sort {
            1 => v.sort_by_key(|&i| self.entries[i].meta.name()),
            2 => v.sort_by_key(|&i| {
                std::cmp::Reverse(self.state.last_opened.get(&self.entries[i].meta.id).copied().unwrap_or(0))
            }),
            _ => v.sort_by_key(|&i| self.entries[i].meta.order),
        }
        v
    }

    // ------------------------------------------------------------ top / bottom bars

    fn top_bar(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let ko = self.is_ko();
        egui::TopBottomPanel::top("top")
            .exact_height(46.0_f32)
            .frame(Frame::new().fill(BG).stroke(Stroke::new(1.0_f32, LINE)).inner_margin(Margin::symmetric(16, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(l) = &self.logo {
                        let s = l.size_vec2();
                        let h = 13.0_f32;
                        ui.image((l.id(), vec2(s.x * h / s.y, h)));
                    }
                    ui.add_space(18.0_f32);
                    let l_txt = if ko { "런처 홈" } else { "LAUNCHER" };
                    let s_txt = if ko { "스위트 정보" } else { "CRAFT APP SUITE" };
                    if chip(ui, l_txt, self.tab == Tab::Launcher, false).clicked() {
                        self.tab = Tab::Launcher;
                    }
                    if chip(ui, s_txt, self.tab == Tab::Suite, false).clicked() {
                        self.tab = Tab::Suite;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Settings button
                        let set_tooltip = if ko { "환경설정" } else { "Settings" };
                        if chip(ui, "⚙", self.show_settings, false).on_hover_text(set_tooltip).clicked() {
                            self.show_settings = !self.show_settings;
                        }

                        // AI Companion toggle button (Controls in-app companion panel on launch)
                        let any_open = self.state.auto_show_companion;
                        let ai_lbl = if any_open { "🤖 앱 내장 AI 패널 (켜짐)" } else { "🤖 앱 내장 AI 패널 (꺼짐)" };
                        let ai_tooltip = if ko { "PhotoCraft 등 프로그램 실행 시 우측 하단 내장 AI 채팅창 표시 설정" } else { "Toggle in-app AI companion panel on app launch" };
                        if chip(ui, ai_lbl, any_open, false).on_hover_text(ai_tooltip).clicked() {
                            self.state.auto_show_companion = !self.state.auto_show_companion;
                            for c in self.companions.values_mut() {
                                c.is_open = self.state.auto_show_companion;
                            }
                            self.state.save();
                        }

                        // Update Manager button with auto-detected badge
                        let upd_count = self.available_update_count();
                        let is_upd = self.updater.has_active_updates();
                        let upd_lbl = if is_upd {
                            if ko { "⏳ 업데이트 중...".to_string() } else { "⏳ Updating...".to_string() }
                        } else if upd_count > 0 {
                            if ko { format!("🔄 업데이트 ({upd_count})") } else { format!("🔄 UPDATE ({upd_count})") }
                        } else {
                            if ko { "🔄 업데이트".to_string() } else { "🔄 UPDATE".to_string() }
                        };
                        let upd_tooltip = if ko {
                            if upd_count > 0 {
                                format!("{upd_count}개의 새로운 업데이트가 있습니다. 클릭하여 업데이트 관리자 열기")
                            } else {
                                "소프트웨어 업데이트 관리자 열기".to_string()
                            }
                        } else {
                            if upd_count > 0 {
                                format!("{upd_count} update(s) available. Click to open update center.")
                            } else {
                                "Open software update center".to_string()
                            }
                        };
                        let is_highlighted = self.show_updates || is_upd || upd_count > 0;
                        if chip(ui, &upd_lbl, is_highlighted, false).on_hover_text(upd_tooltip).clicked() {
                            self.show_updates = !self.show_updates;
                        }

                        // Rescan button
                        let rescan_tooltip = if ko { "앱 및 폴더 다시 스캔" } else { "Rescan apps and folders" };
                        if chip(ui, "⟳", false, false).on_hover_text(rescan_tooltip).clicked() {
                            actions.push(Action::Rescan);
                        }

                        // Open Saves Folder quick button
                        let saves_tooltip = if ko { "작업 저장 폴더(SavedFiles) 열기" } else { "Open SavedFiles folder" };
                        let saves_lbl = if ko { "📁 저장폴더" } else { "📁 SAVES" };
                        if chip(ui, saves_lbl, false, true).on_hover_text(saves_tooltip).clicked() {
                            actions.push(Action::Folder(self.saves_dir.clone()));
                        }

                        // Language Toggle: KR / EN
                        let lang_lbl = if ko { "🌐 한국어" } else { "🌐 English" };
                        if chip(ui, lang_lbl, true, false).on_hover_text(if ko { "클릭하여 영문으로 변경" } else { "Click to switch to Korean" }).clicked() {
                            self.state.language = if ko { "en".to_string() } else { "ko".to_string() };
                            self.toast(
                                if self.is_ko() { "언어를 한국어로 변경했습니다." } else { "Language switched to English." },
                                OK,
                            );
                        }

                        ui.add_space(8.0_f32);

                        // Table / Grid Toggle
                        let tbl_txt = if ko { "목록형" } else { "TABLE" };
                        let grd_txt = if ko { "그리드형" } else { "GRID" };
                        if chip(ui, tbl_txt, self.state.table_view, false).clicked() {
                            self.state.table_view = true;
                        }
                        if chip(ui, grd_txt, !self.state.table_view, false).clicked() {
                            self.state.table_view = false;
                        }
                    });
                });
            });
    }

    fn status_bar(&self, ctx: &egui::Context) {
        let ko = self.is_ko();
        let running = self.entries.iter().filter(|e| e.status == Status::Running).count();
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0_f32)
            .frame(Frame::new().fill(BG).stroke(Stroke::new(1.0_f32, LINE)).inner_margin(Margin::symmetric(16, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let dir_lbl = if ko { "기본 저장 폴더" } else { "SAVED FILES" };
                    ui.label(RichText::new(dir_lbl).font(mono(10.0_f32)).color(DIM));
                    ui.label(RichText::new(self.saves_dir.display().to_string()).font(mono(10.0_f32)).color(OK));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let status_str = if ko {
                            format!(
                                "{} v{}  ·  총 {}개 프로그램  ·  실행 중: {}",
                                self.registry.suite,
                                env!("CARGO_PKG_VERSION"),
                                self.entries.len(),
                                running
                            )
                        } else {
                            format!(
                                "{} v{}  ·  {} APPS  ·  {} RUNNING",
                                self.registry.suite.to_uppercase(),
                                env!("CARGO_PKG_VERSION"),
                                self.entries.len(),
                                running
                            )
                        };
                        ui.label(RichText::new(status_str).font(mono(10.0_f32)).color(DIM));
                    });
                });
            });
    }

    // ------------------------------------------------------------ launcher page

    fn launcher_view(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ko = self.is_ko();
        let total = self.entries.len();
        let running = self.entries.iter().filter(|e| e.status == Status::Running).count();

        // Smart Update Notification Banner: One-Click update right from main screen!
        let upd_count = self.available_update_count();
        let is_updating = self.updater.has_active_updates();
        if upd_count > 0 || is_updating {
            let banner_bg = Color32::from_rgb(0x2A, 0x1F, 0x0A);
            let banner_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0xD4, 0x90, 0x1A));

            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 36.0_f32),
                Layout::left_to_right(Align::Center),
                |ui| {
                    Frame::new()
                        .fill(banner_bg)
                        .stroke(banner_stroke)
                        .corner_radius(6.0_f32)
                        .inner_margin(Margin::symmetric(14, 6))
                        .show(ui, |ui| {
                            ui.set_height(24.0_f32);
                            ui.horizontal(|ui| {
                                if is_updating {
                                    let (pct, desc) = self.updater.overall_progress().unwrap_or((0.15, if ko { "업데이트 진행 중...".to_string() } else { "Updating...".to_string() }));
                                    let pct_100 = (pct * 100.0) as u32;

                                    ui.label(RichText::new("⏳").size(15.0_f32));
                                    let msg = if ko {
                                        format!("소프트웨어 업데이트 진행 중 ({pct_100}%) · {desc}")
                                    } else {
                                        format!("Updating software ({pct_100}%) · {desc}")
                                    };
                                    ui.label(RichText::new(msg).font(mono(11.0_f32)).strong().color(Color32::from_rgb(0xF5, 0xC0, 0x40)));

                                    ui.add_space(8.0_f32);
                                    let pb = egui::ProgressBar::new(pct)
                                        .show_percentage()
                                        .fill(Color32::from_rgb(0xF5, 0xC0, 0x40))
                                        .corner_radius(3.0_f32);
                                    ui.add_sized(vec2(160.0_f32, 16.0_f32), pb);

                                    let remaining = ui.available_width() - 95.0_f32;
                                    if remaining > 0.0_f32 {
                                        ui.add_space(remaining);
                                    }
                                    if chip(ui, if ko { "3M 팝업 열기" } else { "Open Dialog" }, false, false).clicked() {
                                        self.show_progress_modal = true;
                                    }
                                } else {
                                    ui.label(RichText::new("💡").size(15.0_f32));
                                    let msg = if ko {
                                        format!("{upd_count}개의 새로운 소프트웨어 업데이트가 있습니다.")
                                    } else {
                                        format!("{upd_count} new software update(s) available.")
                                    };
                                    ui.label(RichText::new(msg).font(mono(11.0_f32)).color(Color32::from_rgb(0xF5, 0xC0, 0x40)));

                                    let btn_lbl = if ko {
                                        format!("⚡ 지금 전체 자동 업데이트 ({upd_count})")
                                    } else {
                                        format!("⚡ Update All ({upd_count})")
                                    };
                                    let details_lbl = if ko { "상세 보기" } else { "Details" };

                                    // Spacing to right align buttons cleanly without stretching height
                                    let btn_w = 210.0_f32;
                                    let det_w = 70.0_f32;
                                    let remaining = ui.available_width() - btn_w - det_w - 14.0_f32;
                                    if remaining > 0.0_f32 {
                                        ui.add_space(remaining);
                                    }

                                    let btn = egui::Button::new(RichText::new(btn_lbl).font(mono(10.5_f32)).strong().color(Color32::from_rgb(0x18, 0x12, 0x05)))
                                        .fill(Color32::from_rgb(0xF5, 0xC0, 0x40))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xFF, 0xD4, 0x60)))
                                        .corner_radius(4.0_f32);
                                    if ui.add(btn).clicked() {
                                        actions.push(Action::UpdateAll);
                                    }

                                    if chip(ui, details_lbl, false, false).clicked() {
                                        self.show_progress_modal = true;
                                    }
                                }
                            });
                        });
                },
            );
            ui.add_space(8.0_f32);
        }

        ui.horizontal(|ui| {
            let filter_labels = if ko {
                [
                    (Filter::All, format!("전체 {total}")),
                    (Filter::Classic, "클래식 7".to_string()),
                    (Filter::New, format!("신규 {}", total - 7)),
                    (Filter::Running, format!("실행 중 {running}")),
                ]
            } else {
                [
                    (Filter::All, format!("ALL {total}")),
                    (Filter::Classic, "CLASSIC 7".to_string()),
                    (Filter::New, format!("NEW {}", total - 7)),
                    (Filter::Running, format!("RUNNING {running}")),
                ]
            };

            for (f, label) in filter_labels {
                if chip(ui, &label, self.filter == f, true).clicked() {
                    self.filter = f;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let hint = if ko { "앱 이름 또는 .확장자 검색" } else { "Search apps or .ext" };
                ui.add(
                    TextEdit::singleline(&mut self.search)
                        .hint_text(RichText::new(hint).color(DIM))
                        .desired_width(210.0_f32)
                        .margin(Margin::symmetric(8, 5)),
                );
                let labels = if ko {
                    ["스위트 기본순", "이름순", "최근 실행순"]
                } else {
                    ["Suite order", "Name", "Recently opened"]
                };
                ComboBox::from_id_salt("sort")
                    .selected_text(labels[self.state.sort.min(2) as usize])
                    .width(140.0_f32)
                    .show_ui(ui, |ui| {
                        for (i, l) in labels.iter().enumerate() {
                            ui.selectable_value(&mut self.state.sort, i as u8, *l);
                        }
                    });
                let sort_lbl = if ko { "정렬" } else { "SORT" };
                ui.label(RichText::new(sort_lbl).font(mono(10.0_f32)).color(DIM));
            });
        });
        ui.add_space(14.0_f32);

        let visible = self.visible();
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if visible.is_empty() {
                ui.add_space(60.0_f32);
                ui.vertical_centered(|ui| {
                    let no_app_msg = if ko { "일치하는 앱이 없습니다" } else { "NO APPS MATCH" };
                    let no_app_sub = if ko { "다른 검색어나 필터를 선택해보세요." } else { "Try a different filter or search term." };
                    ui.label(RichText::new(no_app_msg).font(mono(12.0_f32)).color(MUTED));
                    ui.label(RichText::new(no_app_sub).color(DIM));
                });
                return;
            }
            if self.state.table_view {
                self.table(ui, &visible, actions);
            } else if self.state.sort == 0 && self.filter == Filter::All {
                let sections = if ko {
                    [("classic", "클래식 크리에이티브 프로그램"), ("new", "신규 프로덕티비티 프로그램")]
                } else {
                    [("classic", "CLASSIC CREATIVE APPS"), ("new", "NEW PRODUCTIVITY APPS")]
                };
                for (n, (group, title)) in sections.iter().enumerate() {
                    let idx: Vec<usize> =
                        visible.iter().copied().filter(|&i| self.entries[i].meta.group == *group).collect();
                    if idx.is_empty() {
                        continue;
                    }
                    section_header(ui, &format!("{:02} / {title}", n + 1), idx.len(), ko);
                    self.grid(ui, &idx, actions);
                    ui.add_space(22.0_f32);
                }
            } else {
                let all_title = if ko { "프로그램 목록" } else { "APPS" };
                section_header(ui, all_title, visible.len(), ko);
                self.grid(ui, &visible, actions);
            }
        });
    }

    fn grid(&self, ui: &mut egui::Ui, idx: &[usize], actions: &mut Vec<Action>) {
        let gap = 14.0_f32;
        let avail = (ui.available_width() - 2.0_f32).max(280.0_f32);
        let target_w = 216.0_f32;
        let cols = (((avail + gap) / (target_w + gap)).floor() as usize).clamp(2, 6);
        let total_gap = gap * (cols - 1) as f32;
        let w = ((avail - total_gap) / cols as f32).floor();
        let h = 284.0_f32;

        for row in idx.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(gap, 0.0_f32);
                for &i in row {
                    self.card(ui, i, vec2(w, h), actions);
                }
            });
            ui.add_space(gap);
        }
    }

    /// Redesigned modern vertical centered card
    fn card(&self, ui: &mut egui::Ui, i: usize, size: egui::Vec2, actions: &mut Vec<Action>) {
        let ko = self.is_ko();
        let lang = &self.state.language;
        let e = &self.entries[i];
        let accent = hex(&e.meta.accent);
        let (rect, card_resp) = ui.allocate_exact_size(size, Sense::hover());
        
        // Show detailed description tooltip on card hover
        card_resp.on_hover_ui(|ui| {
            Frame::new()
                .fill(Color32::from_rgb(0x14, 0x14, 0x18))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2E, 0x2E, 0x38)))
                .corner_radius(6.0_f32)
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    show_app_tooltip(ui, e, lang, ko);
                });
        });

        let hovered = ui.rect_contains_pointer(rect);
        let t = ui.ctx().animate_bool_with_time(Id::new(("card_anim", i)), hovered, 0.15_f32);

        let p = ui.painter();
        let rounding = 8.0_f32;
        p.rect_filled(rect, rounding, lerp(CARD, CARD_HOVER, t));
        p.rect_stroke(rect, rounding, Stroke::new(1.0_f32, lerp(LINE, accent.gamma_multiply(0.6_f32), t)), StrokeKind::Inside);
        
        // Top accent line on hover
        if t > 0.0_f32 {
            p.line_segment(
                [rect.left_top() + vec2(8.0_f32, 1.0_f32), rect.right_top() + vec2(-8.0_f32, 1.0_f32)],
                Stroke::new(2.5_f32, accent.gamma_multiply(t)),
            );
        }

        let inner = rect.shrink(12.0_f32);
        let mut cui = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Center)));
        cui.spacing_mut().item_spacing = vec2(0.0_f32, 3.0_f32);

        // Header: Order badge & Menu dots
        cui.horizontal(|ui| {
            number_badge(ui, e.meta.order, accent);
            if let Some(v) = &e.version {
                ui.add_space(2.0_f32);
                ui.label(RichText::new(format!("v{v}")).font(mono(8.0_f32)).color(DIM));
            }
            if e.is_custom_mod {
                ui.add_space(2.0_f32);
                Frame::new()
                    .fill(Color32::from_rgb(0x40, 0x15, 0x50))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xB4, 0x6C, 0xF7)))
                    .inner_margin(Margin::symmetric(4, 1))
                    .show(ui, |ui| {
                        let txt = if ko { "수정본" } else { "MOD" };
                        ui.label(RichText::new(txt).font(mono(8.0_f32)).color(Color32::from_rgb(0xD8, 0x96, 0xFF)));
                    });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button(RichText::new("···").color(MUTED), |ui| {
                    ui.set_min_width(220.0_f32);
                    let save_folder_txt = if ko { format!("📁 {} 저장 폴더 열기", e.meta.name()) } else { format!("📁 Open {} Save Folder", e.meta.name()) };
                    if ui.button(save_folder_txt).clicked() {
                        actions.push(Action::Folder(e.save_dir.clone()));
                        ui.close_menu();
                    }
                    let bin_folder_txt = if ko { "📂 실행 파일 위치 열기" } else { "📂 Show app folder" };
                    if ui.button(bin_folder_txt).clicked() {
                        actions.push(Action::Folder(e.dir.clone()));
                        ui.close_menu();
                    }
                    ui.separator();
                    let upd_txt = if ko { format!("🔄 {} 업데이트", e.meta.name()) } else { format!("🔄 Update {}", e.meta.name()) };
                    if ui.button(upd_txt).clicked() {
                        actions.push(Action::Update(i));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(if ko { "깃허브 저장소 열기" } else { "GitHub repository" }).clicked() {
                        actions.push(Action::Url(e.meta.repo.clone()));
                        ui.close_menu();
                    }
                });
            });
        });

        cui.add_space(4.0_f32);

        // Big Centered Icon (64px + dynamic fallback if PNG is missing)
        let icon_size = 64.0_f32 + 4.0_f32 * t;
        if let Some(tex) = self.icons.get(&e.meta.id) {
            cui.image((tex.id(), vec2(icon_size, icon_size)));
        } else {
            // Dynamic modern fallback glyph icon (ensures any newly added program automatically looks great!)
            let (icon_rect, _) = cui.allocate_exact_size(vec2(icon_size, icon_size), Sense::hover());
            let p = cui.painter();
            p.rect_filled(icon_rect, 10.0_f32, accent.gamma_multiply(0.18_f32));
            p.rect_stroke(icon_rect, 10.0_f32, Stroke::new(1.5_f32, accent), StrokeKind::Inside);
            let prefix_2: String = e.meta.prefix.chars().take(2).collect();
            p.text(icon_rect.center(), Align2::CENTER_CENTER, prefix_2, mono(24.0_f32), accent);
        }

        cui.add_space(6.0_f32);

        // App Name Centered: Prefix (White) + Craft (Accent color)
        cui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0_f32;
            ui.label(RichText::new(&e.meta.prefix).size(18.0_f32).strong().color(INK));
            ui.label(RichText::new("Craft").size(18.0_f32).strong().color(accent));
            if e.meta.group == "new" {
                ui.add_space(4.0_f32);
                Frame::new()
                    .stroke(Stroke::new(1.0_f32, accent.gamma_multiply(0.6_f32)))
                    .inner_margin(Margin::symmetric(3, 1))
                    .show(ui, |ui| ui.label(RichText::new("NEW").font(mono(7.5_f32)).color(accent)));
            }
        });

        // Alternative to tag (e.g. Adobe Photoshop 대체)
        cui.add(egui::Label::new(RichText::new(e.meta.like(lang)).size(11.0_f32).color(accent.gamma_multiply(0.9_f32))).truncate());

        // Brief Tagline (One line description)
        cui.add(egui::Label::new(RichText::new(e.meta.tagline(lang)).size(11.0_f32).color(MUTED)).truncate());

        // Running status indicator (No 'installed' badge!)
        let running_pids = self.runner.pids(&e.meta.id);
        if !running_pids.is_empty() {
            cui.add_space(2.0_f32);
            running_badge(&mut cui, running_pids.len(), accent, ko);
        }

        // Footer Launch / Updating Button pinned to bottom
        let btn_h = 32.0_f32;
        let btn_rect = Rect::from_min_max(pos2(inner.left(), inner.bottom() - btn_h), inner.max);
        let mut bui = ui.new_child(UiBuilder::new().max_rect(btn_rect).layout(Layout::left_to_right(Align::Center)));

        let is_updating = self.updater.is_updating(&e.meta.id);
        if is_updating {
            let _ = bui.add_enabled(
                false,
                egui::Button::new(RichText::new(if ko { "⏳ 실행 준비 중..." } else { "⏳ Preparing..." }).font(mono(11.0_f32)).color(Color32::from_rgb(0xF5, 0xC0, 0x40)))
                    .fill(Color32::from_rgb(0x35, 0x22, 0x05))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xE5, 0xA0, 0x20)))
                    .corner_radius(4.0_f32)
                    .min_size(vec2(btn_rect.width(), btn_h)),
            );
        } else {
            // Always display direct Launch button across all apps for unified UX
            let fill = lerp(Color32::from_rgb(0x18, 0x18, 0x1C), accent, t);
            let fg = if t > 0.4_f32 { BG } else { INK };
            let btn_label = if ko { "▶  실행" } else { "▶  LAUNCH" };

            let open_btn = bui.add(
                egui::Button::new(RichText::new(btn_label).font(mono(11.0_f32)).strong().color(fg))
                    .fill(fill)
                    .stroke(Stroke::new(1.0_f32, lerp(LINE, accent, t)))
                    .corner_radius(4.0_f32)
                    .min_size(vec2(btn_rect.width(), btn_h)),
            );
            if open_btn.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                actions.push(Action::Open(i));
            }
        }
    }

    fn table(&self, ui: &mut egui::Ui, idx: &[usize], actions: &mut Vec<Action>) {
        let ko = self.is_ko();
        let lang = &self.state.language;
        Frame::new().stroke(Stroke::new(1.0_f32, LINE)).inner_margin(Margin::same(14)).show(ui, |ui| {
            egui::Grid::new("apps_table")
                .num_columns(6)
                .striped(true)
                .spacing(vec2(22.0_f32, 10.0_f32))
                .min_row_height(32.0_f32)
                .show(ui, |ui| {
                    let headers = if ko {
                        ["#", "프로그램", "설명", "대체 대상", "실행 상태", ""]
                    } else {
                        ["#", "APP", "DESCRIPTION", "ALTERNATIVE TO", "STATUS", ""]
                    };
                    for h in headers {
                        ui.label(RichText::new(h).font(mono(10.0_f32)).color(DIM));
                    }
                    ui.end_row();
                    for &i in idx {
                        let e = &self.entries[i];
                        let accent = hex(&e.meta.accent);
                        number_badge(ui, e.meta.order, accent);
                        let row_resp = ui.horizontal(|ui| {
                            if let Some(tex) = self.icons.get(&e.meta.id) {
                                ui.image((tex.id(), vec2(24.0_f32, 24.0_f32)));
                            }
                            ui.spacing_mut().item_spacing.x = 0.0_f32;
                            ui.label(RichText::new(&e.meta.prefix).strong().size(14.0_f32).color(INK));
                            ui.label(RichText::new("Craft").strong().size(14.0_f32).color(accent));
                            if e.is_custom_mod {
                                ui.add_space(4.0_f32);
                                ui.label(RichText::new("[MOD]").font(mono(9.0_f32)).color(Color32::from_rgb(0xD8, 0x96, 0xFF)));
                            }
                        }).response;
                        row_resp.on_hover_ui(|ui| {
                            Frame::new()
                                .fill(Color32::from_rgb(0x14, 0x14, 0x18))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2E, 0x2E, 0x38)))
                                .corner_radius(6.0_f32)
                                .inner_margin(Margin::same(12))
                                .show(ui, |ui| {
                                    show_app_tooltip(ui, e, lang, ko);
                                });
                        });
                        ui.label(RichText::new(e.meta.tagline(lang)).color(INK.gamma_multiply(0.85_f32)));
                        ui.label(RichText::new(e.meta.like(lang)).color(DIM));
                        
                        // Status indicator
                        let running_pids = self.runner.pids(&e.meta.id);
                        if !running_pids.is_empty() {
                            running_badge(ui, running_pids.len(), accent, ko);
                        } else {
                            ui.label(RichText::new(if ko { "대기" } else { "READY" }).font(mono(10.0_f32)).color(DIM));
                        }

                        let label = if ko { "▶ 실행" } else { "▶ OPEN" };
                        if ui
                            .add(egui::Button::new(RichText::new(label).font(mono(10.5_f32)).strong()).min_size(vec2(96.0_f32, 26.0_f32)))
                            .clicked()
                        {
                            actions.push(Action::Open(i));
                        }
                        ui.end_row();
                    }
                });
        });
    }

    // ------------------------------------------------------------ suite page

    fn suite_view(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let ko = self.is_ko();
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(8.0_f32);
            let s_tag = if ko { "아트크래프트 스위트 전체 소개" } else { "CRAFT APP SUITE" };
            let s_title = if ko { "14개의 무료 오픈소스 크리에이티브 및 오피스 프로그램. 단 하나의 런처." } else { "Fourteen open-source creative and office apps. One launcher." };
            let s_desc = if ko {
                "모든 Craft 앱은 ArtCraft에 의해 순수 Rust로 작성된 100% 오픈소스(Apache-2.0) 프로그램입니다. \
                 ArtCraft Studio는 모든 프로그램을 하나의 런처로 통합 실행하며, exe 하위 SavedFiles 폴더에 안전하게 작업 파일을 저장합니다."
            } else {
                "Every Craft app is written from scratch in pure Rust by ArtCraft and released under Apache-2.0. \
                 ArtCraft Studio bundles the official builds and launches each one from a single place, saving user files into the local exe folder."
            };

            ui.label(RichText::new(s_tag).font(mono(11.0_f32)).color(DIM));
            ui.label(RichText::new(s_title).size(26.0_f32).strong().color(INK));
            ui.label(RichText::new(s_desc).size(13.0_f32).color(MUTED));
            ui.add_space(18.0_f32);

            let running = self.entries.iter().filter(|e| e.status == Status::Running).count();
            let disk: u64 = self.entries.iter().map(|e| e.size_bytes).sum();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0_f32;
                let w = ((ui.available_width() - 1.0_f32) / 3.0_f32).floor();
                let metrics = if ko {
                    [
                        ("총 프로그램", self.entries.len().to_string()),
                        ("실행 중인 앱", running.to_string()),
                        ("총 디스크 사용량", apps::fmt_size(disk)),
                    ]
                } else {
                    [
                        ("TOTAL APPS", self.entries.len().to_string()),
                        ("RUNNING", running.to_string()),
                        ("DISK USAGE", apps::fmt_size(disk)),
                    ]
                };
                for (k, v) in metrics {
                    let (r, _) = ui.allocate_exact_size(vec2(w, 84.0_f32), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, 0.0_f32, CARD);
                    p.rect_stroke(r, 0.0_f32, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
                    p.text(r.left_top() + vec2(16.0_f32, 14.0_f32), Align2::LEFT_TOP, k, mono(10.0_f32), DIM);
                    p.text(r.left_bottom() + vec2(16.0_f32, -12.0_f32), Align2::LEFT_BOTTOM, v, egui::FontId::proportional(28.0_f32), INK);
                }
            });
            ui.add_space(22.0_f32);

            let sec_title = if ko { "프로그램 목록 및 오픈소스 라이선스" } else { "APPS & LICENSES" };
            section_header(ui, sec_title, self.entries.len(), ko);
            Frame::new().stroke(Stroke::new(1.0_f32, LINE)).inner_margin(Margin::same(14)).show(ui, |ui| {
                egui::Grid::new("suite_table").num_columns(5).striped(true).spacing(vec2(26.0_f32, 8.0_f32)).show(ui, |ui| {
                    let h_list = if ko {
                        ["프로그램", "라이선스", "디스크 크기", "저장소 링크", ""]
                    } else {
                        ["APP", "LICENSE", "SIZE", "SOURCE", ""]
                    };
                    for h in h_list {
                        ui.label(RichText::new(h).font(mono(10.0_f32)).color(DIM));
                    }
                    ui.end_row();
                    for e in &self.entries {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0_f32;
                            ui.label(RichText::new(&e.meta.prefix).strong().color(INK));
                            ui.label(RichText::new("Craft").strong().color(hex(&e.meta.accent)));
                        });
                        ui.label(RichText::new("Apache-2.0").font(mono(11.0_f32)).color(MUTED));
                        ui.label(
                            RichText::new(if e.size_bytes > 0 { apps::fmt_size(e.size_bytes) } else { "—".into() })
                                .font(mono(11.0_f32))
                                .color(MUTED),
                        );
                        ui.label(
                            RichText::new(e.meta.repo.trim_start_matches("https://")).font(mono(11.0_f32)).color(MUTED),
                        );
                        if link(ui, "VIEW ↗").clicked() {
                            actions.push(Action::Url(e.meta.repo.clone()));
                        }
                        ui.end_row();
                    }
                });
            });
            ui.add_space(14.0_f32);
            ui.horizontal(|ui| {
                let btn_saves = if ko { "📁 작업 저장 폴더 (SavedFiles) 열기" } else { "📁 Open SavedFiles Folder" };
                if ui.button(btn_saves).clicked() {
                    actions.push(Action::Folder(self.saves_dir.clone()));
                }

                let btn_custom = if ko { "🛠️ 수정본(커스텀) 앱 폴더 열기" } else { "🛠️ Open Custom Apps Folder" };
                if ui.button(btn_custom).clicked() {
                    actions.push(Action::Folder(self.custom_apps_dir.clone()));
                }

                let btn_apps = if ko { "📂 공식 앱 설치 폴더 열기" } else { "📂 Open Official Apps Folder" };
                if ui.button(btn_apps).clicked() {
                    actions.push(Action::Folder(self.apps_dir.clone()));
                }
            });
        });
    }

    // ------------------------------------------------------------ 3M-style Progress Modal
    fn progress_modal(&mut self, ctx: &egui::Context) {
        if !self.show_progress_modal {
            return;
        }

        let ko = self.is_ko();
        let is_updating = self.updater.has_active_updates();
        let (overall_pct, overall_desc) = self.updater.overall_progress().unwrap_or((1.0, if ko { "업데이트 완료".to_string() } else { "Complete".to_string() }));
        let pct_int = (overall_pct * 100.0) as u32;

        let title = if ko {
            "🔄 소프트웨어 업데이트 & 파일 교체"
        } else {
            "🔄 SOFTWARE UPDATE & REPLACEMENT"
        };

        let mut is_open = self.show_progress_modal;
        let mut close_requested = false;

        egui::Window::new(RichText::new(title).font(mono(11.5_f32)))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, vec2(0.0_f32, 0.0_f32))
            .default_width(520.0_f32)
            .frame(Frame::window(&ctx.style()).inner_margin(Margin::same(20)))
            .show(ctx, |ui| {
                ui.set_width(500.0_f32);

                if is_updating {
                    // Header Status
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⏳").size(24.0_f32));
                        ui.vertical(|ui| {
                            let title = if ko {
                                format!("최신 버전 다운로드 및 교체 진행 중 ({pct_int}%)")
                            } else {
                                format!("Downloading and replacing files ({pct_int}%)")
                            };
                            ui.label(RichText::new(title).size(15.0_f32).strong().color(Color32::from_rgb(0xF5, 0xC0, 0x40)));
                            ui.label(RichText::new(&overall_desc).font(mono(11.0_f32)).color(MUTED));
                        });
                    });

                    ui.add_space(14.0_f32);

                    // Large Progress Bar
                    let pb = egui::ProgressBar::new(overall_pct)
                        .show_percentage()
                        .fill(Color32::from_rgb(0xF5, 0xC0, 0x40))
                        .corner_radius(4.0_f32);
                    ui.add_sized(vec2(ui.available_width(), 22.0_f32), pb);

                    ui.add_space(16.0_f32);
                    ui.separator();
                    ui.add_space(10.0_f32);

                    // 4-Step Architecture Indicator
                    let steps = if ko {
                        [
                            ("1. 최신 릴리스 정보 대조 및 검증", overall_pct >= 0.15),
                            ("2. 고속 청크 패키지 다운로드", overall_pct >= 0.20),
                            ("3. 무결성 압축 해제 및 안전 임시 백업", overall_pct >= 0.80),
                            ("4. 동일 위치 안전 교체 (In-place Replace)", overall_pct >= 0.92),
                        ]
                    } else {
                        [
                            ("1. Release metadata verification", overall_pct >= 0.15),
                            ("2. Download package chunks", overall_pct >= 0.20),
                            ("3. Decompress and temporary backup", overall_pct >= 0.80),
                            ("4. Safe in-place file replacement", overall_pct >= 0.92),
                        ]
                    };

                    for (name, active) in steps {
                        ui.horizontal(|ui| {
                            if active {
                                ui.label(RichText::new("✔").color(OK).font(mono(11.0_f32)));
                                ui.label(RichText::new(name).color(INK).font(mono(10.5_f32)));
                            } else {
                                ui.label(RichText::new("○").color(DIM).font(mono(11.0_f32)));
                                ui.label(RichText::new(name).color(DIM).font(mono(10.5_f32)));
                            }
                        });
                    }

                    ui.add_space(14.0_f32);
                    Frame::new()
                        .fill(Color32::from_rgb(0x18, 0x18, 0x1C))
                        .stroke(Stroke::new(1.0_f32, LINE))
                        .corner_radius(4.0_f32)
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            let note = if ko {
                                "🔒 안전 보장: 사용자 작업 폴더(SavedFiles)는 100% 안전하게 격리 보존됩니다."
                            } else {
                                "🔒 SavedFiles directory is 100% preserved and untouched."
                            };
                            ui.label(RichText::new(note).font(mono(9.5_f32)).color(MUTED));
                        });
                } else {
                    let overall_res = self.updater.overall_result();
                    if let Some((false, err_msg)) = overall_res {
                        // Update Failed Error State
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("❌").size(26.0_f32).color(ERR));
                            ui.vertical(|ui| {
                                let fail_txt = if ko {
                                    "소프트웨어 업데이트 중 오류가 발생했습니다"
                                } else {
                                    "Update failed to complete"
                                };
                                ui.label(RichText::new(fail_txt).size(15.0_f32).strong().color(ERR));
                                ui.label(RichText::new(&err_msg).size(11.0_f32).color(Color32::from_rgb(0xF5, 0x90, 0x90)));
                            });
                        });

                        ui.add_space(14.0_f32);
                        let pb = egui::ProgressBar::new(1.0)
                            .show_percentage()
                            .fill(ERR)
                            .corner_radius(4.0_f32);
                        ui.add_sized(vec2(ui.available_width(), 18.0_f32), pb);

                        ui.add_space(14.0_f32);
                        Frame::new()
                            .fill(Color32::from_rgb(0x28, 0x10, 0x10))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x80, 0x20, 0x20)))
                            .corner_radius(4.0_f32)
                            .inner_margin(Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                let fail_sub = if ko {
                                    "인터넷 연결 또는 저장소 권한을 확인해주세요. 기존 작업 파일은 안전하게 보존되어 있습니다."
                                } else {
                                    "Check internet connection or permissions. Existing files are safe."
                                };
                                ui.label(RichText::new(fail_sub).font(mono(9.5_f32)).color(Color32::from_rgb(0xF5, 0xA0, 0xA0)));
                            });

                        ui.add_space(14.0_f32);
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let btn_txt = if ko { "닫기" } else { "Close" };
                                let btn = egui::Button::new(RichText::new(btn_txt).font(mono(11.0_f32)).strong().color(INK))
                                    .fill(Color32::from_rgb(0x30, 0x30, 0x38))
                                    .corner_radius(4.0_f32)
                                    .min_size(vec2(100.0_f32, 32.0_f32));
                                if ui.add(btn).clicked() {
                                    close_requested = true;
                                }
                            });
                        });
                    } else {
                        // Update Completed Success State
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("✔").size(26.0_f32).color(OK));
                            ui.vertical(|ui| {
                                let comp_txt = if ko {
                                    "소프트웨어 최신 버전 교체가 성공적으로 완료되었습니다!"
                                } else {
                                    "Software update completed successfully!"
                                };
                                ui.label(RichText::new(comp_txt).size(15.0_f32).strong().color(OK));
                                let sub_txt = if ko {
                                    "동일 위치 안전 교체(In-place Replace) 방식으로 최신 프로그램 파일이 안전하게 갱신되었습니다."
                                } else {
                                    "App binaries updated to the latest release in-place."
                                };
                                ui.label(RichText::new(sub_txt).size(11.0_f32).color(MUTED));
                            });
                        });

                        ui.add_space(14.0_f32);
                        let pb = egui::ProgressBar::new(1.0)
                            .show_percentage()
                            .fill(OK)
                            .corner_radius(4.0_f32);
                        ui.add_sized(vec2(ui.available_width(), 18.0_f32), pb);

                        ui.add_space(14.0_f32);
                        Frame::new()
                            .fill(Color32::from_rgb(0x0C, 0x22, 0x16))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x1E, 0x54, 0x38)))
                            .corner_radius(4.0_f32)
                            .inner_margin(Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                let ok_sub = if ko {
                                    "작업 파일(SavedFiles) 및 개인 설정이 모두 완벽하게 보존되었습니다."
                                } else {
                                    "Projects and personal settings in SavedFiles are fully preserved."
                                };
                                ui.label(RichText::new(ok_sub).font(mono(9.5_f32)).color(Color32::from_rgb(0xA0, 0xE8, 0xC0)));
                            });

                        ui.add_space(14.0_f32);
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let btn_txt = if ko { "확인 및 닫기" } else { "Close" };
                                let btn = egui::Button::new(RichText::new(btn_txt).font(mono(11.0_f32)).strong().color(BG))
                                    .fill(OK)
                                    .corner_radius(4.0_f32)
                                    .min_size(vec2(130.0_f32, 32.0_f32));
                                if ui.add(btn).clicked() {
                                    close_requested = true;
                                }
                            });
                        });
                    }
                }
            });
        if close_requested {
            is_open = false;
            self.updater.clear_results();
        }
        self.show_progress_modal = is_open;
    }

    fn updates_window(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let ko = self.is_ko();
        let mut open = self.show_updates;
        let title = if ko {
            "🔄 소프트웨어 통합 업데이트 관리자"
        } else {
            "🔄 SOFTWARE UPDATE CENTER"
        };

        egui::Window::new(RichText::new(title).font(mono(11.5_f32)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(700.0_f32)
            .anchor(Align2::CENTER_CENTER, vec2(0.0_f32, 0.0_f32))
            .frame(Frame::window(&ctx.style()).inner_margin(Margin::same(18)))
            .show(ctx, |ui| {
                ui.set_max_width(720.0_f32);
                let desc = if ko {
                    "동일 위치 안전 교체(In-place Replace) 방식으로 최신 버전으로 교체합니다. \
                     사용자가 작업한 파일(SavedFiles)과 개인 설정은 100% 안전하게 유지됩니다."
                } else {
                    "In-place replacements update apps safely to latest releases. \
                     Your projects and files in SavedFiles are 100% preserved."
                };
                ui.label(RichText::new(desc).size(12.0_f32).color(MUTED));
                ui.add_space(10.0_f32);

                let upd_count = self.available_update_count();
                let is_upd_all = self.updater.has_active_updates();

                // Top Smart Banner & Bulk Action Bar
                if upd_count > 0 {
                    Frame::new()
                        .fill(Color32::from_rgb(0x2A, 0x1F, 0x0A))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xD4, 0x90, 0x1A)))
                        .corner_radius(6.0_f32)
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("⚡").size(18.0_f32));
                                let sum_txt = if ko {
                                    format!("새로운 업데이트 {upd_count}개가 발견되었습니다! 아래 버튼으로 한 번에 적용할 수 있습니다.")
                                } else {
                                    format!("{upd_count} update(s) ready to install. Update all with one click.")
                                };
                                ui.label(RichText::new(sum_txt).font(mono(11.0_f32)).color(Color32::from_rgb(0xF5, 0xC0, 0x40)));

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let all_lbl = if is_upd_all {
                                        if ko { "⏳ 일괄 업데이트 중...".to_string() } else { "⏳ Updating...".to_string() }
                                    } else if ko {
                                        format!("⚡ 전체 일괄 업데이트 ({upd_count})")
                                    } else {
                                        format!("⚡ Update All ({upd_count})")
                                    };
                                    let btn = egui::Button::new(RichText::new(all_lbl).font(mono(11.0_f32)).strong().color(Color32::from_rgb(0x18, 0x12, 0x05)))
                                        .fill(Color32::from_rgb(0xF5, 0xC0, 0x40))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xFF, 0xD4, 0x60)))
                                        .corner_radius(4.0_f32);
                                    if ui.add_enabled(!is_upd_all, btn).clicked() {
                                        actions.push(Action::UpdateAll);
                                    }
                                });
                            });
                        });
                } else {
                    Frame::new()
                        .fill(Color32::from_rgb(0x0C, 0x22, 0x16))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x1E, 0x54, 0x38)))
                        .corner_radius(6.0_f32)
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("✔").size(16.0_f32).color(OK));
                                let all_ok = if ko {
                                    "모든 프로그램이 최신 상태로 유지되고 있습니다. 일일이 확인할 필요가 없습니다."
                                } else {
                                    "All programs are completely up to date. No manual action required."
                                };
                                ui.label(RichText::new(all_ok).font(mono(11.0_f32)).color(Color32::from_rgb(0xA0, 0xE8, 0xC0)));
                            });
                        });
                }

                ui.add_space(8.0_f32);
                ui.horizontal(|ui| {
                    let chk_lbl = if self.updater.is_checking {
                        if ko { "⏳ 최신 정보 확인 중..." } else { "⏳ Checking for updates..." }
                    } else if ko {
                        "🔄 최신 버전 정보 새로고침"
                    } else {
                        "🔄 Refresh Version Info"
                    };
                    if ui.add_enabled(!self.updater.is_checking, egui::Button::new(RichText::new(chk_lbl).font(mono(10.0_f32)))).clicked() {
                        actions.push(Action::CheckUpdates);
                    }
                });

                ui.add_space(10.0_f32);
                ui.separator();
                ui.add_space(6.0_f32);

                ScrollArea::vertical().max_height(380.0_f32).show(ui, |ui| {
                    egui::Grid::new("update_table")
                        .striped(true)
                        .num_columns(6)
                        .spacing(vec2(16.0_f32, 8.0_f32))
                        .min_row_height(30.0_f32)
                        .show(ui, |ui| {
                            let headers = if ko {
                                ["#", "프로그램", "현재 버전", "최신 버전", "상태", "동작"]
                            } else {
                                ["#", "APP", "CURRENT", "LATEST", "STATUS", "ACTION"]
                            };
                            for h in headers {
                                ui.label(RichText::new(h).font(mono(10.0_f32)).color(DIM));
                            }
                            ui.end_row();

                            for (idx, e) in self.entries.iter().enumerate() {
                                number_badge(ui, e.meta.order, hex(&e.meta.accent));
                                ui.label(RichText::new(e.meta.name()).strong().color(INK));

                                let cur_ver = e.version.as_deref().unwrap_or("—");
                                ui.label(RichText::new(format!("v{cur_ver}")).font(mono(10.5_f32)).color(MUTED));

                                let latest = self.updater.latest_version(&e.meta.id);
                                let is_updating = self.updater.is_updating(&e.meta.id);

                                let has_update = if let (Some(cur), Some(lat)) = (e.version.as_deref(), latest.as_deref()) {
                                    crate::updater::is_newer(lat, cur)
                                } else {
                                    false
                                };

                                let latest_str = if has_update {
                                    latest.as_deref().unwrap_or(cur_ver)
                                } else if let (Some(cur), Some(lat)) = (e.version.as_deref(), latest.as_deref()) {
                                    if crate::updater::is_newer(cur, lat) {
                                        cur
                                    } else {
                                        lat
                                    }
                                } else {
                                    latest.as_deref().unwrap_or(cur_ver)
                                };

                                if has_update {
                                    ui.label(RichText::new(format!("v{latest_str}")).font(mono(10.5_f32)).strong().color(Color32::from_rgb(0xF5, 0xC0, 0x40)));
                                } else {
                                    ui.label(RichText::new(format!("v{latest_str}")).font(mono(10.5_f32)).color(DIM));
                                }

                                if is_updating {
                                    let (p, desc) = self.updater.get_progress(&e.meta.id).unwrap_or((0.15, if ko { "업데이트 중...".to_string() } else { "Updating...".to_string() }));
                                    let pct_val = (p * 100.0) as u32;
                                    ui.label(RichText::new(format!("⏳ {pct_val}% · {desc}")).font(mono(10.0_f32)).color(Color32::from_rgb(0xF5, 0xC0, 0x40)));
                                } else if has_update {
                                    ui.label(RichText::new(if ko { "⚡ 업데이트 가능" } else { "⚡ Available" }).font(mono(10.0_f32)).strong().color(Color32::from_rgb(0xF5, 0xC0, 0x40)));
                                } else if latest.is_some() {
                                    ui.label(RichText::new(if ko { "✔ 최신 상태" } else { "✔ Up to date" }).font(mono(10.0_f32)).color(OK));
                                } else {
                                    ui.label(RichText::new(if ko { "확인 대기" } else { "Ready" }).font(mono(10.0_f32)).color(DIM));
                                }

                                if is_updating {
                                    let p = self.updater.get_progress(&e.meta.id).map(|(v, _)| v).unwrap_or(0.15);
                                    let pb = egui::ProgressBar::new(p)
                                        .show_percentage()
                                        .fill(Color32::from_rgb(0xF5, 0xC0, 0x40));
                                    ui.add_sized(vec2(80.0_f32, 20.0_f32), pb);
                                } else if has_update {
                                    let btn = egui::Button::new(RichText::new(if ko { "⚡ 지금 업데이트" } else { "⚡ Update" }).font(mono(10.0_f32)).strong().color(Color32::from_rgb(0x18, 0x12, 0x05)))
                                        .fill(Color32::from_rgb(0xF5, 0xC0, 0x40))
                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xFF, 0xD4, 0x60)));
                                    if ui.add(btn).clicked() {
                                        actions.push(Action::Update(idx));
                                    }
                                } else {
                                    let _ = ui.add_enabled(
                                        false,
                                        egui::Button::new(RichText::new(if ko { "✔ 완료" } else { "✔ Done" }).font(mono(10.0_f32)).color(DIM)),
                                    );
                                }
                                ui.end_row();
                            }
                        });
                });
            });
        self.show_updates = open;
    }

    fn settings_window(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let ko = self.is_ko();
        let mut open = self.show_settings;
        let set_title = if ko { "환경설정" } else { "SETTINGS" };
        egui::Window::new(RichText::new(set_title).font(mono(11.0_f32)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::RIGHT_TOP, vec2(-12.0_f32, 54.0_f32))
            .frame(Frame::window(&ctx.style()).inner_margin(Margin::same(16)))
            .show(ctx, |ui| {
                ui.set_width(400.0_f32);

                // --- 1. Language & Behavior ---
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if ko { "언어" } else { "Language" }).font(mono(10.5_f32)).color(MUTED));
                    ui.add_space(8.0_f32);
                    if ui.selectable_value(&mut self.state.language, "ko".to_string(), "한국어").clicked() {
                        self.toast("언어를 한국어로 변경했습니다.", OK);
                    }
                    if ui.selectable_value(&mut self.state.language, "en".to_string(), "English").clicked() {
                        self.toast("Language switched to English.", OK);
                    }
                });
                ui.add_space(6.0_f32);
                let min_txt = if ko { "프로그램 실행 시 런처 최소화" } else { "Minimize launcher after opening an app" };
                ui.checkbox(&mut self.state.minimize_on_open, min_txt);
                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);

                // --- 2. Saved Files Folder Section ---
                let saves_hdr = if ko { "📁 작업 파일 저장 위치 (SavedFiles)" } else { "📁 SAVED FILES FOLDER" };
                ui.label(RichText::new(saves_hdr).font(mono(10.5_f32)).color(OK));
                ui.add_space(3.0_f32);

                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.saves_dir_input)
                            .desired_width(ui.available_width() - 75.0_f32)
                            .font(mono(10.0_f32)),
                    );
                    let apply_txt = if ko { "적용" } else { "Apply" };
                    if ui.button(apply_txt).clicked() {
                        let p = PathBuf::from(self.saves_dir_input.trim());
                        if !p.as_os_str().is_empty() {
                            self.set_saves_dir(p);
                        }
                    }
                });
                ui.add_space(3.0_f32);
                ui.horizontal(|ui| {
                    let browse_txt = if ko { "📂 찾아보기" } else { "Browse..." };
                    if ui.button(browse_txt).clicked() {
                        self.browse_saves_dir();
                    }
                    let open_s_txt = if ko { "📁 폴더 열기" } else { "Open Folder" };
                    if ui.button(open_s_txt).clicked() {
                        actions.push(Action::Folder(self.saves_dir.clone()));
                    }
                    let reset_txt = if ko { "↺ 초기화" } else { "Reset" };
                    if ui.button(reset_txt).on_hover_text(if ko { "기본 SavedFiles 위치로 복원" } else { "Reset to default folder" }).clicked() {
                        self.reset_saves_dir();
                    }
                });
                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);

                // --- 3. Custom Apps Folder Section ---
                let custom_hdr = if ko { "🛠️ 커스텀 앱 위치 (custom_apps)" } else { "🛠️ CUSTOM MOD APPS FOLDER" };
                ui.label(RichText::new(custom_hdr).font(mono(10.5_f32)).color(Color32::from_rgb(0x7C, 0xA8, 0xFF)));
                ui.label(RichText::new(self.custom_apps_dir.display().to_string()).font(mono(10.0_f32)).color(MUTED));
                ui.add_space(3.0_f32);
                ui.horizontal(|ui| {
                    let open_c_txt = if ko { "📁 커스텀 폴더 열기" } else { "Open Folder" };
                    if ui.button(open_c_txt).clicked() {
                        actions.push(Action::Folder(self.custom_apps_dir.clone()));
                    }
                    let rescan_txt = if ko { "🔄 다시 스캔" } else { "Rescan" };
                    if ui.button(rescan_txt).clicked() {
                        actions.push(Action::Rescan);
                    }
                });
                ui.add_space(2.0_f32);
                let tip = if ko {
                    "💡 한글 패치/개조 .exe 보관용 (공식 앱보다 우선 실행)"
                } else {
                    "💡 Priority 1 folder for modded/translated apps"
                };
                ui.label(RichText::new(tip).size(10.5_f32).color(DIM));

                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);

                // --- 4. External AI Agent One-Click MCP Integration ---
                let ai_hdr = if ko {
                    "🔌 외부 IDE / AI 에이전트 MCP 브릿지"
                } else {
                    "🔌 EXTERNAL AI AGENT MCP BRIDGE"
                };
                ui.label(RichText::new(ai_hdr).font(mono(11.0_f32)).color(Color32::from_rgb(0x4C, 0xCE, 0xAC)));
                ui.add_space(2.0_f32);
                let ai_sub = if ko {
                    "Cursor, Claude Desktop, Antigravity(.mcp.json) 등 외부 AI 도구들이 14종 Craft 앱을 MCP 및 --bridge 모드로 직접 제어할 수 있도록 원클릭으로 자동 등록합니다."
                } else {
                    "One-click auto-registration for Cursor, Claude Desktop, Antigravity (.mcp.json) to control 14 Craft apps via MCP & --bridge mode."
                };
                ui.label(RichText::new(ai_sub).size(10.0_f32).color(DIM));
                ui.add_space(6.0_f32);

                // Prominent One-Click Injection Card
                Frame::new()
                    .fill(Color32::from_rgb(0x13, 0x1A, 0x26))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x23, 0x3E, 0x62)))
                    .corner_radius(8.0_f32)
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let btn_title = if ko {
                                "Cursor / Claude / Antigravity에 14종 Craft 앱 연동 설정 자동 주입"
                            } else {
                                "Auto-Inject 14 Craft Apps MCP Bridge to Cursor / Claude / Antigravity"
                            };
                            let btn_tip = if ko {
                                "클릭 시 PC에 설치된 Cursor(%USERPROFILE%\\.cursor\\mcp.json), Claude Desktop(%APPDATA%\\Claude), Antigravity(.mcp.json)에 14종 앱의 고유 포트(49152~)와 보안 토큰이 포함된 --bridge 설정을 즉시 병합 주입합니다."
                            } else {
                                "Merges MCP bridge settings with ports and security token into Cursor, Claude Desktop, and Antigravity."
                            };

                            let inject_btn = egui::Button::new(
                                RichText::new(format!("🚀 {btn_title}"))
                                    .font(mono(11.0_f32))
                                    .strong()
                                    .color(Color32::from_rgb(0x10, 0x18, 0x20)),
                            )
                            .fill(Color32::from_rgb(0x50, 0xFA, 0x9E))
                            .corner_radius(6.0_f32);

                            if ui.add(inject_btn).on_hover_text(btn_tip).clicked() {
                                let reports = crate::mcp_config_gen::sync_all_ai_mcp_configs(
                                    &self.entries,
                                    Some(&self.saves_dir),
                                );
                                let successes: Vec<_> = reports.iter().filter(|r| r.success).collect();
                                if !successes.is_empty() {
                                    let toast_msg = if ko {
                                        "성공적으로 연동되었습니다!"
                                    } else {
                                        "Successfully connected!"
                                    };
                                    self.toast(toast_msg, OK);
                                } else {
                                    let err_msg = if ko {
                                        "연동 실패: 대상 설정 파일을 생성하거나 수정하지 못했습니다."
                                    } else {
                                        "Connection failed: Unable to update target configuration files."
                                    };
                                    self.toast(err_msg, ERR);
                                }
                            }
                        });
                        ui.add_space(3.0_f32);
                        let desc_lbl = if ko {
                            "💡 기존 사용자의 타사 MCP 설정은 100% 안전하게 보존되며, Craft 앱의 고유 포트와 보안 토큰이 최신 경로로 등록됩니다."
                        } else {
                            "💡 Preserves your existing custom MCP servers and registers Craft apps with unique ports (49152+) and security token."
                        };
                        ui.label(RichText::new(desc_lbl).font(mono(9.5_f32)).color(MUTED));
                    });

                ui.add_space(6.0_f32);

                // Collapsible Individual AI Client Management
                let detail_lbl = if ko {
                    "▶ 세부 AI 클라이언트 상태 및 개별 설정"
                } else {
                    "▶ Detailed AI Client Status & Settings"
                };
                egui::CollapsingHeader::new(RichText::new(detail_lbl).font(mono(10.0_f32)).color(MUTED))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_space(4.0_f32);
                        for &client in &crate::ai_connect::Client::ALL {
                            ui.horizontal(|ui| {
                                let is_installed = client.is_present();
                                let count = crate::ai_connect::status(client).unwrap_or(0);

                                let badge = if count > 0 {
                                    RichText::new(format!("● {count}개 연결됨")).font(mono(9.5_f32)).color(OK)
                                } else if is_installed {
                                    RichText::new("○ 대기 중").font(mono(9.5_f32)).color(MUTED)
                                } else {
                                    RichText::new("미설치").font(mono(9.5_f32)).color(DIM)
                                };

                                ui.label(RichText::new(client.label()).font(mono(10.0_f32)).color(INK));
                                ui.label(badge);

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if count > 0 {
                                        let disc_txt = if ko { "연결 해제" } else { "Disconnect" };
                                        if ui.small_button(disc_txt).clicked() {
                                            match crate::ai_connect::disconnect(client) {
                                                Ok(n) => self.toast(format!("{}에서 {n}개 도구 연결을 해제했습니다.", client.label()), MUTED),
                                                Err(e) => self.toast(format!("해제 실패: {e}"), ERR),
                                            }
                                        }
                                    } else {
                                        let conn_txt = if ko { "개별 연결" } else { "Connect" };
                                        if ui.small_button(conn_txt).clicked() {
                                            match crate::ai_connect::connect(client, &self.entries) {
                                                Ok(n) => self.toast(format!("{}에 {n}개 앱 도구 등록 완료! 앱을 재시작하면 인식됩니다.", client.label()), OK),
                                                Err(e) => self.toast(format!("연결 실패: {e}"), ERR),
                                            }
                                        }
                                    }
                                });
                            });
                            ui.add_space(2.0_f32);
                        }

                        ui.add_space(4.0_f32);
                        ui.horizontal(|ui| {
                            let copy_btn_txt = if ko { "📋 MCP JSON 복사 (기타 AI 클라이언트용)" } else { "📋 Copy MCP JSON" };
                            if ui
                                .button(copy_btn_txt)
                                .on_hover_text(if ko {
                                    "Windsurf, Cline 등 기타 AI 클라이언트 설정에 붙여넣을 수 있는 JSON을 복사합니다."
                                } else {
                                    "Copy MCP JSON config for Windsurf, Cline or other AI clients."
                                })
                                .clicked()
                            {
                                let json = crate::ai_connect::mcp_json_string(&self.entries);
                                ctx.copy_text(json);
                                let msg = if ko {
                                    "클립보드에 MCP 설정 JSON이 복사되었습니다!"
                                } else {
                                    "MCP JSON copied to clipboard!"
                                };
                                self.toast(msg, OK);
                            }
                        });
                    });

                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);

                // --- 5. AI Copilot Companion Settings ---
                let comp_hdr = if ko { "💬 앱 내장 AI 어시스턴트 (런처 우측 하단 위젯)" } else { "💬 IN-APP AI ASSISTANT (DOCK WIDGET)" };
                ui.label(RichText::new(comp_hdr).font(mono(10.5_f32)).color(Color32::from_rgb(0x68, 0x98, 0xFF)));
                ui.add_space(2.0_f32);
                let auto_show_lbl = if ko { "프로그램 실행 시 우측 하단 AI 위젯 자동 표시" } else { "Auto-show docked AI widget when launching app" };
                if ui.checkbox(&mut self.state.auto_show_companion, auto_show_lbl).changed() {
                    for c in self.companions.values_mut() {
                        c.auto_show_on_launch = self.state.auto_show_companion;
                    }
                }
                ui.add_space(4.0_f32);

                // AI Provider Selector (Gemini, Claude, OpenAI Codex, Ollama)
                let mut cur_prov = crate::companion::AiProvider::from_u8(self.state.ai_provider);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if ko { "AI 모델 선택:" } else { "AI Provider:" }).font(mono(9.5_f32)).color(MUTED));
                    let prev = cur_prov;
                    egui::ComboBox::from_id_salt("companion_provider_select")
                        .selected_text(RichText::new(cur_prov.label()).font(mono(9.5_f32)).color(Color32::from_rgb(0x68, 0x98, 0xFF)))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut cur_prov, crate::companion::AiProvider::Gemini, "Google Gemini (추천/무료)");
                            ui.selectable_value(&mut cur_prov, crate::companion::AiProvider::Claude, "Anthropic Claude");
                            ui.selectable_value(&mut cur_prov, crate::companion::AiProvider::OpenAi, "OpenAI Codex / ChatGPT");
                            ui.selectable_value(&mut cur_prov, crate::companion::AiProvider::Ollama, "Ollama (로컬 오프라인)");
                        });
                    if cur_prov != prev {
                        self.state.ai_provider = cur_prov.to_u8();
                        for c in self.companions.values_mut() {
                            c.provider = cur_prov;
                        }
                    }
                });
                ui.add_space(4.0_f32);

                ui.horizontal(|ui| {
                    let key_lbl = match cur_prov {
                        crate::companion::AiProvider::Claude => "Claude Key:",
                        crate::companion::AiProvider::OpenAi => "OpenAI Key:",
                        crate::companion::AiProvider::Ollama => "Ollama Host:",
                        _ => "Gemini Key:",
                    };
                    ui.label(RichText::new(key_lbl).font(mono(9.5_f32)).color(MUTED));
                    let is_pwd = cur_prov != crate::companion::AiProvider::Ollama;
                    let avail_w = ui.available_width() - 8.0_f32;
                    let api_edit = ui.add(
                        TextEdit::singleline(&mut self.state.gemini_api_key)
                            .password(is_pwd)
                            .desired_width(avail_w)
                            .hint_text(if cur_prov == crate::companion::AiProvider::Ollama {
                                "http://localhost:11434"
                            } else if ko {
                                "선택 사항 (API Key 입력 시 기능 활성화)"
                            } else {
                                "Optional API Key"
                            }),
                    );
                    if api_edit.changed() {
                        for c in self.companions.values_mut() {
                            c.api_key = self.state.gemini_api_key.clone();
                        }
                    }
                });

                ui.add_space(8.0_f32);
                ui.separator();
                ui.add_space(8.0_f32);

                // --- 6. Update Data Source Section ---
                let upd_hdr = if ko { "🌐 업데이트 소스 (GitHub)" } else { "🌐 UPDATE SOURCE (GITHUB)" };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(upd_hdr).font(mono(10.5_f32)).color(Color32::from_rgb(0xF5, 0xC0, 0x40)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("gochujangs33-dot/Craft-Studio").font(mono(9.5_f32)).color(DIM));
                    });
                });
                ui.add_space(4.0_f32);
                ui.horizontal(|ui| {
                    let repo_btn = if ko { "🌐 GitHub 저장소" } else { "GitHub Repo" };
                    if ui.button(repo_btn).clicked() {
                        actions.push(Action::Url("https://github.com/gochujangs33-dot/Craft-Studio".to_string()));
                    }
                    let check_btn = if ko { "🔄 업데이트 확인" } else { "Check Updates" };
                    if ui.button(check_btn).clicked() {
                        actions.push(Action::CheckUpdates);
                    }
                });
            });
        self.show_settings = open;
    }

    fn drop_overlay(&self, ctx: &egui::Context) {
        let hovering: Vec<String> = ctx.input(|i| {
            i.raw.hovered_files.iter().filter_map(|f| f.path.as_ref()).map(|p| p.display().to_string()).collect()
        });
        if hovering.is_empty() && !ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            return;
        }
        let ko = self.is_ko();
        let p = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("drop")));
        let r = ctx.screen_rect();
        p.rect_filled(r, 0.0_f32, Color32::from_black_alpha(200));
        p.rect_stroke(r.shrink(18.0_f32), 0.0_f32, Stroke::new(1.5_f32, INK.gamma_multiply(0.6_f32)), StrokeKind::Inside);
        let drop_title = if ko { "파일을 여기에 놓아 실행" } else { "DROP TO OPEN" };
        let drop_sub = if ko { "해당 확장자를 지원하는 Craft 앱으로 즉시 열립니다" } else { "The file opens in the matching Craft app" };
        p.text(r.center() - vec2(0.0_f32, 12.0_f32), Align2::CENTER_CENTER, drop_title, mono(14.0_f32), INK);
        p.text(
            r.center() + vec2(0.0_f32, 14.0_f32),
            Align2::CENTER_CENTER,
            drop_sub,
            egui::FontId::proportional(13.0_f32),
            MUTED,
        );
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.2.elapsed() < Duration::from_secs(6));
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0_f32, -40.0_f32))
            .order(Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                for (msg, color, born) in &self.toasts {
                    let a = (1.0_f32 - (born.elapsed().as_secs_f32() - 5.2_f32).max(0.0_f32) / 0.8_f32).clamp(0.0_f32, 1.0_f32);
                    Frame::new()
                        .fill(PANEL.gamma_multiply(a))
                        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(a)))
                        .inner_margin(Margin::symmetric(14, 9))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(6.0_f32, 6.0_f32), Sense::hover());
                                ui.painter().rect_filled(r, 0.0_f32, color.gamma_multiply(a));
                                ui.label(RichText::new(msg).color(INK.gamma_multiply(a)));
                            });
                        });
                    ui.add_space(6.0_f32);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    #[allow(dead_code)]
    fn embedded_ai_dock(&mut self, ctx: &egui::Context, _actions: &mut Vec<Action>) {
        if !self.embedded_ai_open {
            return;
        }

        let temp_dir = std::env::temp_dir().join("ArtCraftCompanion");
        let active_id = self.active_companion_app.clone();
        let app_name = self
            .entries
            .iter()
            .find(|e| e.meta.id == active_id)
            .map(|e| e.meta.name().to_string())
            .unwrap_or_else(|| "PhotoCraft".to_string());

        let comp = self
            .companions
            .entry(active_id.clone())
            .or_insert_with(|| crate::companion::CompanionState::new_for_app(&active_id, &app_name));
        comp.api_key = self.state.gemini_api_key.clone();
        comp.provider = crate::companion::AiProvider::from_u8(self.state.ai_provider);

        // Check if user closed companion from internal header
        if !comp.is_open {
            self.embedded_ai_open = false;
            comp.is_open = true;
            return;
        }

        let (card_w, card_h) = if comp.is_minimized {
            (240.0_f32, 42.0_f32)
        } else {
            (360.0_f32, 490.0_f32)
        };

        let mut open_action = None;

        egui::Area::new(Id::new("embedded_ai_dock_area"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -32.0))
            .show(ctx, |ui| {
                ui.set_width(card_w);
                ui.set_height(card_h);

                Frame::new()
                    .fill(Color32::from_rgb(0x13, 0x16, 0x20))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x2E, 0x36, 0x4D)))
                    .corner_radius(10.0_f32)
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        if !comp.is_minimized {
                            ui.horizontal(|ui| {
                                for target in &["photocraft", "wordcraft", "gridcraft", "figcraft"] {
                                    let is_active = self.active_companion_app == *target;
                                    let label = match *target {
                                        "photocraft" => "PhotoCraft",
                                        "wordcraft" => "WordCraft",
                                        "gridcraft" => "GridCraft",
                                        "figcraft" => "FigCraft",
                                        _ => *target,
                                    };
                                    if chip(ui, label, is_active, false).clicked() {
                                        self.active_companion_app = target.to_string();
                                    }
                                }
                            });
                            ui.add_space(4.0_f32);
                        }

                        comp.render_embedded(
                            ui,
                            ctx,
                            &app_name,
                            &self.entries,
                            &self.saves_dir,
                            &temp_dir,
                            &mut open_action,
                        );
                    });
            });

        if let Some((target, file_path)) = open_action {
            if let Some(pos) = self.entries.iter().position(|e| e.meta.id == target) {
                self.launch(ctx, pos, Some(&file_path));
            }
        }
    }
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.updater.is_checking {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Poll background updater events
        for (app_id, success, msg) in self.updater.poll_events() {
            if app_id == "__check_all__" {
                let was_manual = self.manual_check_requested;
                self.manual_check_requested = false;

                // Dynamically reload synced registry so new cards added on GitHub appear immediately!
                self.registry = apps::load_registry(&self.exe_dir);
                self.entries = apps::scan(&self.registry, &self.apps_dir, &self.exe_dir, &self.saves_dir);
                self.refresh_status();

                let count = self.available_update_count();
                if count > 0 {
                    let alert = if self.is_ko() {
                        format!("💡 {count}개 프로그램의 새로운 업데이트가 감지되었습니다! [🔄 업데이트]를 확인하세요.")
                    } else {
                        format!("💡 {count} update(s) detected! Check [🔄 UPDATE].")
                    };
                    self.toast(alert, Color32::from_rgb(0xF5, 0xC0, 0x40));
                } else if was_manual {
                    if success {
                        let ok_msg = if self.is_ko() {
                            "✔ 모든 프로그램이 최신 버전입니다. (확인 완료)"
                        } else {
                            "✔ All programs are completely up to date. (Verified)"
                        };
                        self.toast(ok_msg, OK);
                    } else {
                        let err_msg = if self.is_ko() {
                            "⚠ 최신 버전 정보를 확인하지 못했습니다. 네트워크 연결을 확인해주세요."
                        } else {
                            "⚠ Failed to fetch version info. Please check your network connection."
                        };
                        self.toast(err_msg, ERR);
                    }
                }
            } else {
                let color = if success { OK } else { ERR };
                self.toast(msg, color);
                if success {
                    self.entries = apps::scan(&self.registry, &self.apps_dir, &self.exe_dir, &self.saves_dir);
                    self.refresh_status();
                    if let Some(e) = self.entries.iter().find(|e| e.meta.id == app_id) {
                        let cur_v = e.version.clone().unwrap_or_else(|| {
                            self.updater.latest_version(&app_id).unwrap_or_default()
                        });
                        self.updater.mark_updated(&app_id, &cur_v);
                    }
                    if self.launch_after_install.as_deref() == Some(&app_id) {
                        self.launch_after_install = None;
                        if let Some(pos) = self.entries.iter().position(|e| e.meta.id == app_id) {
                            self.launch(ctx, pos, None);
                        }
                    }
                }
            }
        }

        if self.last_poll.elapsed() > Duration::from_millis(500) {
            self.refresh_status();
            self.last_poll = Instant::now();
        }
        let has_active_companion = self.companions.values().any(|c| c.is_open);
        if self.updater.has_active_updates() || self.show_progress_modal || has_active_companion {
            ctx.request_repaint_after(Duration::from_millis(50));
        } else {
            ctx.request_repaint_after(Duration::from_millis(800));
        }

        let mut files = std::mem::take(&mut self.pending_files);
        files.extend(ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect::<Vec<_>>()));
        for f in files {
            self.open_file(ctx, &f);
        }

        let mut actions = Vec::new();
        self.top_bar(ctx, &mut actions);
        self.status_bar(ctx);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin { left: 18, right: 18, top: 16, bottom: 8 }))
            .show(ctx, |ui| match self.tab {
                Tab::Launcher => self.launcher_view(ui, &mut actions),
                Tab::Suite => self.suite_view(ui, &mut actions),
            });
        self.updates_window(ctx, &mut actions);
        self.settings_window(ctx, &mut actions);
        self.progress_modal(ctx);
        self.drop_overlay(ctx);
        self.toasts_ui(ctx);

        // Render docked AI companion viewport inside running apps (PhotoCraft, etc.)
        let mut companion_actions = Vec::new();
        let temp_dir = std::env::temp_dir().join("ArtCraftCompanion");
        for (app_id, companion) in self.companions.iter_mut() {
            if !companion.is_open {
                continue;
            }
            let pids = self.runner.pids(app_id);
            let app_name = self
                .entries
                .iter()
                .find(|e| &e.meta.id == app_id)
                .map(|e| e.meta.name().to_string())
                .unwrap_or_else(|| app_id.clone());

            let is_running = !pids.is_empty() || crate::apps::get_app_window_rect(&pids, &app_name).is_some();
            if !is_running {
                continue;
            }

            companion.api_key = self.state.gemini_api_key.clone();
            companion.provider = crate::companion::AiProvider::from_u8(self.state.ai_provider);

            let mut action = None;
            companion.render_for_app(
                ctx,
                app_id,
                &app_name,
                &pids,
                &self.entries,
                &self.saves_dir,
                &temp_dir,
                &mut action,
            );
            if let Some((target, file_path)) = action {
                companion_actions.push((target, file_path));
            }
        }
        for (target, file_path) in companion_actions {
            if let Some(pos) = self.entries.iter().position(|e| e.meta.id == target) {
                self.launch(ctx, pos, Some(&file_path));
            }
        }

        for a in actions {
            self.apply_action(ctx, a);
        }
        if self.state != self.saved_state {
            self.state.save();
            self.saved_state = self.state.clone();
        }
    }
}

// ---------------------------------------------------------------- small widgets

fn chip(ui: &mut egui::Ui, text: &str, active: bool, outlined: bool) -> egui::Response {
    let (fg, bg, stroke) = if active {
        (BG, INK, Stroke::new(1.0_f32, INK))
    } else if outlined {
        (MUTED, Color32::TRANSPARENT, Stroke::new(1.0_f32, LINE))
    } else {
        (MUTED, Color32::TRANSPARENT, Stroke::NONE)
    };
    ui.add(
        egui::Button::new(RichText::new(text).font(mono(10.5_f32)).color(fg))
            .fill(bg)
            .stroke(stroke)
            .min_size(vec2(0.0_f32, 24.0_f32)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let r = ui.add(egui::Label::new(RichText::new(text).font(mono(9.5_f32)).color(MUTED)).sense(Sense::click()));
    if r.hovered() {
        ui.painter().line_segment(
            [r.rect.left_bottom() + vec2(0.0_f32, 1.0_f32), r.rect.right_bottom() + vec2(0.0_f32, 1.0_f32)],
            Stroke::new(1.0_f32, INK),
        );
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn number_badge(ui: &mut egui::Ui, n: u32, accent: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(26.0_f32, 17.0_f32), Sense::hover());
    ui.painter().rect_filled(r, 0.0_f32, accent);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, format!("{n:02}"), mono(10.0_f32), BG);
}

fn running_badge(ui: &mut egui::Ui, count: usize, accent: Color32, ko: bool) {
    let txt = if ko {
        if count > 1 { format!("● 실행 중 ({count}개 창)") } else { "● 실행 중".to_string() }
    } else {
        if count > 1 { format!("● RUNNING ({count})") } else { "● RUNNING".to_string() }
    };
    Frame::new()
        .fill(accent.gamma_multiply(0.12_f32))
        .stroke(Stroke::new(1.0_f32, accent.gamma_multiply(0.6_f32)))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(txt).font(mono(9.5_f32)).strong().color(accent));
        });
}

fn section_header(ui: &mut egui::Ui, title: &str, count: usize, ko: bool) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).font(mono(10.5_f32)).color(MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let cnt_str = if ko { format!("{count}개 프로그램") } else { format!("{count} APPS") };
            ui.label(RichText::new(cnt_str).font(mono(10.5_f32)).color(DIM));
        });
    });
    ui.add_space(6.0_f32);
}

fn show_app_tooltip(ui: &mut egui::Ui, e: &AppEntry, lang: &str, ko: bool) {
    let accent = hex(&e.meta.accent);
    ui.set_max_width(340.0_f32);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0_f32, 4.0_f32);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0_f32;
            ui.label(RichText::new(&e.meta.prefix).size(16.0_f32).strong().color(INK));
            ui.label(RichText::new("Craft").size(16.0_f32).strong().color(accent));
            ui.add_space(8.0_f32);
            ui.label(RichText::new(e.meta.like(lang)).size(11.0_f32).color(Color32::from_rgb(0xA0, 0xA0, 0xAC)));
        });
        ui.separator();
        ui.add_space(2.0_f32);
        ui.label(RichText::new(e.meta.description(lang)).size(11.5_f32).color(Color32::from_rgb(0xDD, 0xDD, 0xE4)));
        if !e.meta.extensions.is_empty() {
            ui.add_space(6.0_f32);
            ui.horizontal(|ui| {
                let fmt_hdr = if ko { "지원 포맷:" } else { "Formats:" };
                ui.label(RichText::new(fmt_hdr).font(mono(9.5_f32)).color(DIM));
                ui.label(
                    RichText::new(format!(".{}", e.meta.extensions.join("  .")))
                        .font(mono(9.5_f32))
                        .color(Color32::from_rgb(0xB0, 0xB0, 0xBC)),
                );
            });
        }
    });
}
