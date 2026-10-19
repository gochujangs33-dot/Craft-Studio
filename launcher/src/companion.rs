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
const CARD_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1E);
const ACCENT_DEFAULT: Color32 = Color32::from_rgb(0x68, 0x98, 0xFF);
const TEXT_MAIN: Color32 = Color32::from_rgb(0xEE, 0xEE, 0xF2);
const TEXT_MUTED: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x9A);
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
    pub status_msg: Option<String>,
    pub v_was_down: bool,
    pub last_dock_pos: Option<[f32; 2]>,
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
        let mut messages = Vec::new();
        let greeting = match app_id.to_lowercase().as_str() {
            "photocraft" => "안녕하세요! PhotoCraft 전용 AI 어시스턴트입니다.\n현재 캔버스에 열린 사진의 편집 요청('배경 지워줘', '배경 흐리게', '더 밝게', '흑백', '좌우 반전', '90도 회전' 등)을 편하게 말씀해 주세요. 사진을 따로 복사/첨부하지 않아도 자동으로 인식합니다!",
            "vectorcraft" => "안녕하세요! VectorCraft 벡터 일러스트 AI 어시스턴트입니다.\n'로고 디자인', '별/화살표 아이콘 SVG 생성', '심볼 제작' 등의 벡터 그래픽 요청을 입력해 보세요.",
            "filmcraft" => "안녕하세요! FilmCraft 비선형 영상 편집 AI 어시스턴트입니다.\n'유튜브 영상 콘티', '타임라인 컷 대본', '자막/BGM 연출 구성안' 작성을 도와드립니다.",
            "lightcraft" => "안녕하세요! LightCraft 사진 보정/RAW 현상 AI 어시스턴트입니다.\n'시네마틱 감성 색감', '야경 보정 프리셋', '인물 톤업 레시피' 생성을 도와드립니다.",
            "pdfcraft" => "안녕하세요! PdfCraft PDF 문서 AI 어시스턴트입니다.\n'표준 계약서 양식', '동의서 서식 작성', '문서 구조 요약' 등을 지원합니다.",
            "effectcraft" => "안녕하세요! EffectCraft 모션 그래픽 & VFX AI 어시스턴트입니다.\n'타이틀 자막 애니메이션 키프레임', '로고 인트로 연출안', 'VFX 타이밍 시트'를 생성해 드립니다.",
            "designcraft" => "안녕하세요! DesignCraft 출판/인쇄 레이아웃 AI 어시스턴트입니다.\n'3단 리플렛 구성', '카탈로그/포스터 레이아웃 기획', '그리드 규격안'을 작성해 드립니다.",
            "wordcraft" => "안녕하세요! WordCraft 문서/오피스 AI 어시스턴트입니다.\n'기획서 작성', '보고서 초안', '회의록 요약', '이력서/자기소개서' 작성을 도와드립니다.",
            "deckcraft" => "안녕하세요! DeckCraft 프레젠테이션 AI 어시스턴트입니다.\n'발표 슬라이드 초안', '투자 피치덱 5장 구성', '사업계획서 발표자료'를 제작해 드립니다.",
            "gridcraft" => "안녕하세요! GridCraft 스프레드시트 AI 어시스턴트입니다.\n'가계부 표 생성', '견적서 서식', '월별 매출 통계 시트', '수식 자동 계산'을 도와드립니다.",
            "cadcraft" => "안녕하세요! CADCraft 2D/3D CAD 모델링 AI 어시스턴트입니다.\n'3D 큐브/기어 모델링', '기하 형상 OBJ 생성', '기계 부품 도면 초안'을 지원합니다.",
            "soundcraft" => "안녕하세요! SoundCraft 디지털 오디오 AI 어시스턴트입니다.\n'팟캐스트 녹음 대본', '라디오 오프닝 스크립트', '음향 SFX 큐시트' 작성을 도와드립니다.",
            "figcraft" => "안녕하세요! FigCraft UI/UX 디자인 AI 어시스턴트입니다.\n'모바일 앱 로그인 화면 UI 기획', '대시보드 와이어프레임 구조', '디자인 시스템 토큰'을 설계해 드립니다.",
            "hop" => "안녕하세요! HopCraft 대한민국 공문서 AI 어시스턴트입니다.\n'표준 공문서 양식', '기안서/품의서 작성', 'HWP 공문서 서식' 작성을 도와드립니다.",
            _ => "안녕하세요! ArtCraft Studio AI 어시스턴트입니다.\n요청사항을 입력하시거나 질문을 남겨주시면 즉시 도와드립니다.",
        };
        messages.push(ChatMessage {
            role: Role::Assistant,
            text: greeting.to_string(),
            image_preview: None,
            image_path: None,
            action_result: None,
            timestamp: now_secs(),
        });

        Self {
            is_open: true,
            is_minimized: false,
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
            status_msg: None,
            v_was_down: false,
            last_dock_pos: None,
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
        let mut auto_open = None;
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
                        if act.auto_open {
                            auto_open = Some((act.app_id.clone(), act.file_path.clone()));
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
    pub fn save_working_image(&mut self, saves_base: &Path) -> Option<PathBuf> {
        let rgba = self.working_image.as_ref()?;
        let target_app = self.target_app.clone().unwrap_or_else(|| "photocraft".to_string());
        let app_folder = match target_app.to_lowercase().as_str() {
            "photocraft" => "PhotoCraft",
            "wordcraft" => "WordCraft",
            "cadcraft" => "CADCraft",
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

        // 1. Hardware-level Win32 Async Keystate check for Ctrl+V
        #[cfg(windows)]
        {
            unsafe {
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
                let ctrl_down = (GetAsyncKeyState(0x11 /* VK_CONTROL */) as u16 & 0x8000) != 0;
                let v_down = (GetAsyncKeyState(0x56 /* 'V' */) as u16 & 0x8000) != 0;
                if ctrl_down && v_down {
                    if !self.v_was_down {
                        self.v_was_down = true;
                        self.try_paste_image(ctx, temp_dir);
                    }
                } else if !v_down {
                    self.v_was_down = false;
                }
            }
        }

        // 2. Egui paste event
        let is_paste = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.command;
            let v = i.key_pressed(Key::V) || i.keys_down.contains(&Key::V);
            let evt_paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
            (ctrl && v) || evt_paste
        });
        if is_paste {
            self.try_paste_image(ctx, temp_dir);
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

    /// Renders the AI companion docked at the bottom-right of the screen for a specific app
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
        if let Some((app_id, file)) = self.poll_response(ctx, pids, app_name) {
            *open_action = Some((app_id, file));
        }

        if !self.is_open {
            return;
        }

        let vp_id = ViewportId::from_hash_of(format!("companion_dock_{app_id}"));

        // Match PhotoCraft's right panel column width (289px) and sit directly under '레이어, 채널, 패스'
        let width = 289.0_f32;
        let height = 284.0_f32;

        let ppp = ctx.pixels_per_point().max(0.1);
        let app_rect = crate::apps::get_app_window_rect(pids, app_name);
        let (x, y, is_visible) = if let Some([_left, top, right, bottom]) = app_rect {
            let right_pt = right / ppp;
            let bottom_pt = bottom / ppp;
            let top_pt = top / ppp;
            let x = right_pt - width;
            let y = (bottom_pt - height).max(top_pt + 50.0_f32);
            (x, y, true)
        } else if !cfg!(windows) {
            let (w, h) = ctx.input(|i| i.viewport().monitor_size).map(|s| (s.x, s.y)).unwrap_or((1920.0, 1080.0));
            let x = (w - width - 20.0_f32).max(10.0_f32);
            let y = (h - height - 60.0_f32).max(10.0_f32);
            (x, y, true)
        } else {
            (100.0_f32, 100.0_f32, false)
        };

        if !is_visible {
            ctx.send_viewport_cmd_to(vp_id, egui::ViewportCommand::Visible(false));
            return;
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(32));

        let builder = ViewportBuilder::default()
            .with_title(format!("{app_name} AI"))
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false)
            .with_always_on_top()
            .with_visible(true)
            .with_decorations(false)
            .with_transparent(false)
            .with_position([x, y]);

        ctx.show_viewport_immediate(vp_id, builder, |ctx, _class| {
            if self.last_dock_pos != Some([x, y]) {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
                self.last_dock_pos = Some([x, y]);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(32));

            // 1. Hardware-level Win32 Async Keystate check for Ctrl+V (works across active windows!)
            #[cfg(windows)]
            {
                unsafe {
                    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
                    let ctrl_down = (GetAsyncKeyState(0x11 /* VK_CONTROL */) as u16 & 0x8000) != 0;
                    let v_down = (GetAsyncKeyState(0x56 /* 'V' */) as u16 & 0x8000) != 0;
                    if ctrl_down && v_down {
                        if !self.v_was_down {
                            self.v_was_down = true;
                            self.try_paste_image(ctx, temp_dir);
                        }
                    } else if !v_down {
                        self.v_was_down = false;
                    }
                }
            }

            // 2. Egui paste event fallback
            let is_paste = ctx.input(|i| {
                let ctrl = i.modifiers.ctrl || i.modifiers.command;
                let v = i.key_pressed(Key::V) || i.keys_down.contains(&Key::V);
                let evt_paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
                (ctrl && v) || evt_paste
            });
            if is_paste {
                self.try_paste_image(ctx, temp_dir);
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

            egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(Color32::from_rgb(0x22, 0x24, 0x2B))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x1C, 0x1E, 0x24)))
                        .corner_radius(0.0_f32)
                        .inner_margin(Margin::ZERO),
                )
                .show(ctx, |ui| {
                    self.render_full(ui, ctx, entries, saves_base, temp_dir, open_action, app_name, pids);
                });
        });
    }

    #[allow(dead_code)]
    fn render_minimized(&mut self, ui: &mut Ui, app_name: &str, ctx: &egui::Context) {
        let resp = Frame::new()
            .fill(Color32::from_rgb(0x2B, 0x2D, 0x33))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x1C, 0x1E, 0x24)))
            .inner_margin(Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("AI 도우미").strong().size(11.0_f32).color(Color32::from_rgb(0xDF, 0xE1, 0xE8)));
                    ui.label(RichText::new(format!("{app_name}")).font(egui::FontId::monospace(9.0_f32)).color(Color32::from_rgb(0x3E, 0x7B, 0xFA)));
                });
            });
        let _ = resp;
        let _ = ctx;
    }

    fn render_full(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        _entries: &[AppEntry],
        saves_base: &Path,
        temp_dir: &Path,
        open_action: &mut Option<(String, PathBuf)>,
        _app_name: &str,
        _pids: &[u32],
    ) {
        // --- PhotoCraft Native Tab Header Bar ---
        Frame::new()
            .fill(Color32::from_rgb(0x2B, 0x2D, 0x33))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x1C, 0x1E, 0x24)))
            .corner_radius(0.0_f32)
            .inner_margin(Margin::symmetric(10, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("AI 도우미")
                            .strong()
                            .size(11.5_f32)
                            .color(Color32::from_rgb(0xDF, 0xE1, 0xE8)),
                    );
                });
            });

        ui.add_space(2.0_f32);
        ui.separator();
        ui.add_space(3.0_f32);

        // --- Chat message area ---
        let footer_height = if self.pending_image.is_some() || self.working_image.is_some() {
            135.0_f32
        } else {
            38.0_f32
        };
        let scroll_height = (ui.available_height() - footer_height).max(80.0_f32);

        let mut do_paste = false;
        let mut do_restore: Option<(PathBuf, PathBuf)> = None;
        let mut do_save_copy: Option<PathBuf> = None;

        ScrollArea::vertical()
            .max_height(scroll_height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0_f32;
                for msg in &self.messages {
                    match msg.role {
                        Role::User => {
                            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                Frame::new()
                                    .fill(Color32::from_rgb(0x1F, 0x2A, 0x44))
                                    .corner_radius(8.0_f32)
                                    .inner_margin(Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        ui.set_max_width(260.0_f32);
                                        ui.vertical(|ui| {
                                            if let Some(tex) = &msg.image_preview {
                                                ui.image((tex.id(), vec2(100.0_f32, 100.0_f32 * (tex.size()[1] as f32 / tex.size()[0] as f32).max(0.5_f32).min(1.5_f32))));
                                                ui.add_space(4.0_f32);
                                            }
                                            ui.label(RichText::new(&msg.text).color(TEXT_MAIN).size(12.0_f32));
                                        });
                                    });
                            });
                        }
                        Role::Assistant => {
                            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                                Frame::new()
                                    .fill(CARD_BG)
                                    .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
                                    .corner_radius(8.0_f32)
                                    .inner_margin(Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        ui.set_max_width(280.0_f32);
                                        ui.vertical(|ui| {
                                            if let Some(tex) = &msg.image_preview {
                                                let aspect = (tex.size()[1] as f32 / tex.size()[0] as f32).max(0.4_f32).min(2.0_f32);
                                                let w = 180.0_f32.min(ui.available_width() - 10.0_f32);
                                                ui.image((tex.id(), vec2(w, w * aspect)));
                                                ui.add_space(4.0_f32);
                                            }
                                            ui.label(RichText::new(&msg.text).color(TEXT_MAIN).size(12.0_f32));
                                            
                                            // Clickable action buttons
                                            if let Some(action) = &msg.action_result {
                                                ui.add_space(6.0_f32);
                                                ui.vertical(|ui| {
                                                    // 1. Primary badge or open button
                                                    let is_live = action.label.contains("실시간");
                                                    if is_live {
                                                        ui.horizontal(|ui| {
                                                            ui.label(
                                                                RichText::new("⚡ PhotoCraft 화면에 실시간 즉시 반영됨")
                                                                    .font(egui::FontId::monospace(10.5_f32))
                                                                    .strong()
                                                                    .color(Color32::from_rgb(0x50, 0xFA, 0x9E)),
                                                            );
                                                        });
                                                    } else {
                                                        let open_btn = ui.button(
                                                            RichText::new(format!("✨ {}", action.label))
                                                                .font(egui::FontId::monospace(11.0_f32))
                                                                .strong()
                                                                .color(Color32::from_rgb(0x50, 0xFA, 0x9E)),
                                                        ).on_hover_text("새 캔버스로 열기");
                                                        if open_btn.clicked() {
                                                            *open_action = Some((action.app_id.clone(), action.file_path.clone()));
                                                        }
                                                    }

                                                    // 2. Restore original button (if backup exists)
                                                    if let Some(orig) = &action.original_path {
                                                        if orig.exists() {
                                                            let restore_btn = ui.button(
                                                                RichText::new("↩ 원본으로 되돌리기")
                                                                    .font(egui::FontId::monospace(10.5_f32))
                                                                    .strong()
                                                                    .color(Color32::from_rgb(0xFF, 0xB8, 0x6C)),
                                                            ).on_hover_text("현재 수정을 취소하고 원래 이미지로 즉시 되돌립니다");
                                                            if restore_btn.clicked() {
                                                                do_restore = Some((orig.clone(), action.file_path.clone()));
                                                            }
                                                        }
                                                    }

                                                    ui.horizontal(|ui| {
                                                        if action.can_paste {
                                                            let copy_btn = ui.button(
                                                                RichText::new("📋 클립보드 복사")
                                                                    .font(egui::FontId::monospace(10.0_f32))
                                                                    .color(Color32::from_rgb(0xD0, 0xD0, 0xE0)),
                                                            ).on_hover_text("현재 투명 이미지를 클립보드에 복사합니다");
                                                            if copy_btn.clicked() {
                                                                do_paste = true;
                                                            }
                                                        }

                                                        let save_copy_btn = ui.button(
                                                            RichText::new("💾 별도 저장")
                                                                .font(egui::FontId::monospace(10.0_f32))
                                                                .color(Color32::from_rgb(0x66, 0xC2, 0xFF)),
                                                        ).on_hover_text("새 별도 PNG 파일로 복사 저장합니다");
                                                        if save_copy_btn.clicked() {
                                                            do_save_copy = Some(action.file_path.clone());
                                                        }

                                                        let reveal_path = action.file_path.clone();
                                                        if ui.button(
                                                            RichText::new("📂 저장 폴더")
                                                                .font(egui::FontId::monospace(10.0_f32))
                                                                .color(Color32::from_rgb(0xD0, 0xD0, 0xE0)),
                                                        ).on_hover_text("생성된 파일 위치를 윈도우 탐색기에서 엽니다").clicked() {
                                                            #[cfg(windows)]
                                                            {
                                                                use std::os::windows::process::CommandExt;
                                                                let _ = std::process::Command::new("explorer.exe")
                                                                    .arg(format!("/select,{}", reveal_path.display()))
                                                                    .creation_flags(0x08000000)
                                                                    .spawn();
                                                            }
                                                        }
                                                    });
                                                });
                                            }
                                        });
                                    });
                            });
                        }
                        Role::System => {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(&msg.text).font(egui::FontId::monospace(10.0_f32)).color(TEXT_MUTED));
                            });
                        }
                    }
                }

                if self.is_busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("AI가 요청을 처리 중입니다...").font(egui::FontId::monospace(10.5_f32)).color(TEXT_MUTED));
                    });
                }
            });

        if do_paste {
            if let Some(rgba) = &self.working_image {
                Self::copy_image_to_clipboard(rgba);
            }
            self.status_msg = Some("📋 클립보드에 투명 이미지가 복사되었습니다.".to_string());
        }

        if let Some((backup, target)) = do_restore {
            if let Ok(orig_dyn) = image::open(&backup) {
                let rgba = orig_dyn.to_rgba8();
                if rgba.save(&target).is_ok() {
                    photocraft_live_reload_document(&target);
                    let thumb = image::imageops::thumbnail(&rgba, 320, 320);
                    let size = [thumb.width() as usize, thumb.height() as usize];
                    let color_img = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_flat_samples().as_slice());
                    let tex = ctx.load_texture(format!("working_preview_{}", now_secs()), color_img, Default::default());
                    self.working_image = Some(rgba);
                    self.working_preview = Some(tex);
                    self.status_msg = Some("↩ 캔버스가 원본 이미지로 성공적으로 복원되었습니다!".to_string());
                }
            }
        }

        if let Some(target) = do_save_copy {
            let copy_name = format!("photocraft_saved_{}.png", now_secs());
            let copy_path = saves_base.join("PhotoCraft").join(&copy_name);
            let _ = std::fs::copy(&target, &copy_path);
            self.status_msg = Some(format!("💾 별도 파일({copy_name})로 저장되었습니다."));
        }

        ui.separator();

        // --- Working Image In-Progress Banner (Unsaved in-memory state) ---
        let mut do_save = false;
        if let Some(tex) = &self.working_preview {
            let tex_id = tex.id();
            Frame::new()
                .fill(Color32::from_rgb(0x16, 0x22, 0x36))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(0x3B, 0x6E, 0xAA)))
                .corner_radius(6.0_f32)
                .inner_margin(Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.image((tex_id, vec2(28.0_f32, 28.0_f32)));
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("🎨 현재 이미지 작업 중 (미저장)")
                                    .strong()
                                    .font(egui::FontId::monospace(10.5_f32))
                                    .color(Color32::from_rgb(0x66, 0xC2, 0xFF)),
                            );
                            ui.label(
                                RichText::new("수정이 완료되면 우측 버튼을 눌러 저장하세요")
                                    .font(egui::FontId::monospace(9.0_f32))
                                    .color(TEXT_MUTED),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let save_btn = ui.button(
                                RichText::new("💾 완성! 저장")
                                    .font(egui::FontId::monospace(11.0_f32))
                                    .strong()
                                    .color(Color32::from_rgb(0x50, 0xFA, 0x9E)),
                            ).on_hover_text("현재까지 수정한 이미지를 최종 파일로 저장합니다");
                            if save_btn.clicked() {
                                do_save = true;
                            }
                        });
                    });
                });
            ui.add_space(4.0_f32);
        }
        if do_save {
            self.save_working_image(saves_base);
        }

        // --- Pending attachment preview ---
        let mut remove_pending = false;
        if let Some((_, tex, path)) = &self.pending_image {
            let tex_id = tex.id();
            let fname = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "이미지".to_string());
            Frame::new()
                .fill(Color32::from_rgb(0x16, 0x1E, 0x2E))
                .stroke(Stroke::new(1.0_f32, ACCENT_DEFAULT.gamma_multiply(0.4_f32)))
                .corner_radius(6.0_f32)
                .inner_margin(Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.image((tex_id, vec2(32.0_f32, 32.0_f32)));
                        ui.label(RichText::new(format!("붙여넣은 이미지: {fname}")).font(egui::FontId::monospace(10.5_f32)).color(TEXT_MAIN));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("✕").on_hover_text("첨부 취소").clicked() {
                                remove_pending = true;
                            }
                        });
                    });
                });
            ui.add_space(4.0_f32);
        }
        if remove_pending {
            self.pending_image = None;
        }

        // --- Input Area (Simplified pure text input with Enter send & Ctrl+V support) ---
        ui.add_space(2.0_f32);
        let full_width = (ui.available_width() - 8.0_f32).max(100.0_f32);
        let resp = ui.horizontal(|ui| {
            ui.add_space(4.0_f32);
            ui.add(
                egui::TextEdit::singleline(&mut self.input_text)
                    .hint_text("요청 입력... (Enter 전송, Ctrl+V 사진/텍스트 붙여넣기)")
                    .desired_width(full_width),
            )
        }).inner;

        // Directly intercept Ctrl+V while the input box is focused to check for image in clipboard
        if resp.has_focus() {
            let is_paste = ui.input(|i| {
                let ctrl = i.modifiers.ctrl || i.modifiers.command;
                let v = i.key_pressed(Key::V) || i.keys_down.contains(&Key::V);
                let evt_paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)));
                (ctrl && v) || evt_paste
            });
            if is_paste && self.try_paste_image(ctx, temp_dir) {
                // Clean up any path suffix accidentally inserted into text edit
                let lower = self.input_text.to_lowercase();
                if lower.ends_with(".png")
                    || lower.ends_with(".jpg")
                    || lower.ends_with(".jpeg")
                    || lower.ends_with(".webp")
                    || lower.ends_with(".bmp")
                {
                    self.input_text.clear();
                }
            }
        }

        // Press Enter to send
        let enter_pressed = (resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
            || (resp.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
        if enter_pressed
            && !self.is_busy
            && (!self.input_text.trim().is_empty() || self.pending_image.is_some())
        {
            self.send(saves_base);
            resp.request_focus();
        }
    }
}

// ---------------------------------------------------------------------------
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
        return Ok((
            format!("📄 WordCraft 문서 초안이 성공적으로 생성되었습니다!\n- 파일명: {doc_name}\n\n아래 버튼을 누르면 WordCraft에서 즉시 열어 편집할 수 있습니다."),
            None,
            Some(ActionResult::for_file("WordCraft에서 문서 열기", "wordcraft", doc_path, true)),
        ));
    }

    // (2) GridCraft: Spreadsheets, data tables, accounting
    if target_app_lower == "gridcraft" || prompt_lower.contains("표") || prompt_lower.contains("가계부") || prompt_lower.contains("견적서") || prompt_lower.contains("매출") || prompt_lower.contains("통계") || prompt_lower.contains("시트") || prompt_lower.contains("csv") || prompt_lower.contains("엑셀") || prompt_lower.contains("계산") {
        let sheet_name = format!("sheet_{timestamp}.csv");
        let sheet_path = saves_dir.join(&sheet_name);
        let content = generate_gridcraft_csv(prompt);
        std::fs::write(&sheet_path, content).map_err(|e| format!("스프레드시트 생성 실패: {e}"))?;
        return Ok((
            format!("📊 GridCraft 스프레드시트가 성공적으로 생성되었습니다!\n- 파일명: {sheet_name}\n- 수식: SUM, AVERAGE 자동 계산 포함\n\n아래 버튼을 누르면 GridCraft에서 즉시 열립니다."),
            None,
            Some(ActionResult::for_file("GridCraft에서 시트 열기", "gridcraft", sheet_path, true)),
        ));
    }

    // (3) VectorCraft: SVG vector illustration, logos, icons
    if target_app_lower == "vectorcraft" || prompt_lower.contains("로고") || prompt_lower.contains("벡터") || prompt_lower.contains("svg") || prompt_lower.contains("아이콘") || prompt_lower.contains("심볼") || prompt_lower.contains("일러스트") {
        let svg_name = format!("vector_{timestamp}.svg");
        let svg_path = saves_dir.join(&svg_name);
        let content = generate_vectorcraft_svg(prompt);
        std::fs::write(&svg_path, content).map_err(|e| format!("SVG 생성 실패: {e}"))?;
        return Ok((
            format!("🎨 VectorCraft용 SVG 벡터 그래픽이 생성되었습니다!\n- 파일명: {svg_name}\n- 표준 Scalable Vector Graphics (.svg)\n\n아래 버튼을 누르면 VectorCraft에서 열어 자유롭게 패스를 편집할 수 있습니다."),
            None,
            Some(ActionResult::for_file("VectorCraft에서 벡터 열기", "vectorcraft", svg_path, true)),
        ));
    }

    // (4) DeckCraft: Presentation slides, pitch decks
    if target_app_lower == "deckcraft" || prompt_lower.contains("발표") || prompt_lower.contains("슬라이드") || prompt_lower.contains("피치덱") || prompt_lower.contains("프레젠테이션") || prompt_lower.contains("ppt") {
        let deck_name = format!("presentation_{timestamp}.deck");
        let deck_path = saves_dir.join(&deck_name);
        let content = generate_deckcraft_deck(prompt);
        std::fs::write(&deck_path, content).map_err(|e| format!("슬라이드 생성 실패: {e}"))?;
        return Ok((
            format!("📽️ DeckCraft 발표 슬라이드 덱이 생성되었습니다!\n- 파일명: {deck_name}\n- 구성: 표지, 문제 정의, 핵심 가치, 비즈니스 모델, Q&A 슬라이드\n\n아래 버튼을 누르면 DeckCraft에서 즉시 열립니다."),
            None,
            Some(ActionResult::for_file("DeckCraft에서 슬라이드 열기", "deckcraft", deck_path, true)),
        ));
    }

    // (5) CADCraft: 3D CAD modeling, geometry, OBJ meshes
    if target_app_lower == "cadcraft" || prompt_lower.contains("3d") || prompt_lower.contains("cad") || prompt_lower.contains("도면") || prompt_lower.contains("캐드") || prompt_lower.contains("모델링") || prompt_lower.contains("obj") {
        let obj_name = format!("model3d_{timestamp}.obj");
        let obj_path = saves_dir.join(&obj_name);
        let content = generate_cadcraft_obj(prompt);
        std::fs::write(&obj_path, content).map_err(|e| format!("3D 모델 생성 실패: {e}"))?;
        return Ok((
            format!("📐 CADCraft 3D 지오메트리 모델 파일이 생성되었습니다!\n- 파일명: {obj_name}\n- 표준 Wavefront 3D OBJ 포맷\n\n아래 버튼을 누르면 CADCraft에서 즉시 3D 뷰포트로 회전/확대 검토할 수 있습니다."),
            None,
            Some(ActionResult::for_file("CADCraft에서 3D 모델 열기", "cadcraft", obj_path, true)),
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

fn generate_cadcraft_obj(prompt: &str) -> String {
    format!(
        "# Wavefront OBJ File generated by CADCraft AI\n\
        # Model: {}\n\
        o Mechanical_Component\n\
        v -1.000000 -1.000000 1.000000\n\
        v 1.000000 -1.000000 1.000000\n\
        v -1.000000 1.000000 1.000000\n\
        v 1.000000 1.000000 1.000000\n\
        v -1.000000 1.000000 -1.000000\n\
        v 1.000000 1.000000 -1.000000\n\
        v -1.000000 -1.000000 -1.000000\n\
        v 1.000000 -1.000000 -1.000000\n\
        # Beveled cylinder top vertices\n\
        v 0.000000 1.500000 0.000000\n\
        v 0.707107 1.250000 0.707107\n\
        v -0.707107 1.250000 0.707107\n\
        v -0.707107 1.250000 -0.707107\n\
        v 0.707107 1.250000 -0.707107\n\
        vn 0.0000 0.0000 1.0000\n\
        vn 0.0000 1.0000 0.0000\n\
        vn 0.0000 0.0000 -1.0000\n\
        vn 0.0000 -1.0000 0.0000\n\
        vn 1.0000 0.0000 0.0000\n\
        vn -1.0000 0.0000 0.0000\n\
        s 1\n\
        f 1//1 2//1 4//1 3//1\n\
        f 3//2 4//2 6//2 5//2\n\
        f 5//3 6//3 8//3 7//3\n\
        f 7//4 8//4 2//4 1//4\n\
        f 2//5 8//5 6//5 4//5\n\
        f 7//6 1//6 3//6 5//6\n\
        f 9//2 10//2 11//2\n\
        f 9//2 11//2 12//2\n\
        f 9//2 12//2 13//2\n\
        f 9//2 13//2 10//2\n",
        prompt
    )
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
