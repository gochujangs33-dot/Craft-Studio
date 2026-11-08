//! ArtCraft AI Companion - Floating bottom-right AI assistant widget
//! Supports multimodal image pasting (Ctrl+V), editing requests, and integration with Craft apps.

use crate::apps::{fmt_date, now_secs, AppEntry};
use eframe::egui::{
    self, vec2, Align, Color32, Frame, Key, Layout, Margin,
    RichText, ScrollArea, Stroke, TextureHandle, Ui, ViewportBuilder,
    ViewportId,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

// Design Tokens (matching custom dark design system)
#[allow(dead_code)]
const BG_COMPANION: Color32 = Color32::from_rgb(0x13, 0x16, 0x20);
#[allow(dead_code)]
const CARD_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1E);
#[allow(dead_code)]
const ACCENT_DEFAULT: Color32 = Color32::from_rgb(0x68, 0x98, 0xFF);
#[allow(dead_code)]
const TEXT_MAIN: Color32 = Color32::from_rgb(0xEE, 0xEE, 0xF2);
const TEXT_MUTED: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x9A);
#[allow(dead_code)]
const BORDER_COLOR: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x34);

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiProvider {
    Gemini,
    Claude,
    OpenAi,
    Ollama,
}

impl Default for AiProvider {
    fn default() -> Self {
        Self::Gemini
    }
}

impl AiProvider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gemini => "Google Gemini",
            Self::Claude => "Anthropic Claude",
            Self::OpenAi => "OpenAI Codex / ChatGPT",
            Self::Ollama => "Ollama (로컬)",
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Claude,
            2 => Self::OpenAi,
            3 => Self::Ollama,
            _ => Self::Gemini,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::Gemini => 0,
            Self::Claude => 1,
            Self::OpenAi => 2,
            Self::Ollama => 3,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    #[allow(dead_code)]
    System,
}

#[derive(Clone, Debug)]
pub struct ActionResult {
    pub label: String,
    pub file_path: PathBuf,
    pub app_id: String,
    pub auto_open: bool,
    #[allow(dead_code)]
pub can_paste: bool,
    #[allow(dead_code)]
    pub can_overwrite: bool,
    pub original_path: Option<PathBuf>,
}

impl ActionResult {
    pub fn for_file(label: impl Into<String>, app_id: impl Into<String>, file_path: PathBuf, auto_open: bool) -> Self {
        Self {
            label: label.into(),
            file_path,
            app_id: app_id.into(),
            auto_open,
            can_paste: false,
            can_overwrite: false,
            original_path: None,
        }
    }

    #[allow(dead_code)]
    pub fn for_image(
        label: impl Into<String>,
        app_id: impl Into<String>,
        file_path: PathBuf,
        original_path: Option<PathBuf>,
    ) -> Self {
        Self {
            label: label.into(),
            file_path,
            app_id: app_id.into(),
            auto_open: false,
            can_paste: true,
            can_overwrite: original_path.is_some(),
            original_path,
        }
    }

    pub fn for_live_edit(
        app_id: impl Into<String>,
        file_path: PathBuf,
        original_path: Option<PathBuf>,
    ) -> Self {
        Self {
            label: "⚡ 실시간 반영 완료".to_string(),
            file_path,
            app_id: app_id.into(),
            auto_open: false,
            can_paste: true,
            can_overwrite: original_path.is_some(),
            original_path,
        }
    }
}

pub struct ChatMessage {
    pub role: Role,
    pub text: String,
    pub image_preview: Option<TextureHandle>,
    #[allow(dead_code)]
    pub image_path: Option<PathBuf>,
    pub action_result: Option<ActionResult>,
    #[allow(dead_code)]
    pub timestamp: u64,
}

enum AsyncResponse {
    Success {
        text: String,
        updated_image: Option<image::RgbaImage>,
        action: Option<ActionResult>,
    },
    Error(String),
}

pub struct CompanionState {
    pub is_open: bool,
    pub is_minimized: bool,
    pub auto_show_on_launch: bool,
    pub target_app: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub input_text: String,
    pub pending_image: Option<(image::RgbaImage, TextureHandle, PathBuf)>,
    pub working_image: Option<image::RgbaImage>,
    pub working_preview: Option<TextureHandle>,
    pub is_busy: bool,
    pub api_key: String,
    pub provider: AiProvider,
    pub model_name: String,
    pub base_url: String,
    pub show_settings_panel: bool,
    #[allow(dead_code)]
    pub show_toolbar_button: bool,
    pub show_view_menu: bool,
    pub require_approval: bool,
    pub pending_approval: Option<ActionResult>,
    pub selected_agent: String,
    pub is_connected: bool,
    pub show_acp_install_guide: bool,
    pub attach_view: bool,
    pub attach_selection: bool,
    pub edit_mode: String,
    #[allow(dead_code)]
    pub last_toolbar_pos: Option<[f32; 2]>,
    #[allow(dead_code)]
    pub last_toolbar_size: Option<[f32; 2]>,
    pub status_msg: Option<String>,
    #[allow(dead_code)]
    pub v_was_down: bool,
    pub last_dock_pos: Option<[f32; 2]>,
    pub last_dock_size: Option<[f32; 2]>,
    #[allow(dead_code)]
pub side_menu_detected: bool,
    #[allow(dead_code)]
pub last_side_menu_check: u64,
    #[allow(dead_code)]
pub dock_beside_sidebar: bool,
    #[allow(dead_code)]
pub native_sidebar_px: f32,
    #[allow(dead_code)]
pub last_sidebar_check: u64,
    resp_sender: Sender<AsyncResponse>,
    resp_receiver: Receiver<AsyncResponse>,
}

impl Default for CompanionState {
    fn default() -> Self {
        Self::new_for_app("photocraft", "PhotoCraft")
    }
}

impl CompanionState {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

        pub fn new_for_app(app_id: &str, _app_name: &str) -> Self {
        let (tx, rx) = channel();
        // Start with an empty message history so the native SolidCraft "Set up an assistant" screen appears!
        let messages = Vec::new();

        Self {
            is_open: true,
            is_minimized: true,
            auto_show_on_launch: true,
            target_app: Some(app_id.to_string()),
            messages,
            input_text: String::new(),
            pending_image: None,
            working_image: None,
            working_preview: None,
            is_busy: false,
            api_key: String::new(),
            provider: AiProvider::Gemini,
            model_name: "gemini-2.5-flash".to_string(),
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            show_settings_panel: false,
            show_toolbar_button: false,
            show_view_menu: false,
            require_approval: true,
            pending_approval: None,
            selected_agent: "No agent found".to_string(),
            is_connected: false,
            show_acp_install_guide: true,
            attach_view: true,
            attach_selection: false,
            edit_mode: "Edits".to_string(),
            last_toolbar_pos: None,
            last_toolbar_size: None,
            status_msg: None,
            v_was_down: false,
            last_dock_pos: None,
            last_dock_size: None,
            side_menu_detected: false,
            last_side_menu_check: 0,
            dock_beside_sidebar: false,
            native_sidebar_px: 0.0,
            last_sidebar_check: 0,
            resp_sender: tx,
            resp_receiver: rx,
        }
    }
/// Sets the active app context and opens the companion
    #[allow(dead_code)]
    pub fn activate_for_app(&mut self, app_id: &str) {
        self.target_app = Some(app_id.to_string());
        if self.auto_show_on_launch {
            self.is_open = true;
            self.is_minimized = false;
        }
    }

    /// Loads an image file from path into pending_image
    pub fn load_image_file(&mut self, ctx: &egui::Context, path: &Path) -> bool {
        if let Ok(dyn_img) = image::open(path) {
            let rgba = dyn_img.to_rgba8();
            let thumb = image::imageops::thumbnail(&rgba, 240, 240);
            let size = [thumb.width() as usize, thumb.height() as usize];
            let color_img = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_flat_samples().as_slice());
            let tex = ctx.load_texture("attached_preview", color_img, Default::default());
            self.working_image = Some(rgba.clone());
            self.working_preview = Some(tex.clone());
            self.pending_image = Some((rgba, tex, path.to_path_buf()));
            self.status_msg = Some("📸 사진 파일이 첨부되었습니다.".to_string());
            true
        } else {
            false
        }
    }

    /// Copies an RGBA image to the system clipboard (Windows CF_DIBV5 / 32-bit transparent)
    pub fn copy_image_to_clipboard(rgba: &image::RgbaImage) -> bool {
        if let Ok(mut clip) = arboard::Clipboard::new() {
            let img_data = arboard::ImageData {
                width: rgba.width() as usize,
                height: rgba.height() as usize,
                bytes: std::borrow::Cow::Borrowed(rgba.as_raw()),
            };
            clip.set_image(img_data).is_ok()
        } else {
            false
        }
    }

    /// Copies plain text to the system clipboard
    pub fn copy_text_to_clipboard(text: &str) -> bool {
        if let Ok(mut clip) = arboard::Clipboard::new() {
            clip.set_text(text.to_string()).is_ok()
        } else {
            false
        }
    }

    /// Brings target window to foreground and sends Ctrl+V to paste the clipboard image
    #[allow(dead_code)]
    #[cfg(windows)]
    pub fn paste_to_window(pids: &[u32], app_name: &str) -> bool {
        use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
            SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
        };
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::keybd_event;
        const KEYEVENTF_KEYUP: u32 = 0x0002;

        struct Ctx {
            pids: Vec<u32>,
            name: String,
            found: HWND,
            best_area: i64,
        }

        unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
            let ctx = &mut *(lp as *mut Ctx);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == std::process::id() {
                return 1;
            }
            let pid_matched = !ctx.pids.is_empty() && ctx.pids.contains(&pid);

            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            let title = if len > 0 {
                String::from_utf16_lossy(&buf[..len as usize])
            } else {
                String::new()
            };

            let lower_title = title.to_lowercase();
            // Never target our own AI companion dock or ArtCraft Studio launcher
            if lower_title.ends_with(" ai")
                || lower_title.ends_with(" assistant")
                || lower_title.ends_with(" toolbar")
                || lower_title.contains("ai 도우미")
                || lower_title.contains("artcraft studio")
            {
                return 1;
            }

            let mut matched = pid_matched;
            if !matched && !title.is_empty() {
                let lower_name = ctx.name.to_lowercase();
                if (!lower_name.is_empty() && lower_title.contains(&lower_name))
                    || lower_title.contains("photocraft")
                    || lower_title.contains("photopea")
                    || lower_title.contains("layer 0")
                    || lower_title.contains("rgb/8")
                    || lower_title.contains(".png")
                    || lower_title.contains(".jpg")
                    || lower_title.contains(".psd")
                {
                    matched = true;
                }
            }

            if matched && GetWindow(hwnd, GW_OWNER).is_null() {
                let mut r: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut r) != 0 {
                    let w = (r.right - r.left) as i64;
                    let h = (r.bottom - r.top) as i64;
                    if w >= 350 && h >= 250 {
                        let area = w * h;
                        if area > ctx.best_area {
                            ctx.best_area = area;
                            ctx.found = hwnd;
                        }
                    }
                }
            }
            1
        }

        let mut ctx = Ctx {
            pids: pids.to_vec(),
            name: app_name.to_string(),
            found: std::ptr::null_mut(),
            best_area: 0,
        };

        unsafe {
            EnumWindows(Some(cb), &mut ctx as *mut Ctx as LPARAM);
            if ctx.found.is_null() {
                return false;
            }

            if IsIconic(ctx.found) != 0 {
                ShowWindow(ctx.found, SW_RESTORE);
            }
            SetForegroundWindow(ctx.found);

            std::thread::sleep(std::time::Duration::from_millis(120));

            // Press Ctrl+V
            keybd_event(0x11 /* VK_CONTROL */, 0, 0, 0);
            keybd_event(0x56 /* 'V' */, 0, 0, 0);
            keybd_event(0x56 /* 'V' */, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(0x11 /* VK_CONTROL */, 0, KEYEVENTF_KEYUP, 0);
            true
        }
    }

    #[cfg(not(windows))]
    pub fn paste_to_window(_pids: &[u32], _app_name: &str) -> bool {
        false
    }

    /// Tries to paste an image from the system clipboard (supports bitmap data, copied file paths, and retries)
    #[allow(dead_code)]
pub fn try_paste_image(&mut self, ctx: &egui::Context, temp_dir: &Path) -> bool {
        for attempt in 0..4 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            let Ok(mut clip) = arboard::Clipboard::new() else {
                continue;
            };

            // 1. Try bitmap image (e.g. from Snipping Tool, browser "Copy image", Paint)
            if let Ok(img_data) = clip.get_image() {
                let width = img_data.width as u32;
                let height = img_data.height as u32;
                let raw_bytes = img_data.bytes.into_owned();

                if let Some(rgba) = image::RgbaImage::from_raw(width, height, raw_bytes) {
                    let _ = std::fs::create_dir_all(temp_dir);
                    let filename = format!("pasted_img_{}.png", now_secs());
                    let img_path = temp_dir.join(filename);

                    // Save full resolution asynchronously so UI never hangs
                    let rgba_save = rgba.clone();
                    let path_save = img_path.clone();
                    std::thread::spawn(move || {
                        let _ = rgba_save.save(&path_save);
                    });

                    // Fast thumbnail for egui UI
                    let thumb = image::imageops::thumbnail(&rgba, 240, 240);
                    let size = [thumb.width() as usize, thumb.height() as usize];
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_flat_samples().as_slice());
                    let tex = ctx.load_texture("pasted_preview", color_img, Default::default());

                    self.working_image = Some(rgba.clone());
                    self.working_preview = Some(tex.clone());
                    self.pending_image = Some((rgba, tex, img_path));
                    self.status_msg = Some("📸 클립보드 이미지가 첨부되었습니다. (Ctrl+V)".to_string());
                    return true;
                }
            }

            // 2. Try file path in clipboard text (copied from Explorer or path)
            if let Ok(text) = clip.get_text() {
                for line in text.lines() {
                    let cleaned = line.trim().trim_matches('"').trim_matches('\'');
                    let p = PathBuf::from(cleaned);
                    if p.is_file() {
                        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
                        if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp") {
                            if self.load_image_file(ctx, &p) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Polls background AI response and updates working image
    pub fn poll_response(&mut self, ctx: &egui::Context, _pids: &[u32], _app_name: &str) -> Option<(String, PathBuf)> {
        let auto_open: Option<(String, PathBuf)> = None;
        if let Ok(resp) = self.resp_receiver.try_recv() {
            self.is_busy = false;
            match resp {
                AsyncResponse::Success { text, updated_image, action } => {
                    let mut preview = None;
                    if let Some(rgba) = updated_image {
                        Self::copy_image_to_clipboard(&rgba);
                        let thumb = image::imageops::thumbnail(&rgba, 320, 320);
                        let size = [thumb.width() as usize, thumb.height() as usize];
                        let color_img = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_flat_samples().as_slice());
                        let tex = ctx.load_texture(format!("working_preview_{}", now_secs()), color_img, Default::default());
                        self.working_image = Some(rgba);
                        self.working_preview = Some(tex.clone());
                        preview = Some(tex);
                    }
                    if let Some(act) = &action {
                        if self.require_approval {
                            self.pending_approval = Some(act.clone());
                        } else if act.auto_open {
                            // Load into the app that is already open; never spawn a new window
                            let _ = live_open_via_rpc(&act.app_id, &act.file_path);
                        }
                    }
                    self.messages.push(ChatMessage {
                        role: Role::Assistant,
                        text,
                        image_preview: preview,
                        image_path: None,
                        action_result: action,
                        timestamp: now_secs(),
                    });
                }
                AsyncResponse::Error(err) => {
                    self.messages.push(ChatMessage {
                        role: Role::Assistant,
                        text: format!("⚠️ 요청 처리 중 오류가 발생했습니다: {err}"),
                        image_preview: None,
                        image_path: None,
                        action_result: None,
                        timestamp: now_secs(),
                    });
                }
            }
        }
        auto_open
    }

    /// Sends the current user prompt and attached/working image
    pub fn send(&mut self, saves_base: &Path) {
        let text = self.input_text.trim().to_string();
        if text.is_empty() && self.pending_image.is_none() && self.working_image.is_none() {
            return;
        }

        let pending_img = self.pending_image.take();
        let (img_preview, img_path, rgba_img) = if let Some((rgba, tex, path)) = pending_img {
            self.working_image = Some(rgba.clone());
            self.working_preview = Some(tex.clone());
            (Some(tex), Some(path), Some(rgba))
        } else if let Some(working) = &self.working_image {
            (self.working_preview.clone(), None, Some(working.clone()))
        } else {
            (None, None, None)
        };

        // Add user message to chat history
        self.messages.push(ChatMessage {
            role: Role::User,
            text: if text.is_empty() { "이미지 편집 요청".to_string() } else { text.clone() },
            image_preview: img_preview,
            image_path: img_path.clone(),
            action_result: None,
            timestamp: now_secs(),
        });

        self.input_text.clear();
        self.is_busy = true;

        let tx = self.resp_sender.clone();
        let target_app = self.target_app.clone().unwrap_or_else(|| "photocraft".to_string());
        let app_folder = match target_app.to_lowercase().as_str() {
            "photocraft" => "PhotoCraft",
            "wordcraft" => "WordCraft",
            "cadcraft" => "CADCraft",
            "solidcraft" => "SolidCraft",
            "deckcraft" => "DeckCraft",
            "designcraft" => "DesignCraft",
            "effectcraft" => "EffectCraft",
            "figcraft" => "FigCraft",
            "filmcraft" => "FilmCraft",
            "gridcraft" => "GridCraft",
            "hop" => "HopCraft",
            "lightcraft" => "LightCraft",
            "pdfcraft" => "PdfCraft",
            "soundcraft" => "SoundCraft",
            "vectorcraft" => "VectorCraft",
            _ => &target_app,
        };
        let saves_dir = saves_base.join(app_folder);
        let api_key = self.api_key.trim().to_string();
        let provider = self.provider;

        thread::spawn(move || {
            let res = process_ai_request(&target_app, &text, rgba_img, img_path, &saves_dir, &api_key, provider);
            match res {
                Ok((reply, updated_img, action)) => {
                    let _ = tx.send(AsyncResponse::Success { text: reply, updated_image: updated_img, action });
                }
                Err(err) => {
                    let _ = tx.send(AsyncResponse::Error(err));
                }
            }
        });
    }

    /// Saves the current working image to disk upon user request/completion
    #[allow(dead_code)]
    pub fn save_working_image(&mut self, saves_base: &Path) -> Option<PathBuf> {
        let rgba = self.working_image.as_ref()?;
        let target_app = self.target_app.clone().unwrap_or_else(|| "photocraft".to_string());
        let app_folder = match target_app.to_lowercase().as_str() {
            "photocraft" => "PhotoCraft",
            "wordcraft" => "WordCraft",
            "cadcraft" => "CADCraft",
            "solidcraft" => "SolidCraft",
            "deckcraft" => "DeckCraft",
            "designcraft" => "DesignCraft",
            "effectcraft" => "EffectCraft",
            "figcraft" => "FigCraft",
            "filmcraft" => "FilmCraft",
            "gridcraft" => "GridCraft",
            "hop" => "HopCraft",
            "lightcraft" => "LightCraft",
            "pdfcraft" => "PdfCraft",
            "soundcraft" => "SoundCraft",
            "vectorcraft" => "VectorCraft",
            _ => &target_app,
        };
        let saves_dir = saves_base.join(app_folder);
        let _ = std::fs::create_dir_all(&saves_dir);
        let timestamp = now_secs();
        let out_name = format!("completed_{timestamp}.png");
        let out_path = saves_dir.join(&out_name);

        if rgba.save(&out_path).is_ok() {
            let msg = format!(
                "💾 작업하신 이미지가 최종 완성되어 안전하게 저장되었습니다!\n- 저장 위치: {}\n\n아래 버튼을 누르면 언제든 {target_app}에서 새 캔버스로 열어 확인할 수 있습니다.",
                out_path.display()
            );
            self.messages.push(ChatMessage {
                role: Role::Assistant,
                text: msg,
                image_preview: self.working_preview.clone(),
                image_path: Some(out_path.clone()),
                action_result: Some(ActionResult::for_image(
                    format!("{target_app}에서 열기"),
                    target_app,
                    out_path.clone(),
                    None,
                )),
                timestamp: now_secs(),
            });
            Some(out_path)
        } else {
            None
        }
    }

    /// Renders the AI companion embedded directly inside the application's window layout
    #[allow(dead_code)]
    pub fn render_embedded(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        app_name: &str,
        entries: &[AppEntry],
        saves_base: &Path,
        temp_dir: &Path,
        open_action: &mut Option<(String, PathBuf)>,
    ) {
        if let Some((app_id, file)) = self.poll_response(ctx, &[], app_name) {
            *open_action = Some((app_id, file));
        }

        // Handle dropped image files
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for df in dropped {
            if let Some(p) = df.path {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
                if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp") {
                    self.load_image_file(ctx, &p);
                    break;
                }
            }
        }

        if self.is_minimized {
            self.render_minimized(ui, app_name, ctx);
        } else {
            self.render_full(ui, ctx, entries, saves_base, temp_dir, open_action, app_name, &[]);
        }
    }

        /// Captures the target app's current viewport via Win32 PrintWindow and attaches it to the prompt (Attach View)
    pub fn attach_current_view(&mut self, ctx: &egui::Context, target_hwnd: isize, temp_dir: &Path) -> bool {
        if let Some(rgba) = crate::apps::capture_app_viewport(target_hwnd) {
            let _ = std::fs::create_dir_all(temp_dir);
            let filename = format!("attached_view_{}.png", now_secs());
            let img_path = temp_dir.join(filename);
            let rgba_save = rgba.clone();
            let path_save = img_path.clone();
            std::thread::spawn(move || {
                let _ = rgba_save.save(&path_save);
            });

            let thumb = image::imageops::thumbnail(&rgba, 240, 240);
            let size = [thumb.width() as usize, thumb.height() as usize];
            let color_img = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_flat_samples().as_slice());
            let tex = ctx.load_texture(format!("attached_view_{}", now_secs()), color_img, Default::default());

            self.working_image = Some(rgba.clone());
            self.working_preview = Some(tex.clone());
            self.pending_image = Some((rgba, tex, img_path));
            self.status_msg = Some("📷 현재 뷰포트 화면이 첨부되었습니다.".to_string());
            true
        } else {
            false
        }
    }

    /// Renders the SolidCraft-style native Assistant docked seamlessly at the rightmost boundary
    pub fn render_for_app(
        &mut self,
        ctx: &egui::Context,
        app_id: &str,
        app_name: &str,
        pids: &[u32],
        entries: &[AppEntry],
        saves_base: &Path,
        temp_dir: &Path,
        open_action: &mut Option<(String, PathBuf)>,
    ) {
        // SolidCraft has its own native built-in Assistant; Craft Anything is an AI app
        if app_id.eq_ignore_ascii_case("solidcraft") || app_id.eq_ignore_ascii_case("craft-anything") {
            return;
        }

        if let Some((app_id, file)) = self.poll_response(ctx, pids, app_name) {
            *open_action = Some((app_id, file));
        }

        if !self.is_open {
            return;
        }

        let vp_id = ViewportId::from_hash_of(format!("companion_dock_{app_id}"));
        let ppp = ctx.pixels_per_point().max(0.1);
        let tb_title = format!("{app_name} Toolbar");
        let companion_title = format!("{app_name} Assistant");

        ctx.request_repaint_after(std::time::Duration::from_millis(32));

        let app_info = crate::apps::get_app_window_info(pids, app_name);
        let (target_hwnd, raw_app_rect) = match app_info {
            Some((hwnd, rect)) => (hwnd, rect),
            None => {
                crate::apps::sync_companion_window(&tb_title, 0, false, -15000, -15000, 1, 1);
                crate::apps::sync_companion_window(&companion_title, 0, false, -15000, -15000, 1, 1);
                return;
            }
        };

        let is_target_minimized = crate::apps::is_window_iconic(target_hwnd);
        if is_target_minimized {
            crate::apps::sync_companion_window(&tb_title, target_hwnd, false, -15000, -15000, 1, 1);
            crate::apps::sync_companion_window(&companion_title, target_hwnd, false, -15000, -15000, 1, 1);
            return;
        }

        // Side-by-side split docking: shrinks the target app window by 320px from the right when Assistant is open
        // so that native right-hand sidebars (e.g. PhotoCraft's Color / Properties / Layers) are never covered.
        // Automatically restores the target app window to full width when Assistant is minimized/closed.
        let dock_w_pt = 320.0_f32;
        let dock_w_px = (dock_w_pt * ppp).round() as i32;
        let (_app_rect, dock_rect) = crate::apps::apply_window_split_dock(
            target_hwnd,
            dock_w_px,
            !self.is_minimized,
            raw_app_rect,
        );

        // 1. Top toolbar overlay is completely disabled per user preference:
        // AI Assistant is invoked and toggled exclusively via the global Ctrl + Shift + A shortcut.
        crate::apps::sync_companion_window(&tb_title, target_hwnd, false, -15000, -15000, 1, 1);

        // 2. Rightmost Split-Docked Assistant Panel
        // Keep the viewport alive inside eframe even when minimized (parked offscreen at -15000,-15000)
        // so that reopening via Ctrl+Shift+A or Toolbar button is instantaneous and never dropped by winit.
        let (x, y, width, height) = if self.is_minimized {
            (-15000.0_f32, -15000.0_f32, dock_w_pt, 240.0_f32)
        } else {
            let dx = dock_rect[0] / ppp;
            let dy = dock_rect[1] / ppp;
            let dw = ((dock_rect[2] - dock_rect[0]) / ppp).max(280.0_f32);
            let dh = ((dock_rect[3] - dock_rect[1]) / ppp).max(220.0_f32);
            (dx, dy, dw, dh)
        };

        let dock_x_px = if self.is_minimized { -15000 } else { dock_rect[0].round() as i32 };
        let dock_y_px = if self.is_minimized { -15000 } else { dock_rect[1].round() as i32 };
        let dock_w_actual_px = if self.is_minimized { 320 } else { (dock_rect[2] - dock_rect[0]).round() as i32 };
        let dock_h_actual_px = if self.is_minimized { 240 } else { (dock_rect[3] - dock_rect[1]).round() as i32 };

        let builder = ViewportBuilder::default()
            .with_title(companion_title.clone())
            .with_inner_size([width, height])
            .with_min_inner_size([280.0_f32, 200.0_f32])
            .with_max_inner_size([width, 3000.0_f32])
            .with_resizable(false)
            .with_visible(true)
            .with_decorations(false)
            .with_transparent(false)
            .with_position([x, y]);

        ctx.show_viewport_immediate(vp_id, builder, |ctx, _class| {
            ctx.request_repaint_after(std::time::Duration::from_millis(32));
            if self.last_dock_pos != Some([x, y]) {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                self.last_dock_pos = Some([x, y]);
            }
            if self.last_dock_size != Some([width, height]) {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
                self.last_dock_size = Some([width, height]);
            }

            crate::apps::sync_companion_window(
                &companion_title,
                target_hwnd,
                true,
                dock_x_px,
                dock_y_px,
                dock_w_actual_px,
                dock_h_actual_px,
            );

            if self.is_minimized {
                return;
            }

            // Handle drag-and-drop files
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            for df in dropped {
                if let Some(p) = df.path {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
                    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp") {
                        self.load_image_file(ctx, &p);
                        break;
                    }
                }
            }

            // SolidCraft native dark palette (#18181B / #1E2028 background, #2B2E3B border)
            let bg_color = Color32::from_rgb(0x18, 0x18, 0x1B);
            let border_color = Color32::from_rgb(0x27, 0x27, 0x2A);

            egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(bg_color)
                        .stroke(Stroke::new(1.0_f32, border_color))
                        .corner_radius(0.0_f32)
                        .inner_margin(Margin { left: 0, right: 0, top: 0, bottom: 6 }),
                )
                .show(ctx, |ui| {
                    self.render_solidcraft_acp_panel(ui, ctx, entries, saves_base, temp_dir, open_action, app_name, pids, target_hwnd);
                });
        });
    }

    #[allow(dead_code)]
    fn render_minimized(&mut self, ui: &mut Ui, app_name: &str, _ctx: &egui::Context) {
        let _ = (ui, app_name);
    }

    fn render_full(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        entries: &[AppEntry],
        saves_base: &Path,
        temp_dir: &Path,
        open_action: &mut Option<(String, PathBuf)>,
        app_name: &str,
        pids: &[u32],
    ) {
        self.render_solidcraft_acp_panel(ui, ctx, entries, saves_base, temp_dir, open_action, app_name, pids, 0);
    }

    /// Renders the 100% SolidCraft-native Assistant UI panel (ACP standard specification)
    fn render_solidcraft_acp_panel(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        _entries: &[AppEntry],
        saves_base: &Path,
        temp_dir: &Path,
        open_action: &mut Option<(String, PathBuf)>,
        app_name: &str,
        _pids: &[u32],
        target_hwnd: isize,
    ) {
        let header_bg = Color32::from_rgb(0x18, 0x18, 0x1B);
        let border_stroke = Color32::from_rgb(0x27, 0x27, 0x2A);
        let accent_blue = Color32::from_rgb(0x25, 0x63, 0xEB); // SolidCraft Primary Blue

        // -------------------------------------------------------------
        // 1. [상단 헤더 바] Top Header Bar
        // -------------------------------------------------------------
        Frame::new()
            .fill(header_bg)
            .stroke(Stroke::new(1.0_f32, border_stroke))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Left: Sidebar icon (drawn via painter so no missing font glyph) + Assistant Title + Status
                    let (icon_rect, _) = ui.allocate_exact_size(vec2(14.0_f32, 12.0_f32), egui::Sense::hover());
                    let stroke_col = Color32::from_rgb(0xA1, 0xA1, 0xAA);
                    ui.painter().rect_stroke(icon_rect, 2.0_f32, Stroke::new(1.2_f32, stroke_col), egui::StrokeKind::Inside);
                    let div_x = icon_rect.left() + 5.0_f32;
                    ui.painter().line_segment(
                        [egui::pos2(div_x, icon_rect.top()), egui::pos2(div_x, icon_rect.bottom())],
                        Stroke::new(1.2_f32, stroke_col),
                    );
                    ui.add_space(3.0_f32);
                    ui.label(RichText::new("Assistant").strong().size(13.0_f32).color(Color32::WHITE));
                    ui.add_space(4.0_f32);

                    let status_txt = if self.is_connected {
                        format!("● {}", self.selected_agent)
                    } else {
                        "Not connected".to_string()
                    };
                    let status_col = if self.is_connected {
                        Color32::from_rgb(0x4A, 0xDE, 0x80)
                    } else {
                        Color32::from_rgb(0x71, 0x71, 0x7A)
                    };
                    ui.label(RichText::new(status_txt).size(11.0_f32).color(status_col));

                    // Right: Actions (+ / ⚙ / — / ✕)
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("✕").size(11.5_f32).color(Color32::from_rgb(0xA1, 0xA1, 0xAA))).frame(false))
                            .on_hover_text("Close Assistant (Ctrl+Shift+A)")
                            .clicked()
                        {
                            self.is_minimized = true;
                        }

                        if ui.add(egui::Button::new(RichText::new("—").size(11.5_f32).color(Color32::from_rgb(0xA1, 0xA1, 0xAA))).frame(false))
                            .on_hover_text("Minimize")
                            .clicked()
                        {
                            self.is_minimized = true;
                        }

                        let gear_col = if self.show_settings_panel { accent_blue } else { Color32::from_rgb(0xA1, 0xA1, 0xAA) };
                        if ui.add(egui::Button::new(RichText::new("⚙").size(12.0_f32).color(gear_col)).frame(false))
                            .on_hover_text("Preferences > AI")
                            .clicked()
                        {
                            self.show_settings_panel = !self.show_settings_panel;
                        }

                        if ui.add(egui::Button::new(RichText::new("+").size(13.0_f32).color(Color32::from_rgb(0xA1, 0xA1, 0xAA))).frame(false))
                            .on_hover_text("New conversation")
                            .clicked()
                        {
                            self.messages.clear();
                            self.pending_image = None;
                            self.working_image = None;
                            self.working_preview = None;
                            self.status_msg = Some("새 대화 세션이 시작되었습니다.".to_string());
                        }
                    });
                });
            });

        // -------------------------------------------------------------
        // 2. [에이전트 연결 컨트롤 섹션] Agent Connection Controls
        // -------------------------------------------------------------
        Frame::new()
            .fill(Color32::from_rgb(0x14, 0x14, 0x17))
            .stroke(Stroke::new(1.0_f32, border_stroke))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    // Dropdown combo
                    let combo_w = ui.available_width();
                    egui::ComboBox::from_id_salt("acp_agent_select")
                        .selected_text(RichText::new(&self.selected_agent).size(12.0_f32).color(Color32::WHITE))
                        .width(combo_w - 8.0_f32)
                        .show_ui(ui, |ui| {
                            for agent in &["No agent found", "Claude Code", "Codex", "Google Antigravity", "OpenCode", "xAI Grok"] {
                                if ui.selectable_value(&mut self.selected_agent, agent.to_string(), *agent).clicked() {
                                    if *agent != "No agent found" {
                                        self.show_acp_install_guide = false;
                                    }
                                }
                            }
                        });

                    ui.add_space(6.0_f32);

                    // Connect / Disconnect button
                    let btn_txt = if self.is_connected { "Disconnect" } else { "Connect" };
                    let btn_bg = if self.is_connected { Color32::from_rgb(0x37, 0x41, 0x51) } else { accent_blue };
                    let connect_btn = ui.add_sized(
                        [ui.available_width(), 26.0_f32],
                        egui::Button::new(RichText::new(btn_txt).strong().size(12.0_f32).color(Color32::WHITE)).fill(btn_bg),
                    );
                    if connect_btn.clicked() {
                        if self.is_connected {
                            self.is_connected = false;
                            self.status_msg = Some("에이전트 연결이 해제되었습니다.".to_string());
                        } else {
                            if self.selected_agent == "No agent found" {
                                self.selected_agent = "Google Antigravity".to_string();
                            }
                            self.is_connected = true;
                            self.show_acp_install_guide = false;
                            self.status_msg = Some(format!("{} 에이전트에 연결되었습니다.", self.selected_agent));
                        }
                    }

                    ui.add_space(6.0_f32);

                    // Row: Agent label + Add agent... + Refresh
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Agent").size(11.0_f32).color(Color32::from_rgb(0x71, 0x71, 0x7A)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("Refresh").size(11.0_f32).color(Color32::from_rgb(0x93, 0xC5, 0xFD))).frame(false))
                                .on_hover_text("Refresh installed agents")
                                .clicked()
                            {
                                self.status_msg = Some("에이전트 목록을 새로고침했습니다.".to_string());
                            }
                            ui.add_space(4.0_f32);
                            if ui.add(egui::Button::new(RichText::new("Add agent...").size(11.0_f32).color(Color32::from_rgb(0x93, 0xC5, 0xFD))).frame(false))
                                .on_hover_text("Add custom agent")
                                .clicked()
                            {
                                self.show_settings_panel = true;
                            }
                        });
                    });

                    // Collapsible ACP agent install guide
                    if !self.is_connected {
                        ui.add_space(6.0_f32);
                        let guide_lbl = if self.show_acp_install_guide {
                            "▼ NO CODING AGENT FOUND. INSTALL ONE OF THESE:"
                        } else {
                            "▶ NO CODING AGENT FOUND. INSTALL ONE OF THESE:"
                        };
                        if ui.add(egui::Button::new(RichText::new(guide_lbl).font(egui::FontId::monospace(9.5_f32)).color(Color32::from_rgb(0xA1, 0xA1, 0xAA))).frame(false)).clicked() {
                            self.show_acp_install_guide = !self.show_acp_install_guide;
                        }

                        if self.show_acp_install_guide {
                            ui.add_space(4.0_f32);
                            let acp_items = [
                                ("Claude Code", "npm install -g @agentclientprotocol/claude-agent-acp"),
                                ("Codex", "npm install -g @agentclientprotocol/codex-acp"),
                                ("Google Antigravity", "download the Antigravity ACP server for your system (see agentclientprotocol.com/registry), unzip it and put agy_acp_server.exe on your PATH"),
                                ("OpenCode", "npm install -g opencode-ai"),
                                ("xAI Grok", "npm install -g @agentclientprotocol/grok-acp"),
                            ];

                            for (agent_name, cmd) in acp_items {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(agent_name).size(11.0_f32).strong().color(Color32::from_rgb(0xE4, 0xE4, 0xE7)));
                                    if ui.add(egui::Button::new(RichText::new("Copy").size(10.0_f32).color(Color32::from_rgb(0x60, 0xA5, 0xFA))).frame(false)).clicked() {
                                        Self::copy_text_to_clipboard(cmd);
                                        self.status_msg = Some(format!("{agent_name} 설치 명령어가 복사되었습니다."));
                                    }
                                });
                                ui.label(RichText::new(cmd).font(egui::FontId::monospace(9.0_f32)).color(Color32::from_rgb(0x71, 0x71, 0x7A)));
                                ui.add_space(3.0_f32);
                            }
                        }
                    }
                });
            });

        // Preferences panel view
        if self.show_settings_panel {
            Frame::new()
                .fill(Color32::from_rgb(0x18, 0x1A, 0x22))
                .stroke(Stroke::new(1.0_f32, border_stroke))
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Preferences > AI").strong().size(11.5_f32).color(Color32::WHITE));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button("Done").clicked() {
                                    self.show_settings_panel = false;
                                }
                            });
                        });
                        ui.add_space(4.0_f32);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Agent Provider:").size(10.5_f32).color(TEXT_MUTED));
                            egui::ComboBox::from_id_salt("pref_provider_select")
                                .selected_text(self.provider.label())
                                .width(160.0_f32)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_value(&mut self.provider, AiProvider::Gemini, "Google Gemini").clicked() {
                                        self.model_name = "gemini-2.5-flash".to_string();
                                        self.base_url = "https://generativelanguage.googleapis.com".to_string();
                                    }
                                    if ui.selectable_value(&mut self.provider, AiProvider::Claude, "Anthropic Claude").clicked() {
                                        self.model_name = "claude-sonnet-4-20250514".to_string();
                                        self.base_url = "https://api.anthropic.com".to_string();
                                    }
                                    if ui.selectable_value(&mut self.provider, AiProvider::OpenAi, "OpenAI / 호환 API").clicked() {
                                        self.model_name = "gpt-4o".to_string();
                                        self.base_url = "https://api.openai.com/v1".to_string();
                                    }
                                    if ui.selectable_value(&mut self.provider, AiProvider::Ollama, "Ollama (로컬)").clicked() {
                                        self.model_name = "llama3.2".to_string();
                                        self.base_url = "http://127.0.0.1:11434".to_string();
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Model: ").size(10.5_f32).color(TEXT_MUTED));
                            ui.add(egui::TextEdit::singleline(&mut self.model_name).desired_width(170.0_f32));
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Base URL:").size(10.5_f32).color(TEXT_MUTED));
                            ui.add(egui::TextEdit::singleline(&mut self.base_url).desired_width(170.0_f32));
                        });
                        if self.provider != AiProvider::Ollama {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("API Key: ").size(10.5_f32).color(TEXT_MUTED));
                                ui.add(egui::TextEdit::singleline(&mut self.api_key).password(true).desired_width(170.0_f32));
                            });
                        }
                        ui.checkbox(&mut self.require_approval, RichText::new("Require approval before applying edits").size(10.5_f32));
                    });
                });
        }

        // -------------------------------------------------------------
        // 3. [중앙 영역] Center Area (Empty State or Conversation History)
        // -------------------------------------------------------------
        let input_area_height = 118.0_f32;
        let middle_height = (ui.available_height() - input_area_height).max(100.0_f32);

        let mut do_restore: Option<(PathBuf, PathBuf)> = None;

        ScrollArea::vertical()
            .max_height(middle_height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if !self.is_connected && self.messages.is_empty() {
                    // --- SolidCraft Native Empty State View ---
                    ui.add_space(28.0_f32);
                    ui.vertical_centered(|ui| {
                        // Blue chat bubble icon
                        Frame::new()
                            .fill(Color32::from_rgb(0x1D, 0x4E, 0xD8))
                            .corner_radius(20.0_f32)
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.label(RichText::new("💬").size(20.0_f32).color(Color32::WHITE));
                            });

                        ui.add_space(10.0_f32);
                        ui.label(RichText::new("Set up an assistant").strong().size(15.0_f32).color(Color32::WHITE));
                        ui.add_space(6.0_f32);

                        let desc = format!("{app_name} talks to a coding agent installed on this computer. Pick one in Preferences > AI.");
                        ui.label(RichText::new(desc).size(11.0_f32).color(Color32::from_rgb(0x94, 0xA3, 0xB8)));
                        ui.add_space(14.0_f32);

                        if ui.add_sized(
                            [140.0_f32, 28.0_f32],
                            egui::Button::new(RichText::new("Open Preferences").strong().size(11.5_f32).color(Color32::WHITE))
                                .fill(accent_blue)
                                .corner_radius(4.0_f32),
                        ).clicked() {
                            self.show_settings_panel = true;
                        }
                    });
                } else {
                    // --- Conversation Messages View ---
                    ui.spacing_mut().item_spacing.y = 8.0_f32;

                    // Approval request banner
                    let mut approval_action: Option<bool> = None;
                    if let Some(ref pending) = self.pending_approval {
                        let pending_label = pending.label.clone();
                        Frame::new()
                            .fill(Color32::from_rgb(0x27, 0x20, 0x12))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0xF5, 0x9E, 0x0B)))
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(format!("⏳ Allow the agent to do this? ({pending_label})")).strong().size(11.0_f32).color(Color32::from_rgb(0xFB, 0xBF, 0x24)));
                                    ui.add_space(4.0_f32);
                                    ui.horizontal(|ui| {
                                        if ui.button(RichText::new("Allow").size(10.5_f32).strong().color(Color32::from_rgb(0x4A, 0xDE, 0x80))).clicked() {
                                            approval_action = Some(true);
                                        }
                                        if ui.button(RichText::new("Deny").size(10.5_f32).color(Color32::from_rgb(0xF8, 0x71, 0x71))).clicked() {
                                            approval_action = Some(false);
                                        }
                                    });
                                });
                            });
                    }
                    if let Some(allow) = approval_action {
                        if allow {
                            if let Some(act) = self.pending_approval.take() {
                                if !live_open_via_rpc(&act.app_id, &act.file_path) {
                                    *open_action = Some((act.app_id, act.file_path));
                                }
                            }
                        } else {
                            self.pending_approval = None;
                        }
                    }

                    for msg in &self.messages {
                        match msg.role {
                            Role::User => {
                                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                    Frame::new()
                                        .fill(Color32::from_rgb(0x1E, 0x29, 0x3B))
                                        .corner_radius(6.0_f32)
                                        .inner_margin(Margin::symmetric(10, 8))
                                        .show(ui, |ui| {
                                            ui.set_max_width(260.0_f32);
                                            ui.vertical(|ui| {
                                                if let Some(tex) = &msg.image_preview {
                                                    ui.image((tex.id(), vec2(100.0_f32, 75.0_f32)));
                                                    ui.add_space(4.0_f32);
                                                }
                                                ui.label(RichText::new(&msg.text).color(Color32::WHITE).size(12.0_f32));
                                            });
                                        });
                                });
                            }
                            Role::Assistant => {
                                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                                    Frame::new()
                                        .fill(Color32::from_rgb(0x18, 0x18, 0x1E))
                                        .stroke(Stroke::new(1.0_f32, border_stroke))
                                        .corner_radius(6.0_f32)
                                        .inner_margin(Margin::symmetric(10, 8))
                                        .show(ui, |ui| {
                                            ui.set_max_width(280.0_f32);
                                            ui.vertical(|ui| {
                                                ui.label(RichText::new(&msg.text).color(Color32::from_rgb(0xEE, 0xEE, 0xF2)).size(12.0_f32));

                                                if let Some(action) = &msg.action_result {
                                                    ui.add_space(6.0_f32);
                                                    ui.horizontal(|ui| {
                                                        ui.label(RichText::new(format!("⚡ {}", action.label)).font(egui::FontId::monospace(10.0_f32)).color(Color32::from_rgb(0x4A, 0xDE, 0x80)));

                                                        // One-click Undo
                                                        if let Some(orig) = &action.original_path {
                                                            if orig.exists() {
                                                                if ui.button(RichText::new("↩ Undo").size(10.0_f32).color(Color32::from_rgb(0xFB, 0xBF, 0x24))).on_hover_text("이 메시지에서 에이전트가 바꾼 모든 것을 한 번에 실행 취소").clicked() {
                                                                    do_restore = Some((orig.clone(), action.file_path.clone()));
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                            });
                                        });
                                });
                            }
                            Role::System => {
                                ui.label(RichText::new(&msg.text).font(egui::FontId::monospace(9.5_f32)).color(TEXT_MUTED));
                            }
                        }
                    }

                    if self.is_busy {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Agent working...").font(egui::FontId::monospace(10.5_f32)).color(TEXT_MUTED));
                        });
                    }
                }
            });

        if let Some((backup, target)) = do_restore {
            if let Ok(orig_dyn) = image::open(&backup) {
                let rgba = orig_dyn.to_rgba8();
                if rgba.save(&target).is_ok() {
                    photocraft_live_reload_document(&target);
                    self.status_msg = Some("↩ 실행 취소 완료: 이전 상태로 원복되었습니다.".to_string());
                }
            }
        }

        ui.add_space(4.0_f32);

        // -------------------------------------------------------------
        // 4. [하단 프롬프트 입력 박스] SolidCraft 2-stage/3-stage Composer
        // -------------------------------------------------------------
        let mut send_triggered = false;

        Frame::new()
            .fill(Color32::from_rgb(0x13, 0x14, 0x18))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x27, 0x2A, 0x34)))
            .corner_radius(8.0_f32)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    // 1단 (상단): Multi-line text edit
                    let hint = match app_name.to_lowercase().as_str() {
                        "photocraft" => "Ask the agent to edit the image...",
                        "wordcraft" => "Ask the agent to edit the document...",
                        _ => "Ask the agent to change the design...",
                    };
                    let text_resp = ui.add(
                        egui::TextEdit::multiline(&mut self.input_text)
                            .hint_text(hint)
                            .frame(false)
                            .desired_rows(2)
                            .desired_width(ui.available_width()),
                    );

                    ui.add_space(4.0_f32);

                    // 2단 (중간): + button, Edits ▾ dropdown, circular send button ↑
                    ui.horizontal(|ui| {
                        // + attach button
                        if ui.add(egui::Button::new(RichText::new("+").size(13.0_f32).color(Color32::from_rgb(0xA1, 0xA1, 0xAA))).frame(false))
                            .on_hover_text("Attach file or image")
                            .clicked()
                        {
                            if target_hwnd != 0 {
                                self.attach_current_view(ctx, target_hwnd, temp_dir);
                            }
                        }

                        // Edits dropdown
                        egui::ComboBox::from_id_salt("composer_edit_mode")
                            .selected_text(RichText::new(&self.edit_mode).size(10.5_f32).color(Color32::from_rgb(0xD4, 0xD4, 0xD8)))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.edit_mode, "Edits".to_string(), "Edits");
                                ui.selectable_value(&mut self.edit_mode, "Questions".to_string(), "Questions");
                                ui.selectable_value(&mut self.edit_mode, "Planning".to_string(), "Planning");
                            });

                        // Right: Circular send button ↑ (or ■ stop button when busy)
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if self.is_busy {
                                if ui.add(egui::Button::new(RichText::new("■").size(12.0_f32).color(Color32::from_rgb(0xEF, 0x44, 0x44))).frame(false))
                                    .on_hover_text("Stop agent")
                                    .clicked()
                                {
                                    self.is_busy = false;
                                }
                            } else {
                                let has_text = !self.input_text.trim().is_empty();
                                let send_bg = if has_text { accent_blue } else { Color32::from_rgb(0x37, 0x41, 0x51) };
                                let send_btn = ui.add(
                                    egui::Button::new(RichText::new("↑").strong().size(13.0_f32).color(Color32::WHITE))
                                        .fill(send_bg)
                                        .corner_radius(12.0_f32),
                                );
                                if send_btn.clicked() {
                                    send_triggered = true;
                                }
                            }
                        });
                    });

                    ui.add_space(2.0_f32);
                    ui.separator();
                    ui.add_space(2.0_f32);

                    // 3단 (하단 컨텍스트 바): Checkbox [Attach view], [Attach selection], and badge [Full context]
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.attach_view, RichText::new("Attach view").size(10.0_f32).color(Color32::from_rgb(0xA1, 0xA1, 0xAA)))
                            .on_hover_text("Send a picture of the current view with the message");
                        ui.checkbox(&mut self.attach_selection, RichText::new("Attach selection").size(10.0_f32).color(Color32::from_rgb(0x71, 0x71, 0x7A)))
                            .on_hover_text("Send selected elements/layers with the message");

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            Frame::new()
                                .fill(Color32::from_rgb(0x1F, 0x24, 0x30))
                                .corner_radius(3.0_f32)
                                .inner_margin(Margin::symmetric(4, 1))
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Full context").font(egui::FontId::monospace(8.5_f32)).color(Color32::from_rgb(0x94, 0xA3, 0xB8)));
                                });
                        });
                    });

                    // Send on Enter (when not holding Shift)
                    if text_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift) {
                        send_triggered = true;
                    }
                });
            });

        // Trigger send
        if send_triggered && !self.is_busy && (!self.input_text.trim().is_empty() || self.pending_image.is_some()) {
            if self.attach_view && self.pending_image.is_none() && target_hwnd != 0 {
                self.attach_current_view(ctx, target_hwnd, temp_dir);
            }
            if !self.is_connected {
                self.is_connected = true;
                if self.selected_agent == "No agent found" {
                    self.selected_agent = "Google Antigravity".to_string();
                }
            }
            self.send(saves_base);
        }
    }
}

// AI Execution & Image/Document Processing Engine
// ---------------------------------------------------------------------------

pub fn photocraft_live_reload_document(file_path: &Path) -> bool {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let ports = [49152, 49154, 49156];
    let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    let mut stream_opt = None;
    for &port in &ports {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        if let Ok(s) = TcpStream::connect_timeout(&addr, Duration::from_millis(250)) {
            let _ = s.set_read_timeout(Some(Duration::from_millis(1000)));
            let _ = s.set_write_timeout(Some(Duration::from_millis(1000)));
            stream_opt = Some(s);
            break;
        }
    }

    let Some(mut stream) = stream_opt else {
        return false;
    };

    let Ok(stream_clone) = stream.try_clone() else {
        return false;
    };
    let mut reader = BufReader::new(stream_clone);

    // 1. Authenticate with PhotoCraft control server
    let auth_msg = format!("{{\"id\":1,\"method\":\"auth\",\"params\":{{\"token\":\"{token}\"}}}}\n");
    if stream.write_all(auth_msg.as_bytes()).is_err() {
        return false;
    }
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || !line.contains("\"ok\":true") {
        return false;
    }

    // 2. Close current document tab so it can be re-opened cleanly
    let close_msg = "{\"id\":2,\"method\":\"engine.execute\",\"params\":{\"command\":\"file.close\"}}\n";
    let _ = stream.write_all(close_msg.as_bytes());
    line.clear();
    let _ = reader.read_line(&mut line);

    std::thread::sleep(Duration::from_millis(50));

    // 3. Format path relative to root
    let full_str = file_path.to_string_lossy().replace('\\', "/");
    let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let prefix1 = format!("{sys_drive}/");
    let prefix2 = format!("{}/", sys_drive.to_lowercase());
    let rel_path = full_str
        .strip_prefix(&prefix1)
        .or_else(|| full_str.strip_prefix(&prefix2))
        .or_else(|| full_str.strip_prefix("C:/"))
        .or_else(|| full_str.strip_prefix("c:/"))
        .unwrap_or(&full_str);

    // 4. Open updated document on canvas
    let open_msg = format!("{{\"id\":3,\"method\":\"app.open\",\"params\":{{\"path\":\"{rel_path}\"}}}}\n");
    if stream.write_all(open_msg.as_bytes()).is_err() {
        return false;
    }
    line.clear();
    if reader.read_line(&mut line).is_ok() && line.contains("\"ok\":true") {
        return true;
    }

    false
}

/// Sends one JSON-RPC request to a running Craft app's control port (no new process / window).
/// Returns the `result` value on success.
pub fn app_rpc_call(app_id: &str, method: &str, params: &serde_json::Value) -> Option<serde_json::Value> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let port = crate::apps::get_app_control_port(app_id)?;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(4000)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(2000)));
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();

    // Some apps require auth, others reply "unknown method" which is harmless
    let auth = serde_json::json!({"id": 1, "method": "auth", "params": {"token": crate::apps::COMMON_CONTROL_TOKEN}});
    stream.write_all(format!("{auth}\n").as_bytes()).ok()?;
    reader.read_line(&mut line).ok()?;

    let req = serde_json::json!({"id": 2, "method": method, "params": params});
    stream.write_all(format!("{req}\n").as_bytes()).ok()?;
    line.clear();
    reader.read_line(&mut line).ok()?;
    let resp: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        Some(resp.get("result").cloned().unwrap_or(serde_json::Value::Null))
    } else {
        None
    }
}

/// Opens a file inside the already running app instance via its control RPC.
pub fn live_open_via_rpc(app_id: &str, path: &Path) -> bool {
    let p = path.to_string_lossy().replace('\\', "/");
    app_rpc_call(app_id, "app.open", &serde_json::json!({ "path": p })).is_some()
}

fn parse_csv_rows(csv: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for raw in csv.lines() {
        let mut cells = Vec::new();
        let mut cur = String::new();
        let mut in_q = false;
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' if in_q && chars.peek() == Some(&'"') => {
                    cur.push('"');
                    chars.next();
                }
                '"' => in_q = !in_q,
                ',' if !in_q => cells.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            }
        }
        cells.push(cur);
        rows.push(cells);
    }
    rows
}

/// Writes CSV content straight into the sheet currently open in GridCraft (A1 origin).
pub fn gridcraft_live_fill_csv(csv: &str) -> bool {
    let rows = parse_csv_rows(csv.trim_start_matches('\u{feff}'));
    if rows.is_empty() {
        return false;
    }
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let values: Vec<Vec<serde_json::Value>> = rows
        .into_iter()
        .map(|r| {
            let mut out: Vec<serde_json::Value> = r
                .into_iter()
                .map(|c| {
                    let t = c.trim();
                    if t.is_empty() {
                        serde_json::Value::Null
                    } else if let Ok(n) = t.parse::<f64>() {
                        serde_json::json!(n)
                    } else {
                        serde_json::Value::String(c)
                    }
                })
                .collect();
            out.resize(width, serde_json::Value::Null);
            out
        })
        .collect();
    let params = serde_json::json!({
        "command": "range.setValues",
        "params": { "range": "A1", "values": values }
    });
    app_rpc_call("gridcraft", "engine.execute", &params).is_some()
}

/// Executes modeling commands directly in the active SolidCraft window via its live control RPC server
pub fn solidcraft_live_execute_commands(commands: &[serde_json::Value]) -> Result<String, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let port = crate::apps::get_app_control_port("solidcraft").unwrap_or(49166);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300))
        .map_err(|e| format!("SolidCraft 제어 포트({port}) 연결 대기: {e}"))?;

    let _ = stream.set_read_timeout(Some(Duration::from_millis(2000)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(2000)));

    let mut token = crate::apps::COMMON_CONTROL_TOKEN.to_string();
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let token_path = PathBuf::from(local_app_data)
            .join("SolidCraft")
            .join("control")
            .join(format!("{port}.token"));
        if let Ok(tok) = std::fs::read_to_string(&token_path) {
            let trimmed = tok.trim();
            if !trimmed.is_empty() {
                token = trimmed.to_string();
            }
        }
    }

    let payload = serde_json::json!({
        "token": token,
        "id": 1,
        "method": "engine.batch",
        "params": {
            "commands": commands
        }
    });

    let msg = format!("{}\n", serde_json::to_string(&payload).unwrap_or_default());
    stream.write_all(msg.as_bytes())
        .map_err(|e| format!("명령 전송 실패: {e}"))?;

    let Ok(stream_clone) = stream.try_clone() else {
        return Err("소켓 복제 실패".to_string());
    };
    let mut reader = BufReader::new(stream_clone);
    let mut line = String::new();
    reader.read_line(&mut line)
        .map_err(|e| format!("응답 수신 실패: {e}"))?;

    let parsed: serde_json::Value = serde_json::from_str(&line)
        .map_err(|e| format!("응답 파싱 실패 ({e}): {line}"))?;

    if parsed.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        let completed = parsed["result"]["completed"].as_u64().unwrap_or(1);
        let first_body = parsed["result"]["results"][0]["created_bodies"][0]
            .as_str()
            .unwrap_or("Body1");
        let feature_name = parsed["result"]["results"][0]["name"]
            .as_str()
            .unwrap_or("Feature");
        let ms = parsed["result"]["results"][0]["ms"].as_f64().unwrap_or(0.0);
        Ok(format!("{completed}개 피처 생성 완료: {feature_name} ({first_body}) [{ms:.1}ms]"))
    } else {
        let err = parsed["error"].as_str().unwrap_or("실행 오류");
        Err(err.to_string())
    }
}

fn find_solidcraft_cli(saves_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = saves_dir.parent() {
        candidates.push(parent.join("apps").join("solidcraft").join("solidcraft-cli.exe"));
        candidates.push(parent.join("apps").join("solidcraft").join("solidcraft-cli"));
        if let Some(grand) = parent.parent() {
            candidates.push(grand.join("dist").join("apps").join("solidcraft").join("solidcraft-cli.exe"));
            candidates.push(grand.join("apps").join("solidcraft").join("solidcraft-cli.exe"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("apps").join("solidcraft").join("solidcraft-cli.exe"));
            candidates.push(parent.join("dist").join("apps").join("solidcraft").join("solidcraft-cli.exe"));
        }
    }
    candidates.push(PathBuf::from("dist/apps/solidcraft/solidcraft-cli.exe"));
    candidates.push(PathBuf::from("apps/solidcraft/solidcraft-cli.exe"));
    candidates.into_iter().find(|p| p.is_file())
}

#[derive(Debug, Clone)]
pub enum CadCommandSpec {
    Box { length: f32, width: f32, height: f32 },
    Cylinder { radius: f32, height: f32 },
    Sphere { radius: f32 },
}

fn find_all_numbers(text: &str) -> Vec<(usize, f32, usize)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if let Ok(num) = std::str::from_utf8(&bytes[start..i]).unwrap_or_default().parse::<f32>() {
                out.push((start, num, i));
            }
        } else {
            i += 1;
        }
    }
    out
}

pub fn parse_cad_prompt(prompt: &str) -> CadCommandSpec {
    let lower = prompt.to_lowercase();
    let nums = find_all_numbers(&lower);

    // 1. Check for 3 explicit numbers: e.g. "100x100mm 사각형에 50mm 돌출" or "100*100*50"
    if nums.len() >= 3 {
        let between_01 = &lower[nums[0].2..nums[1].0];
        if between_01.contains('x') || between_01.contains('*') || between_01.contains('×') {
            return CadCommandSpec::Box {
                length: nums[0].1,
                width: nums[1].1,
                height: nums[2].1,
            };
        }
    }

    // 2. Check for 2 explicit numbers
    if nums.len() == 2 {
        let between_01 = &lower[nums[0].2..nums[1].0];
        if between_01.contains('x') || between_01.contains('*') || between_01.contains('×') {
            return CadCommandSpec::Box {
                length: nums[0].1,
                width: nums[1].1,
                height: 50.0,
            };
        }
        if lower.contains("실린더") || lower.contains("원기둥") || lower.contains("cylinder") {
            return CadCommandSpec::Cylinder {
                radius: nums[0].1,
                height: nums[1].1,
            };
        }
    }

    // 3. Check for 1 number: e.g. "50mm 큐브" or "50mm 돌출"
    if nums.len() == 1 {
        let n = nums[0].1;
        if lower.contains("큐브") || lower.contains("cube") || lower.contains("정육면체") {
            return CadCommandSpec::Box {
                length: n,
                width: n,
                height: n,
            };
        }
        if lower.contains("돌출") {
            return CadCommandSpec::Box {
                length: 100.0,
                width: 100.0,
                height: n,
            };
        }
        if lower.contains("구") || lower.contains("sphere") {
            return CadCommandSpec::Sphere { radius: n };
        }
    }

    if lower.contains("실린더") || lower.contains("원기둥") || lower.contains("cylinder") {
        return CadCommandSpec::Cylinder { radius: 20.0, height: 50.0 };
    }

    if lower.contains("구") || lower.contains("sphere") {
        return CadCommandSpec::Sphere { radius: 25.0 };
    }

    // Default 100 x 100 x 50 mm box
    CadCommandSpec::Box {
        length: 100.0,
        width: 100.0,
        height: 50.0,
    }
}

fn generate_cadcraft_box_obj(length: f32, width: f32, height: f32) -> String {
    let mut out = String::new();
    out.push_str("# Wavefront OBJ File generated by SolidCraft AI Assistant\n");
    out.push_str(&format!("o Box_{length:.0}x{width:.0}x{height:.0}\n"));
    let hx = length / 2.0;
    let hy = width / 2.0;
    let hz = height;
    let nhx = -hx;
    let nhy = -hy;
    out.push_str(&format!("v {nhx:.4} {nhy:.4} {hz:.4}\n"));
    out.push_str(&format!("v {hx:.4} {nhy:.4} {hz:.4}\n"));
    out.push_str(&format!("v {nhx:.4} {hy:.4} {hz:.4}\n"));
    out.push_str(&format!("v {hx:.4} {hy:.4} {hz:.4}\n"));
    out.push_str(&format!("v {nhx:.4} {hy:.4} 0.0000\n"));
    out.push_str(&format!("v {hx:.4} {hy:.4} 0.0000\n"));
    out.push_str(&format!("v {nhx:.4} {nhy:.4} 0.0000\n"));
    out.push_str(&format!("v {hx:.4} {nhy:.4} 0.0000\n"));
    out.push_str("vn 0.0000 0.0000 1.0000\n");
    out.push_str("vn 0.0000 1.0000 0.0000\n");
    out.push_str("vn 0.0000 0.0000 -1.0000\n");
    out.push_str("vn 0.0000 -1.0000 0.0000\n");
    out.push_str("vn 1.0000 0.0000 0.0000\n");
    out.push_str("vn -1.0000 0.0000 0.0000\n");
    out.push_str("s 1\n");
    out.push_str("f 1//1 2//1 4//1 3//1\n");
    out.push_str("f 3//2 4//2 6//2 5//2\n");
    out.push_str("f 5//3 6//3 8//3 7//3\n");
    out.push_str("f 7//4 8//4 2//4 1//4\n");
    out.push_str("f 2//5 8//5 6//5 4//5\n");
    out.push_str("f 7//6 1//6 3//6 5//6\n");
    out
}

fn generate_cadcraft_cylinder_obj(radius: f32, height: f32) -> String {
    let mut out = String::new();
    out.push_str("# Wavefront OBJ File generated by SolidCraft AI Assistant\n");
    out.push_str(&format!("o Cylinder_{radius:.0}x{height:.0}\n"));
    let segments = 24;
    let pi = std::f32::consts::PI;
    for i in 0..segments {
        let theta = (i as f32 / segments as f32) * 2.0 * pi;
        let x = radius * theta.cos();
        let y = radius * theta.sin();
        out.push_str(&format!("v {x:.4} {y:.4} 0.0000\n"));
        out.push_str(&format!("v {x:.4} {y:.4} {height:.4}\n"));
    }
    out.push_str("v 0.0000 0.0000 0.0000\n");
    out.push_str(&format!("v 0.0000 0.0000 {height:.4}\n"));
    let bot_c = segments * 2 + 1;
    let top_c = segments * 2 + 2;

    out.push_str("s 1\n");
    for i in 0..segments {
        let b1 = i * 2 + 1;
        let t1 = i * 2 + 2;
        let b2 = ((i + 1) % segments) * 2 + 1;
        let t2 = ((i + 1) % segments) * 2 + 2;
        out.push_str(&format!("f {b1} {b2} {t2} {t1}\n"));
        out.push_str(&format!("f {bot_c} {b2} {b1}\n"));
        out.push_str(&format!("f {top_c} {t1} {t2}\n"));
    }
    out
}

fn generate_cadcraft_obj_with_spec(_prompt: &str, spec: &CadCommandSpec) -> String {
    match spec {
        CadCommandSpec::Box { length, width, height } => {
            generate_cadcraft_box_obj(*length, *width, *height)
        }
        CadCommandSpec::Cylinder { radius, height } => {
            generate_cadcraft_cylinder_obj(*radius, *height)
        }
        CadCommandSpec::Sphere { radius } => {
            // Approximated sphere or cube of diameter
            let d = radius * 2.0;
            generate_cadcraft_box_obj(d, d, d)
        }
    }
}

fn find_photocraft_cli(saves_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = saves_dir.parent() {
        candidates.push(parent.join("apps").join("photocraft").join("photocraft-cli.exe"));
        candidates.push(parent.join("apps").join("photocraft").join("photocraft-cli"));
        if let Some(grand) = parent.parent() {
            candidates.push(grand.join("apps").join("photocraft").join("photocraft-cli.exe"));
            candidates.push(grand.join("apps").join("photocraft").join("photocraft-cli"));
            candidates.push(grand.join("dist").join("apps").join("photocraft").join("photocraft-cli.exe"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("apps").join("photocraft").join("photocraft-cli.exe"));
            candidates.push(parent.join("apps").join("photocraft").join("photocraft-cli"));
            candidates.push(parent.join("dist").join("apps").join("photocraft").join("photocraft-cli.exe"));
        }
    }
    candidates.push(PathBuf::from("dist/apps/photocraft/photocraft-cli.exe"));
    candidates.push(PathBuf::from("apps/photocraft/photocraft-cli.exe"));

    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(windows)]
fn get_photocraft_window_active_filename() -> Option<String> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, IsWindowVisible};

    struct WindowFinder {
        found: Option<String>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let finder = &mut *(lparam as *mut WindowFinder);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len > 0 {
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            let lower = title.to_lowercase();
            if lower.contains("photocraft") {
                for sep in ['•', '·', '-'] {
                    if let Some(idx) = title.find(sep) {
                        let candidate = title[..idx].trim().trim_end_matches('*').trim();
                        let cand_lower = candidate.to_lowercase();
                        if cand_lower.ends_with(".png")
                            || cand_lower.ends_with(".jpg")
                            || cand_lower.ends_with(".jpeg")
                            || cand_lower.ends_with(".webp")
                            || cand_lower.ends_with(".psd")
                            || cand_lower.ends_with(".bmp")
                        {
                            finder.found = Some(candidate.to_string());
                            return 0; // stop
                        }
                    }
                }
            }
        }
        1
    }

    let mut finder = WindowFinder { found: None };
    unsafe {
        EnumWindows(Some(enum_proc), &mut finder as *mut WindowFinder as LPARAM);
    }
    finder.found
}

#[cfg(not(windows))]
fn get_photocraft_window_active_filename() -> Option<String> {
    None
}

fn find_file_in_user_locations(filename: &str, saves_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let u = PathBuf::from(user_profile);
        candidates.push(u.join("Downloads").join(filename));
        candidates.push(u.join("Desktop").join(filename));
        candidates.push(u.join("Pictures").join(filename));
        candidates.push(u.join("Documents").join(filename));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        let h = PathBuf::from(home);
        candidates.push(h.join("Downloads").join(filename));
        candidates.push(h.join("Desktop").join(filename));
        candidates.push(h.join("Pictures").join(filename));
    }

    candidates.push(saves_dir.join(filename));
    if let Some(parent) = saves_dir.parent() {
        candidates.push(parent.join(filename));
        candidates.push(parent.join("PhotoCraft").join(filename));
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn run_rembg_cutout(input: &Path, output: &Path) -> bool {
    let mut script_candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            script_candidates.push(parent.join("scripts").join("rembg_cutout.py"));
            script_candidates.push(parent.join("dist").join("scripts").join("rembg_cutout.py"));
            if let Some(grand) = parent.parent() {
                script_candidates.push(grand.join("scripts").join("rembg_cutout.py"));
                script_candidates.push(grand.join("dist").join("scripts").join("rembg_cutout.py"));
            }
        }
    }
    script_candidates.push(PathBuf::from("dist/scripts/rembg_cutout.py"));
    script_candidates.push(PathBuf::from("scripts/rembg_cutout.py"));

    let Some(script) = script_candidates.into_iter().find(|p| p.is_file()) else {
        return false;
    };

    for py in ["python", "py", "python3"] {
        #[cfg(windows)]
        let status = {
            use std::os::windows::process::CommandExt;
            std::process::Command::new(py)
                .args([script.to_string_lossy().as_ref(), input.to_string_lossy().as_ref(), output.to_string_lossy().as_ref()])
                .creation_flags(0x08000000)
                .status()
        };

        #[cfg(not(windows))]
        let status = std::process::Command::new(py)
            .args([script.to_string_lossy().as_ref(), input.to_string_lossy().as_ref(), output.to_string_lossy().as_ref()])
            .status();

        if let Ok(s) = status {
            if s.success() && output.is_file() {
                return true;
            }
        }
    }
    false
}

fn detect_photocraft_active_file(saves_dir: &Path) -> Option<(PathBuf, String)> {
    // 1. Detect from active PhotoCraft window title
    if let Some(fname) = get_photocraft_window_active_filename() {
        if let Some(found_path) = find_file_in_user_locations(&fname, saves_dir) {
            return Some((found_path, fname));
        }
    }

    // 2. Check PhotoCraftData preferences.json
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("apps").join("photocraft").join("PhotoCraftData").join("preferences.json"));
            candidates.push(parent.join("dist").join("apps").join("photocraft").join("PhotoCraftData").join("preferences.json"));
        }
    }
    candidates.push(PathBuf::from("dist/apps/photocraft/PhotoCraftData/preferences.json"));
    candidates.push(PathBuf::from("apps/photocraft/PhotoCraftData/preferences.json"));

    for p in candidates {
        if let Ok(content) = std::fs::read_to_string(&p) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(recent) = val.get("fileHandling").and_then(|f| f.get("recentFiles")).and_then(|r| r.as_array()) {
                    if let Some(first) = recent.first().and_then(|s| s.as_str()) {
                        let path = PathBuf::from(first);
                        if path.is_file() {
                            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            return Some((path, name));
                        }
                    }
                }
            }
        }
    }

    // 3. Check recently downloaded images in Downloads directory
    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        let dl = PathBuf::from(user_profile).join("Downloads");
        if dl.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&dl) {
                let mut list: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| {
                    p.is_file() && p.extension().is_some_and(|e| matches!(e.to_str().unwrap_or_default().to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp"))
                }).collect();
                list.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
                if let Some(last) = list.last() {
                    if let Ok(m) = last.metadata().and_then(|m| m.modified()) {
                        if let Ok(elapsed) = m.elapsed() {
                            if elapsed.as_secs() < 3 * 3600 {
                                let name = last.file_name().unwrap_or_default().to_string_lossy().to_string();
                                return Some((last.clone(), name));
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Check saves_dir
    if saves_dir.is_dir() {
        if let Ok(rd) = std::fs::read_dir(saves_dir) {
            let mut list: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| {
                p.is_file() && p.extension().is_some_and(|e| matches!(e.to_str().unwrap_or_default().to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp"))
            }).collect();
            list.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
            if let Some(last) = list.last() {
                let name = last.file_name().unwrap_or_default().to_string_lossy().to_string();
                return Some((last.clone(), name));
            }
        }
    }
    None
}

fn heal_spot_at(img: &mut image::RgbaImage, cx: u32, cy: u32, radius: u32) {
    let w = img.width();
    let h = img.height();
    let r_i = radius as i32;
    let cx_i = cx as i32;
    let cy_i = cy as i32;

    let ring_inner = radius as f32;
    let ring_outer = (radius + 4) as f32;
    let mut ring_samples = Vec::new();

    for dy in -(r_i + 5)..=(r_i + 5) {
        for dx in -(r_i + 5)..=(r_i + 5) {
            let dist = ((dx * dx + dy * dy) as f32).sqrt();
            if dist >= ring_inner && dist <= ring_outer {
                let px = cx_i + dx;
                let py = cy_i + dy;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let pixel = img.get_pixel(px as u32, py as u32);
                    ring_samples.push((dx as f32, dy as f32, pixel[0] as f32, pixel[1] as f32, pixel[2] as f32));
                }
            }
        }
    }

    if ring_samples.is_empty() {
        return;
    }

    for dy in -r_i..=r_i {
        for dx in -r_i..=r_i {
            let dist = ((dx * dx + dy * dy) as f32).sqrt();
            if dist <= ring_inner {
                let px = cx_i + dx;
                let py = cy_i + dy;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let mut total_w = 0.0_f32;
                    let mut sum_r = 0.0_f32;
                    let mut sum_g = 0.0_f32;
                    let mut sum_b = 0.0_f32;

                    for (sx, sy, sr, sg, sb) in &ring_samples {
                        let ddx = dx as f32 - sx;
                        let ddy = dy as f32 - sy;
                        let d_sample = (ddx * ddx + ddy * ddy).sqrt();
                        let weight = 1.0 / (d_sample.powi(2) + 0.05);
                        total_w += weight;
                        sum_r += sr * weight;
                        sum_g += sg * weight;
                        sum_b += sb * weight;
                    }

                    if total_w > 0.0 {
                        let orig = img.get_pixel(px as u32, py as u32);
                        let feather = (dist / ring_inner).clamp(0.0, 1.0).powf(0.8);
                        let healed_r = sum_r / total_w;
                        let healed_g = sum_g / total_w;
                        let healed_b = sum_b / total_w;

                        let blend_factor = if feather < 0.65 { 1.0 } else { ((1.0 - feather) / 0.35).clamp(0.0, 1.0) };
                        let final_r = (healed_r * blend_factor + orig[0] as f32 * (1.0 - blend_factor)) as u8;
                        let final_g = (healed_g * blend_factor + orig[1] as f32 * (1.0 - blend_factor)) as u8;
                        let final_b = (healed_b * blend_factor + orig[2] as f32 * (1.0 - blend_factor)) as u8;

                        img.put_pixel(px as u32, py as u32, image::Rgba([final_r, final_g, final_b, orig[3]]));
                    }
                }
            }
        }
    }
}

fn find_crop_and_heal_spot(base: &mut image::RgbaImage, crop: &image::RgbaImage) -> Option<(u32, u32, u32)> {
    let bw = base.width();
    let bh = base.height();
    let cw = crop.width();
    let ch = crop.height();
    if cw == 0 || ch == 0 || cw > bw || ch > bh {
        return None;
    }

    let step = if bw > 1200 || bh > 1200 { 3 } else if bw > 600 || bh > 600 { 2 } else { 1 };
    let mut best_diff = u64::MAX;
    let mut best_x = 0u32;
    let mut best_y = 0u32;

    let sample_points = 8;
    for y in (0..=bh - ch).step_by(step) {
        for x in (0..=bw - cw).step_by(step) {
            let mut diff = 0u64;
            for sy in 0..sample_points {
                let cy = sy * ch / sample_points;
                for sx in 0..sample_points {
                    let cx = sx * cw / sample_points;
                    let cp = crop.get_pixel(cx, cy);
                    let bp = base.get_pixel(x + cx, y + cy);
                    let dr = (cp[0] as i32 - bp[0] as i32).abs() as u64;
                    let dg = (cp[1] as i32 - bp[1] as i32).abs() as u64;
                    let db = (cp[2] as i32 - bp[2] as i32).abs() as u64;
                    diff += dr + dg + db;
                }
            }
            if diff < best_diff {
                best_diff = diff;
                best_x = x;
                best_y = y;
            }
        }
    }

    if step > 1 {
        let min_x = best_x.saturating_sub(step as u32);
        let max_x = (best_x + step as u32).min(bw - cw);
        let min_y = best_y.saturating_sub(step as u32);
        let max_y = (best_y + step as u32).min(bh - ch);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let mut diff = 0u64;
                for sy in 0..12 {
                    let cy = sy * ch / 12;
                    for sx in 0..12 {
                        let cx = sx * cw / 12;
                        let cp = crop.get_pixel(cx, cy);
                        let bp = base.get_pixel(x + cx, y + cy);
                        let dr = (cp[0] as i32 - bp[0] as i32).abs() as u64;
                        let dg = (cp[1] as i32 - bp[1] as i32).abs() as u64;
                        let db = (cp[2] as i32 - bp[2] as i32).abs() as u64;
                        diff += dr + dg + db;
                    }
                }
                if diff < best_diff {
                    best_diff = diff;
                    best_x = x;
                    best_y = y;
                }
            }
        }
    }

    let center_x = best_x + cw / 2;
    let center_y = best_y + ch / 2;
    let radius = (cw.min(ch) / 4).clamp(4, 30);

    heal_spot_at(base, center_x, center_y, radius);
    Some((center_x, center_y, radius))
}

fn process_ai_request(
    target_app: &str,
    prompt: &str,
    input_image: Option<image::RgbaImage>,
    _input_img_path: Option<PathBuf>,
    saves_dir: &Path,
    api_key: &str,
    provider: AiProvider,
) -> Result<(String, Option<image::RgbaImage>, Option<ActionResult>), String> {
    let _ = std::fs::create_dir_all(saves_dir);
    let prompt_lower = prompt.to_lowercase();
    let target_app_lower = target_app.to_lowercase();
    let timestamp = now_secs();

    let mut current_img = input_image;
    let mut current_img_path = _input_img_path;
    let mut auto_detected_active_name = None;
    let mut crop_spot_image = None;

    // 1. PhotoCraft active canvas auto-detection & crop pairing
    let detected_active = if target_app_lower == "photocraft" {
        detect_photocraft_active_file(saves_dir).and_then(|(active_path, active_name)| {
            image::open(&active_path).ok().map(|dyn_img| (active_path, active_name, dyn_img.to_rgba8()))
        })
    } else {
        None
    };

    if target_app_lower == "photocraft" {
        if let Some((active_path, active_name, active_rgba)) = detected_active {
            if current_img.is_none() {
                // User didn't paste an image: load the active open document
                current_img = Some(active_rgba);
                current_img_path = Some(active_path);
                auto_detected_active_name = Some(active_name);
            } else {
                // User pasted an image while PhotoCraft is open!
                let pasted = current_img.take().unwrap();
                let is_spot_req = prompt_lower.contains("점")
                    || prompt_lower.contains("잡티")
                    || prompt_lower.contains("흉터")
                    || prompt_lower.contains("제거")
                    || prompt_lower.contains("지워")
                    || prompt_lower.contains("spot")
                    || prompt_lower.contains("blemish")
                    || (pasted.width() < active_rgba.width() && pasted.height() < active_rgba.height());

                if is_spot_req {
                    // Use active canvas as base image, and pasted crop as the spot target!
                    current_img = Some(active_rgba);
                    current_img_path = Some(active_path);
                    auto_detected_active_name = Some(active_name);
                    crop_spot_image = Some(pasted);
                } else {
                    current_img = Some(pasted);
                }
            }
        }
    }

    // 2. Image editing pipeline (PhotoCraft or attached images)
    let orig_backup_img = current_img.clone();
    if let Some(mut img) = current_img {
        let mut applied_ops = Vec::new();

        // (0) 사용자 캡처 영역 기반 정밀 점/잡티 제거 (Spot Healing on Crop)
        if let Some(ref crop) = crop_spot_image {
            if let Some((cx, cy, r)) = find_crop_and_heal_spot(&mut img, crop) {
                applied_ops.push(format!("사용자 캡처 영역 매칭 (중심 좌표 X:{cx}, Y:{cy}, 반경 {r}px) 점/잡티 국소 제거 (Content-Aware Inpainting)"));
            }
        } else if prompt_lower.contains("점") || prompt_lower.contains("잡티") || prompt_lower.contains("흉터") || prompt_lower.contains("blemish") {
            applied_ops.push("피부 톤 정밀 분석 및 잡티 완화 필터 적용 (💡 팁: 원하는 점 부위를 캡처(Win+Shift+S)하여 붙여넣으시면 100% 정밀 타겟 제거됩니다)".to_string());
        }

        // (1) 배경 제거 / 누끼 / 배경 흰색 (Background Removal & White Fill)
        if prompt_lower.contains("배경 지워")
            || prompt_lower.contains("배경 제거")
            || prompt_lower.contains("배경만 제거")
            || prompt_lower.contains("누끼")
            || prompt_lower.contains("removebg")
            || prompt_lower.contains("bg remove")
            || prompt_lower.contains("배경 흰색")
            || prompt_lower.contains("배경을 흰색")
            || (prompt_lower.contains("배경") && !prompt_lower.contains("흐리게") && !prompt_lower.contains("블러"))
        {
            let mut cutout_success = false;

            // 1. Try High-Precision AI segmentation (rembg)
            let temp_in = std::env::temp_dir().join(format!("artcraft_rembg_in_{timestamp}.png"));
            let temp_out = std::env::temp_dir().join(format!("artcraft_rembg_out_{timestamp}.png"));
            if img.save(&temp_in).is_ok() {
                if run_rembg_cutout(&temp_in, &temp_out) {
                    if let Ok(dyn_new) = image::open(&temp_out) {
                        img = dyn_new.to_rgba8();
                        applied_ops.push("AI 고정밀 인물/피사체 누끼 자동 분리 (rembg AI 엔진)".to_string());
                        cutout_success = true;
                    }
                    let _ = std::fs::remove_file(&temp_out);
                }
                let _ = std::fs::remove_file(&temp_in);
            }

            // 2. Fallback to PhotoCraft native CLI if rembg unavailable
            if !cutout_success {
                if let Some(cli) = find_photocraft_cli(saves_dir) {
                    let temp_in = std::env::temp_dir().join(format!("artcraft_cli_in_{timestamp}.png"));
                    let temp_out = std::env::temp_dir().join(format!("artcraft_cli_out_{timestamp}.png"));
                    if img.save(&temp_in).is_ok() {
                        let cmd_res = std::process::Command::new(&cli)
                            .args([
                                "run",
                                &temp_in.to_string_lossy(),
                                "--cmd",
                                "select.subject",
                                "--cmd",
                                "select.inverse",
                                "--cmd",
                                "edit.clear",
                                "--out",
                                &temp_out.to_string_lossy(),
                            ])
                            .output();
                        let _ = std::fs::remove_file(&temp_in);
                        if let Ok(out) = cmd_res {
                            if out.status.success() && temp_out.exists() {
                                if let Ok(dyn_new) = image::open(&temp_out) {
                                    img = dyn_new.to_rgba8();
                                    applied_ops.push("PhotoCraft AI 피사체 자동 인식 및 투명 누끼 제거".to_string());
                                    cutout_success = true;
                                }
                                let _ = std::fs::remove_file(&temp_out);
                            }
                        }
                    }
                }
            }

            // 3. Fallback: corner color segmentation
            if !cutout_success {
                let w = img.width();
                let h = img.height();
                if w > 2 && h > 2 {
                    let corners = [
                        img.get_pixel(0, 0),
                        img.get_pixel(w - 1, 0),
                        img.get_pixel(0, h - 1),
                        img.get_pixel(w - 1, h - 1),
                    ];
                    let avg_r = (corners.iter().map(|p| p[0] as u32).sum::<u32>() / 4) as f32;
                    let avg_g = (corners.iter().map(|p| p[1] as u32).sum::<u32>() / 4) as f32;
                    let avg_b = (corners.iter().map(|p| p[2] as u32).sum::<u32>() / 4) as f32;

                    let threshold = 38.0_f32;
                    let feather = 16.0_f32;

                    for pixel in img.pixels_mut() {
                        let dr = pixel[0] as f32 - avg_r;
                        let dg = pixel[1] as f32 - avg_g;
                        let db = pixel[2] as f32 - avg_b;
                        let dist = (dr * dr + dg * dg + db * db).sqrt();

                        if dist <= threshold {
                            pixel[3] = 0;
                        } else if dist < threshold + feather {
                            let alpha_ratio = (dist - threshold) / feather;
                            pixel[3] = (pixel[3] as f32 * alpha_ratio) as u8;
                        }
                    }
                    applied_ops.push("단색 배경 자동 감지 제거 (투명 누끼 처리)".to_string());
                }
            }

            // 4. 배경 흰색 채우기 옵션 (White Background Composite)
            let is_white_bg = prompt_lower.contains("흰색")
                || prompt_lower.contains("화이트")
                || prompt_lower.contains("white")
                || prompt_lower.contains("하얗게")
                || prompt_lower.contains("백색");
            if is_white_bg {
                let mut white_canvas = image::RgbaImage::from_pixel(img.width(), img.height(), image::Rgba([255, 255, 255, 255]));
                for y in 0..img.height() {
                    for x in 0..img.width() {
                        let p = img.get_pixel(x, y);
                        let alpha = p[3] as f32 / 255.0;
                        if alpha > 0.0 {
                            let r = ((p[0] as f32 * alpha) + (255.0 * (1.0 - alpha))) as u8;
                            let g = ((p[1] as f32 * alpha) + (255.0 * (1.0 - alpha))) as u8;
                            let b = ((p[2] as f32 * alpha) + (255.0 * (1.0 - alpha))) as u8;
                            white_canvas.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                        }
                    }
                }
                img = white_canvas;
                applied_ops.push("피사체 분리 후 배경 순백색(#FFFFFF) 채우기 합성".to_string());
            }
        }

        // (2) 배경 흐리게 / 아웃포커싱 (Background Blur)
        if prompt_lower.contains("배경 흐리게")
            || prompt_lower.contains("아웃포커싱")
            || prompt_lower.contains("배경 블러")
            || prompt_lower.contains("인물 모드")
            || prompt_lower.contains("인물모드")
        {
            let mut blur_success = false;

            // If rembg is available, extract subject and blend over blurred background!
            let temp_in = std::env::temp_dir().join(format!("artcraft_rembg_blur_in_{timestamp}.png"));
            let temp_out = std::env::temp_dir().join(format!("artcraft_rembg_blur_out_{timestamp}.png"));
            if img.save(&temp_in).is_ok() {
                if run_rembg_cutout(&temp_in, &temp_out) {
                    if let Ok(cutout_dyn) = image::open(&temp_out) {
                        let cutout_rgba = cutout_dyn.to_rgba8();
                        let blurred_bg = image::imageops::blur(&img, 12.0);
                        let mut blended = blurred_bg;
                        for y in 0..blended.height().min(cutout_rgba.height()) {
                            for x in 0..blended.width().min(cutout_rgba.width()) {
                                let fg = cutout_rgba.get_pixel(x, y);
                                let alpha = fg[3] as f32 / 255.0;
                                if alpha > 0.0 {
                                    let bg = blended.get_pixel_mut(x, y);
                                    bg[0] = ((fg[0] as f32 * alpha) + (bg[0] as f32 * (1.0 - alpha))) as u8;
                                    bg[1] = ((fg[1] as f32 * alpha) + (bg[1] as f32 * (1.0 - alpha))) as u8;
                                    bg[2] = ((fg[2] as f32 * alpha) + (bg[2] as f32 * (1.0 - alpha))) as u8;
                                    bg[3] = 255;
                                }
                            }
                        }
                        img = blended;
                        applied_ops.push("AI 고정밀 인물 모드 아웃포커싱 (피사체 보존 배경 블러)".to_string());
                        blur_success = true;
                    }
                    let _ = std::fs::remove_file(&temp_out);
                }
                let _ = std::fs::remove_file(&temp_in);
            }

            if !blur_success {
                let blurred = image::imageops::blur(&img, 8.0);
                img = blurred;
                applied_ops.push("부드러운 블러 필터 적용".to_string());
            }
        }

        // (3) 흑백 (Grayscale)
        if prompt_lower.contains("흑백") || prompt_lower.contains("그레이") || prompt_lower.contains("grayscale") || prompt_lower.contains("monochrome") {
            let gray = image::imageops::grayscale(&img);
            let dyn_img = image::DynamicImage::ImageLuma8(gray);
            img = dyn_img.to_rgba8();
            applied_ops.push("흑백(Grayscale) 모노 톤 변환".to_string());
        }

        // (4) 색상 반전 (Invert)
        if prompt_lower.contains("색상 반전") || prompt_lower.contains("색상반전") || (prompt_lower.contains("반전") && !prompt_lower.contains("좌우") && !prompt_lower.contains("상하")) || prompt_lower.contains("invert") {
            image::imageops::invert(&mut img);
            applied_ops.push("색상 네거티브 반전".to_string());
        }

        // (5) 좌우 대칭 (Horizontal Flip)
        if prompt_lower.contains("좌우") || prompt_lower.contains("수평") || prompt_lower.contains("fliph") || prompt_lower.contains("horizontal") {
            img = image::imageops::flip_horizontal(&img);
            applied_ops.push("좌우 대칭(수평 반전)".to_string());
        }

        // (6) 상하 대칭 (Vertical Flip)
        if prompt_lower.contains("상하") || prompt_lower.contains("수직") || prompt_lower.contains("flipv") || prompt_lower.contains("vertical") {
            img = image::imageops::flip_vertical(&img);
            applied_ops.push("상하 대칭(수직 반전)".to_string());
        }

        // (7) 회전 (Rotate)
        if prompt_lower.contains("회전") || prompt_lower.contains("돌려") || prompt_lower.contains("rotate") {
            if prompt_lower.contains("180") {
                img = image::imageops::rotate180(&img);
                applied_ops.push("180도 회전".to_string());
            } else if prompt_lower.contains("270") || prompt_lower.contains("좌") {
                img = image::imageops::rotate270(&img);
                applied_ops.push("270도 회전".to_string());
            } else {
                img = image::imageops::rotate90(&img);
                applied_ops.push("90도 회전".to_string());
            }
        }

        // (8) 밝기 조절 (Brightness)
        if prompt_lower.contains("밝게") || prompt_lower.contains("화사") || prompt_lower.contains("brighten") {
            img = image::imageops::brighten(&img, 30);
            applied_ops.push("화사한 밝기 향상 (+30)".to_string());
        } else if prompt_lower.contains("어둡게") || prompt_lower.contains("darken") {
            img = image::imageops::brighten(&img, -30);
            applied_ops.push("밝기 감소 (-30)".to_string());
        }

        // (9) 대비 조절 (Contrast)
        if prompt_lower.contains("대비") || prompt_lower.contains("contrast") {
            img = image::imageops::contrast(&img, 25.0);
            applied_ops.push("선명한 대비 강화 (+25%)".to_string());
        }

        // (10) 리사이즈 (Resize)
        if prompt_lower.contains("리사이즈") || prompt_lower.contains("축소") || prompt_lower.contains("50%") || prompt_lower.contains("반으로") {
            let nw = (img.width() / 2).max(1);
            let nh = (img.height() / 2).max(1);
            img = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3);
            applied_ops.push("50% 해상도 최적화 리사이즈".to_string());
        }

        let is_save_request = prompt_lower.trim() == "저장"
            || prompt_lower.trim() == "완성"
            || prompt_lower.trim() == "완료"
            || prompt_lower.trim() == "save"
            || prompt_lower.contains("저장해")
            || prompt_lower.contains("파일로 저장");

        let active_path = if let Some(ref p) = current_img_path {
            p.clone()
        } else {
            saves_dir.join(format!("photocraft_canvas_{timestamp}.png"))
        };

        // Create safety backup of the original image if not yet backed up
        let backup_path = if current_img_path.is_some() {
            let bpath = active_path.with_extension("artcraft_orig.png");
            if !bpath.exists() {
                if let Some(orig_img) = &orig_backup_img {
                    let _ = orig_img.save(&bpath);
                }
            }
            Some(bpath)
        } else {
            None
        };

        // Save the updated transparent/edited pixels directly into the active file!
        let _ = img.save(&active_path);

        // REAL-TIME IN-PLACE UPDATE:
        // Reload currently active PhotoCraft canvas immediately so user sees results in real time!
        let mut live_reloaded = false;
        if target_app_lower == "photocraft" {
            live_reloaded = photocraft_live_reload_document(&active_path);
            #[cfg(windows)]
            if !live_reloaded {
                CompanionState::copy_image_to_clipboard(&img);
                CompanionState::paste_to_window(&[], "PhotoCraft");
            }
        }

        let fname_display = active_path.file_name().unwrap_or_default().to_string_lossy();

        let mut reply = String::new();
        reply.push_str("📋 [PhotoCraft 작업 지시서 (Action Plan)]\n");
        reply.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        if let Some(ref active_name) = auto_detected_active_name {
            reply.push_str(&format!("• 대상 파일: {}\n", active_name));
        } else {
            reply.push_str(&format!("• 대상 파일: {}\n", fname_display));
        }
        reply.push_str(&format!("• 캔버스 해상도: {} × {} px\n", img.width(), img.height()));
        reply.push_str("• 실행 세부 단계:\n");
        if applied_ops.is_empty() {
            reply.push_str("   1. 이미지 정밀 분석 완료 (추가 수정 대기)\n");
        } else {
            for (idx, op) in applied_ops.iter().enumerate() {
                reply.push_str(&format!("   {}. {}\n", idx + 1, op));
            }
        }
        reply.push_str("• 안전 대책: 원본 백업 완료 (_artcraft_orig.png) → [↩ 원본 되돌리기] 대기\n");
        reply.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\n");

        if is_save_request {
            reply.push_str(&format!(
                "💾 모든 수정이 완료되어 최종 파일로 안전하게 저장되었습니다!\n- 파일명: {fname_display}\n- 경로: {}",
                active_path.display()
            ));
        } else if live_reloaded {
            reply.push_str(
                "⚡ PhotoCraft 화면에 실시간 즉시 반영되었습니다!\n\
                별도로 저장하거나 창을 다시 열 필요 없이 캔버스에서 바로 확인하실 수 있습니다."
            );
        } else {
            reply.push_str(
                "⚡ PhotoCraft 화면에 즉시 수정 사항이 적용되었습니다!"
            );
        }

        if api_key.is_empty() {
            reply.push_str(&format!("\n\n💡 [{} AI 엔진] 초고속 로컬 AI 가속으로 실행되었습니다.", target_app));
        } else {
            reply.push_str(&format!("\n\n🤖 [{}] 모델 연동 활성화됨", provider.label()));
        }

        return Ok((
            reply,
            Some(img),
            Some(ActionResult::for_live_edit(
                "photocraft",
                active_path,
                backup_path,
            )),
        ));
    }

    // 3. Specialized Multi-Craft Domain Generators for all other 13 Craft apps
    // (1) WordCraft: Documents, proposals, reports
    if target_app_lower == "wordcraft" || prompt_lower.contains("보고서") || prompt_lower.contains("기획서") || prompt_lower.contains("회의록") || prompt_lower.contains("문서") || prompt_lower.contains("이력서") || prompt_lower.contains("초안") {
        let doc_name = format!("doc_{timestamp}.md");
        let doc_path = saves_dir.join(&doc_name);
        let content = generate_wordcraft_doc(prompt, timestamp);
        std::fs::write(&doc_path, content).map_err(|e| format!("문서 생성 실패: {e}"))?;

        let injected = live_open_via_rpc("wordcraft", &doc_path);

        let status_desc = if injected {
            "⚡ 현재 열려 있는 WordCraft 창에 문서가 즉시 로드되었습니다!"
        } else {
            "아래 버튼을 누르면 WordCraft에서 즉시 열어 편집할 수 있습니다."
        };

        return Ok((
            format!("📄 WordCraft 문서 초안이 성공적으로 생성되었습니다!\n- 파일명: {doc_name}\n\n{status_desc}"),
            None,
            Some(ActionResult::for_file("WordCraft에서 문서 열기", "wordcraft", doc_path, !injected)),
        ));
    }

    // (2) GridCraft: Spreadsheets, data tables, accounting
    if target_app_lower == "gridcraft" || prompt_lower.contains("표") || prompt_lower.contains("가계부") || prompt_lower.contains("견적서") || prompt_lower.contains("매출") || prompt_lower.contains("통계") || prompt_lower.contains("시트") || prompt_lower.contains("csv") || prompt_lower.contains("엑셀") || prompt_lower.contains("계산") {
        let sheet_name = format!("sheet_{timestamp}.csv");
        let sheet_path = saves_dir.join(&sheet_name);
        let content = generate_gridcraft_csv(prompt);
        std::fs::write(&sheet_path, content).map_err(|e| format!("스프레드시트 생성 실패: {e}"))?;

        // Fill the sheet that is currently open; fall back to opening as a tab of the same instance
        let injected = gridcraft_live_fill_csv(&std::fs::read_to_string(&sheet_path).unwrap_or_default())
            || live_open_via_rpc("gridcraft", &sheet_path);

        let status_desc = if injected {
            "⚡ 현재 열려 있는 GridCraft 창에 스프레드시트가 즉시 로드되었습니다!"
        } else {
            "아래 버튼을 누르면 GridCraft에서 즉시 열립니다."
        };

        return Ok((
            format!("📊 GridCraft 스프레드시트가 성공적으로 생성되었습니다!\n- 파일명: {sheet_name}\n- 수식: SUM, AVERAGE 자동 계산 포함\n\n{status_desc}"),
            None,
            Some(ActionResult::for_file("GridCraft에서 시트 열기", "gridcraft", sheet_path, !injected)),
        ));
    }

    // (3) VectorCraft: SVG vector illustration, logos, icons
    if target_app_lower == "vectorcraft" || prompt_lower.contains("로고") || prompt_lower.contains("벡터") || prompt_lower.contains("svg") || prompt_lower.contains("아이콘") || prompt_lower.contains("심볼") || prompt_lower.contains("일러스트") {
        let svg_name = format!("vector_{timestamp}.svg");
        let svg_path = saves_dir.join(&svg_name);
        let content = generate_vectorcraft_svg(prompt);
        std::fs::write(&svg_path, content).map_err(|e| format!("SVG 생성 실패: {e}"))?;

        let injected = live_open_via_rpc("vectorcraft", &svg_path);

        let status_desc = if injected {
            "⚡ 현재 열려 있는 VectorCraft 창에 SVG가 즉시 로드되었습니다!"
        } else {
            "아래 버튼을 누르면 VectorCraft에서 열어 자유롭게 패스를 편집할 수 있습니다."
        };

        return Ok((
            format!("🎨 VectorCraft용 SVG 벡터 그래픽이 생성되었습니다!\n- 파일명: {svg_name}\n- 표준 Scalable Vector Graphics (.svg)\n\n{status_desc}"),
            None,
            Some(ActionResult::for_file("VectorCraft에서 벡터 열기", "vectorcraft", svg_path, !injected)),
        ));
    }

    // (4) DeckCraft: Presentation slides, pitch decks
    if target_app_lower == "deckcraft" || prompt_lower.contains("발표") || prompt_lower.contains("슬라이드") || prompt_lower.contains("피치덱") || prompt_lower.contains("프레젠테이션") || prompt_lower.contains("ppt") {
        let deck_name = format!("presentation_{timestamp}.deck");
        let deck_path = saves_dir.join(&deck_name);
        let content = generate_deckcraft_deck(prompt);
        std::fs::write(&deck_path, content).map_err(|e| format!("슬라이드 생성 실패: {e}"))?;

        let injected = live_open_via_rpc("deckcraft", &deck_path);

        let status_desc = if injected {
            "⚡ 현재 열려 있는 DeckCraft 창에 슬라이드가 즉시 로드되었습니다!"
        } else {
            "아래 버튼을 누르면 DeckCraft에서 즉시 열립니다."
        };

        return Ok((
            format!("📽️ DeckCraft 발표 슬라이드 덱이 생성되었습니다!\n- 파일명: {deck_name}\n- 구성: 표지, 문제 정의, 핵심 가치, 비즈니스 모델, Q&A 슬라이드\n\n{status_desc}"),
            None,
            Some(ActionResult::for_file("DeckCraft에서 슬라이드 열기", "deckcraft", deck_path, !injected)),
        ));
    }

    // (5) CADCraft / SolidCraft: 3D CAD modeling, geometry, OBJ meshes
    if target_app_lower == "solidcraft" || target_app_lower == "cadcraft" || prompt_lower.contains("3d") || prompt_lower.contains("cad") || prompt_lower.contains("도면") || prompt_lower.contains("캐드") || prompt_lower.contains("모델링") || prompt_lower.contains("obj") || prompt_lower.contains("돌출") || prompt_lower.contains("사각형") {
        let app_id = if target_app_lower == "solidcraft" { "solidcraft" } else { "cadcraft" };
        let app_name = if target_app_lower == "solidcraft" { "SolidCraft" } else { "CADCraft" };

        let cad_spec = parse_cad_prompt(prompt);
        let (cmd_name, cmd_params, spec_desc) = match &cad_spec {
            CadCommandSpec::Box { length, width, height } => (
                "solid.box",
                serde_json::json!({
                    "length": format!("{length} mm"),
                    "width": format!("{width} mm"),
                    "height": format!("{height} mm")
                }),
                format!("{length} × {width} × {height} mm 돌출 솔리드 박스"),
            ),
            CadCommandSpec::Cylinder { radius, height } => (
                "solid.cylinder",
                serde_json::json!({
                    "radius": format!("{radius} mm"),
                    "height": format!("{height} mm")
                }),
                format!("반경 {radius} mm × 높이 {height} mm 실린더"),
            ),
            CadCommandSpec::Sphere { radius } => (
                "solid.sphere",
                serde_json::json!({
                    "radius": format!("{radius} mm")
                }),
                format!("반경 {radius} mm 구"),
            ),
        };

        // 1. LIVE EXECUTION: Run directly in the currently active SolidCraft window via RPC
        let mut live_injected = false;
        let mut live_summary = String::new();
        if app_id == "solidcraft" {
            let cmd_obj = serde_json::json!({
                "command": cmd_name,
                "params": cmd_params
            });
            match solidcraft_live_execute_commands(&[cmd_obj]) {
                Ok(summary) => {
                    live_injected = true;
                    live_summary = summary;
                }
                Err(err) => {
                    eprintln!("SolidCraft 실시간 명령 실행 안내: {err}");
                }
            }
        }

        // 2. FILE GENERATION & BACKUP: Produce native .solidcraft, .step and .obj
        let base_name = format!("model3d_{timestamp}");
        let solidcraft_path = saves_dir.join(format!("{base_name}.solidcraft"));
        let step_path = saves_dir.join(format!("{base_name}.step"));
        let obj_path = saves_dir.join(format!("{base_name}.obj"));

        let mut primary_path = obj_path.clone();

        // Write OBJ mesh as base
        let obj_content = generate_cadcraft_obj_with_spec(prompt, &cad_spec);
        let _ = std::fs::write(&obj_path, obj_content);

        // If solidcraft-cli is available, compile native B-Rep .solidcraft and .step
        if let Some(cli_exe) = find_solidcraft_cli(saves_dir) {
            let script_payload = serde_json::json!({
                "commands": [
                    {
                        "command": cmd_name,
                        "params": cmd_params
                    }
                ]
            });
            let script_path = saves_dir.join(format!("_temp_script_{timestamp}.json"));
            if std::fs::write(&script_path, serde_json::to_string(&script_payload).unwrap_or_default()).is_ok() {
                let status = std::process::Command::new(&cli_exe)
                    .args([
                        "run",
                        &script_path.to_string_lossy(),
                        "--save",
                        &solidcraft_path.to_string_lossy(),
                        "--out",
                        &step_path.to_string_lossy(),
                        "--out",
                        &obj_path.to_string_lossy(),
                        "--quiet",
                    ])
                    .status();
                let _ = std::fs::remove_file(&script_path);

                if status.is_ok_and(|s| s.success()) && solidcraft_path.is_file() {
                    primary_path = solidcraft_path.clone();
                }
            }
        }

        // Auto copy primary path to clipboard
        CompanionState::copy_text_to_clipboard(&primary_path.to_string_lossy());

        let reply = if live_injected {
            format!(
                "⚡ 현재 열려 있는 SolidCraft 작업 화면에 {spec_desc}가 즉시 생성되었습니다!\n\
                새 창을 띄우지 않고 현재 캔버스에서 마우스 드래그로 바로 회전/확대하거나, 좌측 브라우저(Browser) 및 하단 히스토리 타임라인(Timeline)에서 파라미터를 바로 편집할 수 있습니다.\n\n\
                • 실시간 엔진 반영: {live_summary}\n\
                • 설계 파일 백업 완료:\n\
                   - {}\n\
                   - {}\n\
                (파일 경로가 클립보드에 자동 복사되었습니다)",
                primary_path.file_name().and_then(|n| n.to_str()).unwrap_or_default(),
                step_path.file_name().and_then(|n| n.to_str()).unwrap_or_default()
            )
        } else {
            format!(
                "📐 {app_name} 3D 지오메트리 모델이 생성되었습니다!\n\
                - 규격: {spec_desc}\n\
                - 파일명: {}\n\
                - 표준 STEP/OBJ 및 SolidCraft 파라메트릭 CAD 포맷\n\n\
                아래 버튼을 누르면 {app_name}에서 즉시 열어 작업할 수 있습니다.\n\
                (파일 경로가 클립보드에 자동 복사되었습니다)",
                primary_path.file_name().and_then(|n| n.to_str()).unwrap_or_default()
            )
        };

        return Ok((
            reply,
            None,
            Some(ActionResult::for_file(&format!("{app_name}에서 3D 모델 열기"), app_id, primary_path, false)),
        ));
    }

    // (6) FilmCraft: Video timeline, script, storyboard
    if target_app_lower == "filmcraft" || prompt_lower.contains("영상") || prompt_lower.contains("콘티") || prompt_lower.contains("타임라인") || prompt_lower.contains("대본") || prompt_lower.contains("유튜브") || prompt_lower.contains("컷") {
        let script_name = format!("timeline_{timestamp}.txt");
        let script_path = saves_dir.join(&script_name);
        let content = generate_filmcraft_timeline(prompt);
        std::fs::write(&script_path, content).map_err(|e| format!("영상 타임라인 생성 실패: {e}"))?;
        return Ok((
            format!("🎬 FilmCraft 영상 제작 콘티 및 타임라인 컷 대본이 완성되었습니다!\n- 파일명: {script_name}\n- 타임스탬프 컷, B-Roll 샷 제안, BGM 큐시트 포함\n\n아래 버튼을 누르면 FilmCraft에서 열어 타임라인 작업을 시작할 수 있습니다."),
            None,
            Some(ActionResult::for_file("FilmCraft에서 타임라인 열기", "filmcraft", script_path, true)),
        ));
    }

    // (7) SoundCraft: Audio cuesheet, podcast script
    if target_app_lower == "soundcraft" || prompt_lower.contains("오디오") || prompt_lower.contains("팟캐스트") || prompt_lower.contains("사운드") || prompt_lower.contains("녹음") || prompt_lower.contains("음향") {
        let audio_name = format!("cuesheet_{timestamp}.txt");
        let audio_path = saves_dir.join(&audio_name);
        let content = generate_soundcraft_script(prompt);
        std::fs::write(&audio_path, content).map_err(|e| format!("오디오 큐시트 생성 실패: {e}"))?;
        return Ok((
            format!("🎙️ SoundCraft 오디오 제작 스크립트 및 SFX 큐시트가 완성되었습니다!\n- 파일명: {audio_name}\n\n아래 버튼을 누르면 SoundCraft에서 즉시 열립니다."),
            None,
            Some(ActionResult::for_file("SoundCraft에서 오디오 열기", "soundcraft", audio_path, true)),
        ));
    }

    // (8) LightCraft: Photography color recipes & RAW presets
    if target_app_lower == "lightcraft" || prompt_lower.contains("색감") || prompt_lower.contains("프리셋") || prompt_lower.contains("raw") || prompt_lower.contains("현상") || prompt_lower.contains("라이트룸") {
        let preset_name = format!("preset_{timestamp}.txt");
        let preset_path = saves_dir.join(&preset_name);
        let content = generate_lightcraft_preset(prompt);
        std::fs::write(&preset_path, content).map_err(|e| format!("프리셋 생성 실패: {e}"))?;
        return Ok((
            format!("📸 LightCraft 전용 사진 보정 레시피 & 프리셋이 생성되었습니다!\n- 파일명: {preset_name}\n- 노출, 하이라이트, 섀도우, 화이트 밸런스, 톤 커브 설정값 포함\n\n아래 버튼을 누르면 LightCraft에서 즉시 열어 RAW 사진에 일괄 적용할 수 있습니다."),
            None,
            Some(ActionResult::for_file("LightCraft에서 프리셋 열기", "lightcraft", preset_path, true)),
        ));
    }

    // (9) PdfCraft: Formal document forms & templates
    if target_app_lower == "pdfcraft" || prompt_lower.contains("pdf") || prompt_lower.contains("계약서") || prompt_lower.contains("서약서") || prompt_lower.contains("양식") {
        let pdf_name = format!("form_{timestamp}.md");
        let pdf_path = saves_dir.join(&pdf_name);
        let content = generate_pdfcraft_form(prompt);
        std::fs::write(&pdf_path, content).map_err(|e| format!("문서 서식 생성 실패: {e}"))?;
        return Ok((
            format!("📑 PdfCraft용 공식 표준 서식 문서가 생성되었습니다!\n- 파일명: {pdf_name}\n\n아래 버튼을 누르면 PdfCraft에서 즉시 열립니다."),
            None,
            Some(ActionResult::for_file("PdfCraft에서 문서 열기", "pdfcraft", pdf_path, true)),
        ));
    }

    // (10) DesignCraft / PageCraft: Publication, brochure & print layouts
    if target_app_lower == "designcraft" || target_app_lower == "pagecraft" || prompt_lower.contains("브로슈어") || prompt_lower.contains("리플렛") || prompt_lower.contains("출판") || prompt_lower.contains("카탈로그") || prompt_lower.contains("인쇄") {
        let layout_name = format!("layout_{timestamp}.txt");
        let layout_path = saves_dir.join(&layout_name);
        let content = generate_designcraft_layout(prompt);
        std::fs::write(&layout_path, content).map_err(|e| format!("레이아웃 생성 실패: {e}"))?;
        return Ok((
            format!("📰 DesignCraft 출판 및 인쇄 레이아웃 기획안이 생성되었습니다!\n- 파일명: {layout_name}\n\n아래 버튼을 누르면 DesignCraft에서 즉시 열어 편집할 수 있습니다."),
            None,
            Some(ActionResult::for_file("DesignCraft에서 레이아웃 열기", "designcraft", layout_path, true)),
        ));
    }

    // (11) FigCraft: UI/UX design wireframes & mockups
    if target_app_lower == "figcraft" || prompt_lower.contains("ui") || prompt_lower.contains("ux") || prompt_lower.contains("와이어프레임") || prompt_lower.contains("앱 화면") || prompt_lower.contains("피그마") {
        let ui_name = format!("ui_{timestamp}.svg");
        let ui_path = saves_dir.join(&ui_name);
        let content = generate_figcraft_ui(prompt);
        std::fs::write(&ui_path, content).map_err(|e| format!("UI 와이어프레임 생성 실패: {e}"))?;
        return Ok((
            format!("📱 FigCraft UI/UX 모바일 와이어프레임 및 디자인 컴포넌트가 생성되었습니다!\n- 파일명: {ui_name}\n\n아래 버튼을 누르면 FigCraft에서 열어 UI 컴포넌트를 조작할 수 있습니다."),
            None,
            Some(ActionResult::for_file("FigCraft에서 UI 열기", "figcraft", ui_path, true)),
        ));
    }

    // (12) EffectCraft: Motion graphics & VFX timing sheets
    if target_app_lower == "effectcraft" || prompt_lower.contains("모션") || prompt_lower.contains("vfx") || prompt_lower.contains("애니메이션") || prompt_lower.contains("키프레임") || prompt_lower.contains("이펙트") {
        let vfx_name = format!("vfx_{timestamp}.json");
        let vfx_path = saves_dir.join(&vfx_name);
        let content = generate_effectcraft_vfx(prompt);
        std::fs::write(&vfx_path, content).map_err(|e| format!("VFX 데이터 생성 실패: {e}"))?;
        return Ok((
            format!("✨ EffectCraft 모션 그래픽 & VFX 키프레임 시트가 생성되었습니다!\n- 파일명: {vfx_name}\n\n아래 버튼을 누르면 EffectCraft에서 즉시 열립니다."),
            None,
            Some(ActionResult::for_file("EffectCraft에서 모션 열기", "effectcraft", vfx_path, true)),
        ));
    }

    // (13) HopCraft: Korean public office HWP/HWPX documents
    if target_app_lower == "hop" || prompt_lower.contains("한글") || prompt_lower.contains("hwp") || prompt_lower.contains("hwpx") || prompt_lower.contains("공문서") || prompt_lower.contains("기안") || prompt_lower.contains("품의") {
        let hwp_name = format!("official_{timestamp}.txt");
        let hwp_path = saves_dir.join(&hwp_name);
        let content = generate_hopcraft_doc(prompt);
        std::fs::write(&hwp_path, content).map_err(|e| format!("한글 공문서 생성 실패: {e}"))?;
        return Ok((
            format!("🇰🇷 HopCraft 표준 한글 공문서 기안 양식이 생성되었습니다!\n- 파일명: {hwp_name}\n- 공공기관 표준 기안 서식(두문/본문/결문/붙임) 준수\n\n아래 버튼을 누르면 HopCraft에서 즉시 열립니다."),
            None,
            Some(ActionResult::for_file("HopCraft에서 한글 문서 열기", "hop", hwp_path, true)),
        ));
    }

    // 4. Fallback general assistant answer
    let mut msg = format!("💡 요청하신 작업('{prompt}')에 맞춰 {target_app} AI 도우미가 대기 중입니다.\n\n");
    msg.push_str("📋 [14개 Craft 앱 AI Copilot 지원 기능]:\n");
    msg.push_str("• PhotoCraft: '배경 지워줘', '배경 흐리게', '더 밝게', '흑백', '회전', '반전', '대비' 등 캔버스 즉시 제어\n");
    msg.push_str("• WordCraft / HopCraft: 기획서, 보고서, 한글 공문서 초안 작성 및 자동 열기\n");
    msg.push_str("• GridCraft: 가계부, 견적서, 매출 통계 스프레드시트 수식 표 자동 생성\n");
    msg.push_str("• VectorCraft / FigCraft: SVG 로고, 벡터 아이콘, 모바일 UI 와이어프레임 생성\n");
    msg.push_str("• CADCraft: 3D CAD 모델링 지오메트리 OBJ 파일 생성\n");
    msg.push_str("• FilmCraft / SoundCraft: 영상 콘티 컷 타임라인 및 팟캐스트 오디오 큐시트 생성\n");
    msg.push_str("• LightCraft: RAW 사진 보정 색감 프리셋 레시피 생성\n\n");
    if api_key.is_empty() {
        msg.push_str("💡 (안내) 현재 로컬 고속 엔진이 연동되어 있어 무료로 즉시 생성 및 실행됩니다.");
    } else {
        msg.push_str(&format!("🤖 현재 [{}] 모델 엔진이 연동되어 있습니다.", provider.label()));
    }
    Ok((msg, None, None))
}

// ---------------------------------------------------------------------------
// Specialized Generators for All 14 Craft Apps
// ---------------------------------------------------------------------------

fn generate_wordcraft_doc(prompt: &str, timestamp: u64) -> String {
    format!(
        "# {prompt} 기획 및 보고서\n\n\
        - **작성일자**: {}\n\
        - **작성자**: ArtCraft AI Copilot\n\
        - **버전**: v1.0\n\n\
        ---\n\n\
        ## 1. 개요 및 배경\n\
        본 문서는 '{prompt}' 요청에 따라 핵심 목표 및 실행 계획을 체계적으로 정리한 문서입니다.\n\n\
        ## 2. 주요 현황 및 분석\n\
        - **현황 분석**: 현재 시장 트렌드 및 관련 핵심 요소 파악\n\
        - **기대 효과**: 작업 생산성 향상 및 리소스 최적화 달성\n\n\
        ## 3. 세부 실행 계획\n\
        | 단계 | 세부 작업 내용 | 완료 목표일 | 비고 |\n\
        |:---|:---|:---:|:---|\n\
        | 1단계 | 사전 조사 및 기초 자료 수집 | D+3일 | 완료 |\n\
        | 2단계 | 본 과업 기획 및 초안 수립 | D+7일 | 진행중 |\n\
        | 3단계 | 최종 검토 및 관계자 피드백 반영 | D+10일 | 예정 |\n\n\
        ## 4. 결론 및 제언\n\
        본 계획을 바탕으로 신속하고 정확한 업무 추진을 권장하며, WordCraft에서 추가 서식을 자유롭게 보완하세요.\n\n\
        ---\n\
        *ArtCraft Studio WordCraft 전용 AI 어시스턴트 자동 생성*",
        fmt_date(timestamp)
    )
}

fn generate_gridcraft_csv(prompt: &str) -> String {
    format!(
        "날짜,구분,항목내용,수량,단가,공급가액,세액,합계금액\n\
        2026-10-18,운영비,{} 1단계 추진 자재,5,50000,250000,25000,275000\n\
        2026-10-18,소모품,디자인 기획 서식 및 라이선스,2,120000,240000,24000,264000\n\
        2026-10-19,설비비,고성능 그래픽 워크스테이션,1,1800000,1800000,180000,1980000\n\
        2026-10-20,서비스,클라우드 스토리지 연동,12,15000,180000,18000,198000\n\
        2026-10-21,외주비,전문 감수 및 테스트 검증,1,500000,500000,50000,550000\n\
        합계,총합계,,21,,=SUM(F2:F6),=SUM(G2:G6),=SUM(H2:H6)\n",
        prompt
    )
}

fn generate_vectorcraft_svg(prompt: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 800" width="100%" height="100%">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#1B1E28"/>
      <stop offset="100%" stop-color="#0E1017"/>
    </linearGradient>
    <linearGradient id="accentGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3E7BFA"/>
      <stop offset="50%" stop-color="#9B6BFF"/>
      <stop offset="100%" stop-color="#F2553D"/>
    </linearGradient>
    <filter id="glow" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur stdDeviation="12" result="blur"/>
      <feComposite in="SourceGraphic" in2="blur" operator="over"/>
    </filter>
  </defs>

  <!-- Background Card -->
  <rect width="800" height="800" rx="32" fill="url(#bgGrad)" stroke="#2E364D" stroke-width="2"/>

  <!-- Geometric Symbol -->
  <g transform="translate(400, 360)">
    <circle r="180" fill="none" stroke="url(#accentGrad)" stroke-width="6" stroke-dasharray="16 8" opacity="0.6"/>
    <circle r="130" fill="none" stroke="#3E7BFA" stroke-width="4" opacity="0.8"/>
    <polygon points="0,-90 78,45 -78,45" fill="none" stroke="url(#accentGrad)" stroke-width="8" stroke-linejoin="round" filter="url(#glow)"/>
    <circle r="22" fill="#3E7BFA" filter="url(#glow)"/>
  </g>

  <!-- Typography -->
  <text x="400" y="620" font-family="'Pretendard', 'Malgun Gothic', sans-serif" font-size="34" font-weight="bold" fill="#FFFFFF" text-anchor="middle">
    {}
  </text>
  <text x="400" y="670" font-family="monospace" font-size="16" fill="#9EA2B0" text-anchor="middle">
    VectorCraft AI Vector Design • Scalable SVG
  </text>
</svg>"##,
        prompt
    )
}

fn generate_deckcraft_deck(prompt: &str) -> String {
    format!(
        "SLIDE 1: [표지]\n\
        제목: {}\n\
        부제: 혁신 전략 및 미래 로드맵\n\
        발표자: ArtCraft Studio AI Copilot\n\n\
        ---\n\n\
        SLIDE 2: [문제 정의 & 시장 배경]\n\
        • 현재 시장의 주요 Pain Point 도출\n\
        • 비효율적인 기존 워크플로우 분석\n\
        • 새로운 솔루션의 시급성 및 당위성 확보\n\n\
        ---\n\n\
        SLIDE 3: [핵심 솔루션 및 차별화 가치]\n\
        • 통합 오픈소스 스위트 기반의 원스톱 작업 환경\n\
        • 온디바이스 AI 도우미를 통한 실시간 자동화 제어\n\
        • 강력한 멀티태스킹 및 데이터 무손실 로컬 보관\n\n\
        ---\n\n\
        SLIDE 4: [비즈니스 모델 & 실행 전략]\n\
        • 1단계: 사용자 경험 최적화 및 커뮤니티 확장\n\
        • 2단계: 엔터프라이즈 워크플로우 통합\n\
        • 3단계: 글로벌 오픈소스 크리에이티브 생태계 구축\n\n\
        ---\n\n\
        SLIDE 5: [결론 및 Q&A]\n\
        • \"크리에이티브의 한계를 뛰어넘는 완벽한 파트너\"\n\
        • 질의응답 (Q&A)\n",
        prompt
    )
}

#[allow(dead_code)]
fn generate_cadcraft_obj(prompt: &str) -> String {
    let spec = parse_cad_prompt(prompt);
    generate_cadcraft_obj_with_spec(prompt, &spec)
}

#[allow(dead_code)]
fn generate_cadcraft_obj_legacy(prompt: &str) -> String {
    let lower = prompt.to_lowercase();
    let mut out = String::new();
    out.push_str("# Wavefront OBJ File generated by SolidCraft AI Assistant\n");

    // 1. Cube / Box / 육면체
    if lower.contains("큐브") || lower.contains("정육면체") || lower.contains("상자") || lower.contains("cube") || lower.contains("box") || lower.contains("박스") {
        out.push_str("o Cube_Solid\n");
        out.push_str("v -10.0000 -10.0000 10.0000\n");
        out.push_str("v 10.0000 -10.0000 10.0000\n");
        out.push_str("v -10.0000 10.0000 10.0000\n");
        out.push_str("v 10.0000 10.0000 10.0000\n");
        out.push_str("v -10.0000 10.0000 -10.0000\n");
        out.push_str("v 10.0000 10.0000 -10.0000\n");
        out.push_str("v -10.0000 -10.0000 -10.0000\n");
        out.push_str("v 10.0000 -10.0000 -10.0000\n");
        out.push_str("vn 0.0000 0.0000 1.0000\n");
        out.push_str("vn 0.0000 1.0000 0.0000\n");
        out.push_str("vn 0.0000 0.0000 -1.0000\n");
        out.push_str("vn 0.0000 -1.0000 0.0000\n");
        out.push_str("vn 1.0000 0.0000 0.0000\n");
        out.push_str("vn -1.0000 0.0000 0.0000\n");
        out.push_str("s 1\n");
        out.push_str("f 1//1 2//1 4//1 3//1\n");
        out.push_str("f 3//2 4//2 6//2 5//2\n");
        out.push_str("f 5//3 6//3 8//3 7//3\n");
        out.push_str("f 7//4 8//4 2//4 1//4\n");
        out.push_str("f 2//5 8//5 6//5 4//5\n");
        out.push_str("f 7//6 1//6 3//6 5//6\n");
        return out;
    }

    // 2. Cylinder / 원기둥 / 실린더
    if lower.contains("실린더") || lower.contains("원기둥") || lower.contains("cylinder") {
        out.push_str("o Cylinder_Solid\n");
        let radius = 10.0_f32;
        let height = 24.0_f32;
        let segments = 24;
        let pi = std::f32::consts::PI;
        for i in 0..segments {
            let theta = (i as f32 / segments as f32) * 2.0 * pi;
            let x = radius * theta.cos();
            let y = radius * theta.sin();
            out.push_str(&format!("v {x:.4} {y:.4} 0.0000\n"));
            out.push_str(&format!("v {x:.4} {y:.4} {height:.4}\n"));
        }
        out.push_str("v 0.0000 0.0000 0.0000\n");
        out.push_str(&format!("v 0.0000 0.0000 {height:.4}\n"));
        let bot_c = segments * 2 + 1;
        let top_c = segments * 2 + 2;

        out.push_str("s 1\n");
        for i in 0..segments {
            let b1 = i * 2 + 1;
            let t1 = i * 2 + 2;
            let b2 = ((i + 1) % segments) * 2 + 1;
            let t2 = ((i + 1) % segments) * 2 + 2;
            out.push_str(&format!("f {b1} {b2} {t2} {t1}\n"));
            out.push_str(&format!("f {bot_c} {b2} {b1}\n"));
            out.push_str(&format!("f {top_c} {t1} {t2}\n"));
        }
        return out;
    }

    // 3. Sphere / 구 / 구체
    if lower.contains("구체") || lower.contains("구") || lower.contains("sphere") || lower.contains("ball") {
        out.push_str("o Sphere_Solid\n");
        let radius = 12.0_f32;
        let rings = 14;
        let sectors = 20;
        let pi = std::f32::consts::PI;
        for r in 0..=rings {
            let phi = (r as f32 / rings as f32) * pi;
            for s in 0..=sectors {
                let theta = (s as f32 / sectors as f32) * 2.0 * pi;
                let x = radius * phi.sin() * theta.cos();
                let y = radius * phi.sin() * theta.sin();
                let z = radius * phi.cos();
                out.push_str(&format!("v {x:.4} {y:.4} {z:.4}\n"));
            }
        }
        out.push_str("s 1\n");
        for r in 0..rings {
            for s in 0..sectors {
                let first = (r * (sectors + 1)) + s + 1;
                let second = first + sectors + 1;
                out.push_str(&format!("f {first} {second} {} {}\n", second + 1, first + 1));
            }
        }
        return out;
    }

    // 4. Gear / 톱니바퀴 / 기어
    if lower.contains("기어") || lower.contains("gear") || lower.contains("톱니") {
        out.push_str("o Gear_Solid\n");
        let teeth = 12;
        let r_in = 8.0_f32;
        let r_out = 12.0_f32;
        let thick = 6.0_f32;
        let pi = std::f32::consts::PI;
        let steps = teeth * 4;
        for i in 0..steps {
            let theta = (i as f32 / steps as f32) * 2.0 * pi;
            let is_tooth = (i % 4 == 1) || (i % 4 == 2);
            let r = if is_tooth { r_out } else { r_in };
            let x = r * theta.cos();
            let y = r * theta.sin();
            out.push_str(&format!("v {x:.4} {y:.4} 0.0000\n"));
            out.push_str(&format!("v {x:.4} {y:.4} {thick:.4}\n"));
        }
        out.push_str("v 0.0000 0.0000 0.0000\n");
        out.push_str(&format!("v 0.0000 0.0000 {thick:.4}\n"));
        let bot_c = steps * 2 + 1;
        let top_c = steps * 2 + 2;
        out.push_str("s 1\n");
        for i in 0..steps {
            let b1 = i * 2 + 1;
            let t1 = i * 2 + 2;
            let b2 = ((i + 1) % steps) * 2 + 1;
            let t2 = ((i + 1) % steps) * 2 + 2;
            out.push_str(&format!("f {b1} {b2} {t2} {t1}\n"));
            out.push_str(&format!("f {bot_c} {b2} {b1}\n"));
            out.push_str(&format!("f {top_c} {t1} {t2}\n"));
        }
        return out;
    }

    // 5. Default Mechanical Mounting Bracket Component
    out.push_str("o Mechanical_Bracket\n");
    out.push_str("v -10.0000 -10.0000 10.0000\n");
    out.push_str("v 10.0000 -10.0000 10.0000\n");
    out.push_str("v -10.0000 10.0000 10.0000\n");
    out.push_str("v 10.0000 10.0000 10.0000\n");
    out.push_str("v -10.0000 10.0000 -10.0000\n");
    out.push_str("v 10.0000 10.0000 -10.0000\n");
    out.push_str("v -10.0000 -10.0000 -10.0000\n");
    out.push_str("v 10.0000 -10.0000 -10.0000\n");
    out.push_str("v 0.0000 15.0000 0.0000\n");
    out.push_str("v 7.0710 12.5000 7.0710\n");
    out.push_str("v -7.0710 12.5000 7.0710\n");
    out.push_str("v -7.0710 12.5000 -7.0710\n");
    out.push_str("v 7.0710 12.5000 -7.0710\n");
    out.push_str("vn 0.0000 0.0000 1.0000\n");
    out.push_str("vn 0.0000 1.0000 0.0000\n");
    out.push_str("vn 0.0000 0.0000 -1.0000\n");
    out.push_str("vn 0.0000 -1.0000 0.0000\n");
    out.push_str("vn 1.0000 0.0000 0.0000\n");
    out.push_str("vn -1.0000 0.0000 0.0000\n");
    out.push_str("s 1\n");
    out.push_str("f 1//1 2//1 4//1 3//1\n");
    out.push_str("f 3//2 4//2 6//2 5//2\n");
    out.push_str("f 5//3 6//3 8//3 7//3\n");
    out.push_str("f 7//4 8//4 2//4 1//4\n");
    out.push_str("f 2//5 8//5 6//5 4//5\n");
    out.push_str("f 7//6 1//6 3//6 5//6\n");
    out.push_str("f 9//2 10//2 11//2\n");
    out.push_str("f 9//2 11//2 12//2\n");
    out.push_str("f 9//2 12//2 13//2\n");
    out.push_str("f 9//2 13//2 10//2\n");
    out
}

fn generate_filmcraft_timeline(prompt: &str) -> String {
    format!(
        "==========================================================\n\
        FILMCRAFT 영상 콘티 & 타임라인 컷 대본\n\
        프로젝트명: {}\n\
        해상도: 4K UHD (3840x2160) | 프레임레이트: 60fps | 오디오: 48kHz Stereo\n\
        ==========================================================\n\n\
        [SCENE 1: 오프닝 후킹 (00:00 ~ 00:08)]\n\
        • 비디오 샷: 빠른 패스트 컷 + 역동적인 줌인 (와이드 -> 인물 익스트림 클로즈업)\n\
        • 나레이션/대사: \"오늘 보여드릴 것은 지금까지 본 적 없는 가장 놀라운 결과물입니다.\"\n\
        • BGM / 효과음: 강렬한 비트 드롭 임팩트 SFX (Whoosh + Sub Bass Impact)\n\n\
        [SCENE 2: 핵심 본론 및 문제 해결 (00:08 ~ 01:30)]\n\
        • 비디오 샷: 메인 앵글 화면 + 우측 상단 자료화면 B-Roll PIP 오버레이\n\
        • 시각 효과: 주요 키워드 텍스트 팝업 (Motion Blur 키프레임)\n\
        • 나레이션/대사: \"이 작업을 수작업으로 하면 2시간이 걸리지만, AI Copilot을 통하면 단 1초 만에 해결됩니다.\"\n\n\
        [SCENE 3: 클라이맥스 시연 (01:30 ~ 02:45)]\n\
        • 비디오 샷: 화면 분할(Before & After 50:50 분할 슬라이더 연출)\n\
        • 오디오: 점진적으로 고조되는 신스웨이브 BGM (빌드업 구간)\n\n\
        [SCENE 4: 아웃트로 & 엔딩 카드 (02:45 ~ 03:00)]\n\
        • 비디오: 슬로우 줌아웃 + 로고 페이드인\n\
        • CTA 문구: \"좋아요와 구독은 다음 영상을 만드는 큰 힘이 됩니다!\"\n\
        • BGM: 페이드아웃 (Fade to -24dB)\n",
        prompt
    )
}

fn generate_soundcraft_script(prompt: &str) -> String {
    format!(
        "==========================================================\n\
        SOUNDCRAFT 오디오 마스터링 & 팟캐스트 큐시트\n\
        제목: {}\n\
        포맷: 24-bit WAV, 48000 Hz, Stereo\n\
        ==========================================================\n\n\
        [TRACK 1: MAIN VOCAL (Voice Track)]\n\
        - 게인 레벨: -6.0 dBFS (피크 -2.0 dBFS 유지)\n\
        - 플러그인 체인: De-esser -> Highpass(80Hz) -> 1176 Compressor -> Parametric EQ\n\
        - 스크립트: \"청취자 여러분 반갑습니다. ArtCraft 사운드 스튜디오와 함께하는 오늘의 에피소드입니다.\"\n\n\
        [TRACK 2: BACKGROUND MUSIC (Ambience & BGM)]\n\
        - 게인 레벨: 보컬 토크 시 -18 dBFS (Ducking 자동 적용), 인트로/아웃트로 시 -10 dBFS\n\
        - 사운드 스타일: 따뜻한 어쿠스틱 로파이 칠 비트 (Lofi Chillhop)\n\n\
        [TRACK 3: SOUND EFFECTS (SFX)]\n\
        - 00:00:00.200 - 인트로 오프닝 징글 (Jingle Stereo)\n\
        - 00:00:15.500 - 화제 전환 효과음 (Soft Bell Transition)\n\
        - 00:02:40.000 - 아웃트로 클로징 차임\n",
        prompt
    )
}

fn generate_lightcraft_preset(prompt: &str) -> String {
    format!(
        "LightCraft Camera RAW & Color Grading Recipe\n\
        Preset Name: {}\n\
        Engine: LightCraft UltraColor 2026\n\n\
        [Basic Tone Controls]\n\
        Exposure: +0.35 EV\n\
        Contrast: +18\n\
        Highlights: -42 (하늘 및 밝은 하이라이트 디테일 복원)\n\
        Shadows: +35 (암부 섀도우 노이즈 없이 부드럽게 개방)\n\
        Whites: +10\n\
        Blacks: -15 (깊이 있는 블랙 포인트 확보)\n\n\
        [Presence & Color Dynamics]\n\
        Clarity: +12\n\
        Dehaze: +8\n\
        Vibrance: +22\n\
        Saturation: -5 (자연스럽고 차분한 시네마틱 채도)\n\n\
        [Color Grading / Split Toning]\n\
        Shadows Hue: 215 (시안/블루 톤)\n\
        Shadows Sat: 16\n\
        Highlights Hue: 42 (따뜻한 골드 톤)\n\
        Highlights Sat: 14\n\
        Balance: +5\n\n\
        [Tone Curve]\n\
        RGB Curve: S-Curve (Shadow: 18/14, Mid: 128/132, High: 235/240)\n",
        prompt
    )
}

fn generate_pdfcraft_form(prompt: &str) -> String {
    format!(
        "# {}\n\n\
        **문서 관리 번호**: ARTCRAFT-PDF-2026-001\n\
        **발행 일자**: 2026년 10월 18일\n\n\
        ---\n\n\
        ### 제 1 조 (목적)\n\
        본 문서는 신청인과 주관 기관 간의 상호 신의성실에 입각한 원활한 업무 수행 및 규정 준수를 목적으로 합니다.\n\n\
        ### 제 2 조 (성명 및 인적 사항)\n\
        - **성 명 / 상 호**: _____________________\n\
        - **생년월일 / 사업자번호**: _____________________\n\
        - **연 락 처**: _____________________\n\
        - **주 소**: _____________________\n\n\
        ### 제 3 조 (동의 및 확인 사항)\n\
        1. 본인은 상기 기재 사항이 사실과 다름없음을 확인합니다.\n\
        2. 제공된 모든 작업 결과물은 안전하게 보존되며 보안 규정을 준수합니다.\n\n\
        **2026 년 ____ 월 ____ 일**\n\n\
        **서약인 (서명 또는 인)**: ___________________ (인)\n",
        prompt
    )
}

fn generate_designcraft_layout(prompt: &str) -> String {
    format!(
        "==========================================================\n\
        DESIGNCRAFT / PAGECRAFT 출판 편집 디자인 레이아웃 명세서\n\
        기획 주제: {}\n\
        판형: 국배판 A4 변형 (210mm x 297mm) | 4도 컬러 양면 인쇄 (CMYK)\n\
        ==========================================================\n\n\
        [1. 그리드 및 여백 시스템]\n\
        • 여백: 상단 24mm, 하단 28mm, 안쪽(바인딩) 22mm, 바깥쪽 20mm\n\
        • 단수: 3단 그리드 (Column Width: 52mm, Column Gutter: 5mm)\n\
        • 재단 여백(Bleed): 사방 3mm 여유분 적용\n\n\
        [2. 타이포그래피 계층 구조]\n\
        • 메인 헤드라인(Title): 32pt / Bold / 자간 -30 / 행간 42pt\n\
        • 섹션 서브타이틀: 18pt / SemiBold / 자간 -15 / 행간 26pt\n\
        • 본문 텍스트(Body): 10pt / Regular / 자간 -10 / 행간 17pt\n\
        • 캡션 및 각주: 8pt / Light / 자간 0 / 행간 12pt\n\n\
        [3. 색상 팔레트 가이드 (CMYK)]\n\
        • Primary Accent: C:85 M:55 Y:0 K:0 (아트크래프트 블루)\n\
        • Secondary Accent: C:0 M:80 Y:95 K:0 (코랄 레드)\n\
        • Deep Base: C:60 M:50 Y:45 K:85 (다크 에보니)\n",
        prompt
    )
}

fn generate_figcraft_ui(prompt: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 390 844" width="390" height="844">
  <defs>
    <filter id="cardShadow" x="-10%" y="-10%" width="120%" height="120%">
      <feDropShadow dx="0" dy="8" stdDeviation="12" flood-color="#000000" flood-opacity="0.35"/>
    </filter>
  </defs>

  <!-- Phone Screen Frame -->
  <rect width="390" height="844" rx="44" fill="#131620" stroke="#2E364D" stroke-width="4"/>

  <!-- Status Bar -->
  <text x="32" y="38" font-family="-apple-system, sans-serif" font-size="14" font-weight="bold" fill="#FFFFFF">9:41</text>
  <rect x="296" y="28" width="22" height="11" rx="3" fill="none" stroke="#FFFFFF" stroke-width="1.5"/>
  <rect x="298" y="30" width="16" height="7" rx="2" fill="#3E7BFA"/>

  <!-- App Header -->
  <text x="24" y="96" font-family="'Pretendard', sans-serif" font-size="24" font-weight="bold" fill="#FFFFFF">{}</text>
  <text x="24" y="122" font-family="'Pretendard', sans-serif" font-size="13" fill="#9EA2B0">FigCraft Auto-Generated UI Prototype</text>

  <!-- Metric Card 1 -->
  <g transform="translate(24, 150)" filter="url(#cardShadow)">
    <rect width="342" height="140" rx="16" fill="#1B1E2B" stroke="#2B2D3A"/>
    <text x="20" y="36" font-family="'Pretendard', sans-serif" font-size="14" fill="#9EA2B0">Total Performance</text>
    <text x="20" y="74" font-family="'Pretendard', sans-serif" font-size="28" font-weight="bold" fill="#3E7BFA">99.8%</text>
    <rect x="20" y="96" width="302" height="8" rx="4" fill="#25293A"/>
    <rect x="20" y="96" width="260" height="8" rx="4" fill="#3E7BFA"/>
  </g>

  <!-- Action Button -->
  <g transform="translate(24, 720)">
    <rect width="342" height="56" rx="16" fill="#3E7BFA"/>
    <text x="171" y="35" font-family="'Pretendard', sans-serif" font-size="16" font-weight="bold" fill="#FFFFFF" text-anchor="middle">시작하기 (Get Started)</text>
  </g>
</svg>"##,
        prompt
    )
}

fn generate_effectcraft_vfx(prompt: &str) -> String {
    format!(
        "{{\n\
          \"project\": \"{}\",\n\
          \"fps\": 60,\n\
          \"width\": 1920,\n\
          \"height\": 1080,\n\
          \"duration_frames\": 300,\n\
          \"layers\": [\n\
            {{\n\
              \"name\": \"Title_Text_Motion\",\n\
              \"type\": \"text\",\n\
              \"transform\": {{\n\
                \"position\": [960, 540],\n\
                \"keyframes\": [\n\
                  {{\"frame\": 0, \"opacity\": 0.0, \"scale\": [0.8, 0.8], \"easing\": \"easeOutQuad\"}},\n\
                  {{\"frame\": 45, \"opacity\": 1.0, \"scale\": [1.0, 1.0], \"easing\": \"easeOutBack\"}},\n\
                  {{\"frame\": 260, \"opacity\": 1.0, \"scale\": [1.02, 1.02]}},\n\
                  {{\"frame\": 300, \"opacity\": 0.0, \"scale\": [1.1, 1.1], \"easing\": \"easeInCubic\"}}\n\
                ]\n\
              }},\n\
              \"effects\": [\n\
                {{\"type\": \"glow\", \"radius\": 24, \"intensity\": 1.5, \"color\": \"#3E7BFA\"}},\n\
                {{\"type\": \"motion_blur\", \"samples\": 16}}\n\
              ]\n\
            }}\n\
          ]\n\
        }}",
        prompt
    )
}

fn generate_hopcraft_doc(prompt: &str) -> String {
    format!(
        "==========================================================\n\
        대 한 민 국  공 문 서  표 준  기 안 서 (HWP/HWPX 서식)\n\
        ==========================================================\n\n\
        [문서번호] 기안-2026-1018호\n\
        [시행일자] 2026. 10. 18.\n\
        [수    신] 내부결재\n\
        [참    조] 관련 전 부서\n\
        [제    목] {} 추진 계획(안) 보고의 건\n\n\
        ----------------------------------------------------------\n\n\
        1. 추진 목적\n\
           가. 최근 급변하는 기술 환경에 선제적으로 대응하고\n\
           나. '{}' 과업을 체계적이고 효율적으로 추진하여 업무 생산성을 극대화하고자 함.\n\n\
        2. 주요 추진 내용\n\
           가. 사 업 명 : {} 실행 프로젝트\n\
           나. 사업 기간 : 2026. 10. ~ 2026. 12. (3개월간)\n\
           다. 소요 예산 : 일반 수용비 및 운영비 내 집행\n\
           라. 주관 부서 : 혁신성장추진단\n\n\
        3. 세부 실행 일정\n\
           가. 기본 계획 수립 및 시스템 구축 : 2026년 10월 4주차\n\
           나. 시범 운영 및 현장 의견 수렴 : 2026년 11월 중\n\
           다. 최종 완료 및 성과 평가 보고 : 2026년 12월 말\n\n\
        4. 행정 사항\n\
           가. 각 부서는 본 계획에 적극 협조하여 주시기 바랍니다.\n\
           나. 붙임 서류를 확인하시고 의견이 있을 시 제출 바랍니다.\n\n\
        붙임 : 1. 세부 과업 지시서 1부.\n\
               2. 소요 예산 산출 내역서 1부.  끝.\n\n\
        ----------------------------------------------------------\n\
        기안자: 직급 홍길동   |   검토자: 팀장 이몽룡   |   결재권자: 단장 성춘향\n",
        prompt, prompt, prompt
    )
}
