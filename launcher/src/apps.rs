//! App registry, install detection, process management and persisted launcher state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------- registry

#[derive(Deserialize, Clone)]
pub struct Registry {
    pub suite: String,
    pub apps: Vec<AppMeta>,
}

#[derive(Deserialize, Clone)]
pub struct AppMeta {
    pub id: String,
    pub prefix: String,
    pub group: String,
    pub order: u32,
    pub tagline: String,
    #[serde(default)]
    pub tagline_ko: Option<String>,
    pub like: String,
    #[serde(default)]
    pub like_ko: Option<String>,
    #[serde(default)]
    pub desc_ko: Option<String>,
    #[serde(default)]
    pub desc_en: Option<String>,
    pub accent: String,
    pub repo: String,
    #[allow(dead_code)]
    pub website: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Optional explicit executable path relative to `apps/<id>/`.
    #[serde(default)]
    pub exe: Option<String>,
    /// Arguments that start the app's built-in MCP server via `<id>-cli`.
    /// `{saves}` is replaced with the app's SavedFiles folder.
    #[serde(default)]
    pub mcp_args: Vec<String>,
}

impl AppMeta {
    pub fn name(&self) -> String {
        format!("{}Craft", self.prefix)
    }

    pub fn tagline<'a>(&'a self, lang: &str) -> &'a str {
        if lang == "ko" {
            self.tagline_ko.as_deref().unwrap_or(&self.tagline)
        } else {
            &self.tagline
        }
    }

    pub fn like<'a>(&'a self, lang: &str) -> &'a str {
        if lang == "ko" {
            self.like_ko.as_deref().unwrap_or(&self.like)
        } else {
            &self.like
        }
    }

    pub fn description<'a>(&'a self, lang: &str) -> &'a str {
        if lang == "ko" {
            self.desc_ko.as_deref().unwrap_or(self.tagline(lang))
        } else {
            self.desc_en.as_deref().unwrap_or(self.tagline(lang))
        }
    }
}

/// A `registry.json` next to the executable overrides the embedded copy.
pub fn load_registry(exe_dir: &Path) -> Registry {
    if let Ok(s) = std::fs::read_to_string(exe_dir.join("registry.json")) {
        match serde_json::from_str(&s) {
            Ok(r) => return r,
            Err(e) => log(&format!("registry.json next to exe is invalid, using embedded copy: {e}")),
        }
    }
    serde_json::from_str(include_str!("../../registry.json")).expect("embedded registry.json is valid")
}

// ---------------------------------------------------------------- install detection

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NotInstalled,
    Installed,
    Running,
}

pub struct AppEntry {
    pub meta: AppMeta,
    pub dir: PathBuf,
    pub exe: Option<PathBuf>,
    pub version: Option<String>,
    #[allow(dead_code)]
    pub date: Option<String>,
    pub size_bytes: u64,
    pub status: Status,
    pub is_custom_mod: bool,
    pub save_dir: PathBuf,
}

/// Looks for `apps/` (installed layout) or `dist/apps/` (dev layout) walking up from the exe.
pub fn find_apps_dir(exe_dir: &Path) -> PathBuf {
    let mut cur = Some(exe_dir);
    for _ in 0..5 {
        let Some(p) = cur else { break };
        for cand in [p.join("apps"), p.join("dist").join("apps")] {
            if cand.is_dir() {
                return cand;
            }
        }
        cur = p.parent();
    }
    exe_dir.join("apps")
}

/// Base folder where user modified files and projects are saved (`<exe_dir>/SavedFiles`).
pub fn find_saves_dir(exe_dir: &Path) -> PathBuf {
    let saves = exe_dir.join("SavedFiles");
    let _ = std::fs::create_dir_all(&saves);
    saves
}

/// Folder where user can place custom-modified app binaries (`<exe_dir>/custom_apps/<id>/`).
pub fn find_custom_apps_dir(exe_dir: &Path) -> PathBuf {
    let custom = exe_dir.join("custom_apps");
    let _ = std::fs::create_dir_all(&custom);
    custom
}

pub fn scan(reg: &Registry, apps_dir: &Path, exe_dir: &Path, saves_base: &Path) -> Vec<AppEntry> {
    let custom_base = find_custom_apps_dir(exe_dir);

    let mut out: Vec<AppEntry> = reg
        .apps
        .iter()
        .map(|meta| {
            let app_name = meta.name();
            let app_save_dir = saves_base.join(&app_name);
            let _ = std::fs::create_dir_all(&app_save_dir);

            // Check if user has placed a custom modified binary in custom_apps/<id>/
            let custom_app_dir = custom_base.join(&meta.id);
            let custom_exe = detect_exe(&custom_app_dir, &meta.id, None);

            let official_dir = apps_dir.join(&meta.id);
            let official_exe = detect_exe(&official_dir, &meta.id, meta.exe.as_deref());

            let (exe, dir, is_custom_mod) = if let Some(ce) = custom_exe {
                (Some(ce), custom_app_dir, true)
            } else {
                (official_exe, official_dir, false)
            };

            let version_file = dir.join(".version");
            let version = std::fs::read_to_string(&version_file).ok().map(|s| s.trim().to_string());
            let date = std::fs::metadata(&version_file)
                .or_else(|_| exe.as_ref().map(std::fs::metadata).unwrap_or_else(|| std::fs::metadata(&dir)))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| fmt_date(d.as_secs()));
            let size_bytes = if exe.is_some() { dir_size(&dir) } else { 0 };

            AppEntry {
                status: if exe.is_some() { Status::Installed } else { Status::NotInstalled },
                meta: meta.clone(),
                dir,
                exe,
                version,
                date,
                size_bytes,
                is_custom_mod,
                save_dir: app_save_dir,
            }
        })
        .collect();
    out.sort_by_key(|e| e.meta.order);
    out
}

fn detect_exe(dir: &Path, id: &str, hint: Option<&str>) -> Option<PathBuf> {
    if let Some(h) = hint {
        let p = dir.join(h);
        if p.is_file() || (p.is_dir() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))) {
            return Some(p);
        }
    }
    let mut cands = Vec::new();
    collect_exes(dir, 0, &mut cands);
    let want_exe = format!("{id}.exe");
    let want_app = format!("{id}.app");
    if let Some(p) = cands
        .iter()
        .find(|p| {
            p.file_name().is_some_and(|n| {
                let s = n.to_string_lossy();
                s.eq_ignore_ascii_case(&want_exe)
                    || s.eq_ignore_ascii_case(&want_app)
                    || s.eq_ignore_ascii_case(id)
            })
        })
    {
        return Some(p.clone());
    }
    const SKIP: [&str; 6] = ["cli", "uninst", "crash", "helper", "update", "server"];
    cands
        .into_iter()
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            !SKIP.iter().any(|s| n.contains(s))
        })
        .min_by_key(|p| p.components().count())
}

fn collect_exes(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    if depth > 3 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) {
                out.push(p);
            } else {
                collect_exes(&p, depth + 1, out);
            }
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            out.push(p);
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(m) = ent.metadata() {
                    if m.is_file() && (m.permissions().mode() & 0o111 != 0) {
                        out.push(p);
                    }
                }
            }
        }
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

// ---------------------------------------------------------------- processes

#[derive(Default)]
pub struct Runner {
    children: HashMap<String, Vec<Child>>,
}

impl Runner {
    pub fn launch(
        &mut self,
        id: &str,
        exe: &Path,
        save_dir: &Path,
        file: Option<&Path>,
    ) -> std::io::Result<u32> {
        #[cfg(target_os = "macos")]
        if exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) {
            let mut cmd = Command::new("open");
            cmd.arg("-n").arg(exe);
            if let Some(f) = file {
                cmd.arg("--args").arg(f);
            }
            cmd.env("ARTCRAFT_PROJECTS_DIR", save_dir);
            cmd.env("ARTCRAFT_SAVES_DIR", save_dir);
            let child = cmd.spawn()?;
            let pid = child.id();
            self.children.entry(id.to_string()).or_default().push(child);
            return Ok(pid);
        }

        let mut cmd = Command::new(exe);

        // Sets current working directory to the user's SavedFiles/<AppName> folder
        // so open/save dialogs default directly to the exe's subfolder!
        if save_dir.is_dir() {
            cmd.current_dir(save_dir);
        } else if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }

        // Export environment variables pointing to the dedicated save folders
        cmd.env("ARTCRAFT_PROJECTS_DIR", save_dir);
        cmd.env("ARTCRAFT_SAVES_DIR", save_dir);

        if let Some(f) = file {
            cmd.arg(f);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }
        let child = cmd.spawn()?;
        let pid = child.id();
        self.children.entry(id.to_string()).or_default().push(child);
        Ok(pid)
    }

    /// Drops children that have exited.
    pub fn poll(&mut self) {
        for list in self.children.values_mut() {
            list.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        }
        self.children.retain(|_, v| !v.is_empty());
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.children.contains_key(id)
    }

    pub fn pids(&self, id: &str) -> Vec<u32> {
        self.children.get(id).map(|v| v.iter().map(Child::id).collect()).unwrap_or_default()
    }
}

/// Brings the first visible top-level window owned by one of `pids` to the front.
#[cfg(windows)]
#[allow(unsafe_code, dead_code)]
pub fn focus_process(pids: &[u32]) -> bool {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
        ShowWindow, GW_OWNER, SW_RESTORE,
    };

    struct Ctx {
        pids: Vec<u32>,
        found: HWND,
    }

    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp as *mut Ctx);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if ctx.pids.contains(&pid) && IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() {
            ctx.found = hwnd;
            return 0;
        }
        1
    }

    let mut ctx = Ctx { pids: pids.to_vec(), found: std::ptr::null_mut() };
    unsafe {
        EnumWindows(Some(cb), &mut ctx as *mut Ctx as LPARAM);
        if ctx.found.is_null() {
            return false;
        }
        if IsIconic(ctx.found) != 0 {
            ShowWindow(ctx.found, SW_RESTORE);
        }
        SetForegroundWindow(ctx.found) != 0
    }
}

#[cfg(not(windows))]
pub fn focus_process(_pids: &[u32]) -> bool {
    false
}

// ---------------------------------------------------------------- state & logging

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct State {
    #[serde(default)]
    pub last_opened: HashMap<String, u64>,
    #[serde(default)]
    pub table_view: bool,
    /// 0 = suite order, 1 = name, 2 = recently opened
    #[serde(default)]
    pub sort: u8,
    #[serde(default)]
    pub minimize_on_open: bool,
    #[serde(default = "default_lang")]
    pub language: String, // "ko" or "en"
    #[serde(default)]
    pub custom_saves_dir: Option<String>,
}

fn default_lang() -> String {
    "ko".to_string()
}

impl Default for State {
    fn default() -> Self {
        Self {
            last_opened: HashMap::new(),
            table_view: false,
            sort: 0,
            minimize_on_open: false,
            language: "ko".to_string(),
            custom_saves_dir: None,
        }
    }
}

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("ArtCraftStudio")
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|h| h.join("Library/Application Support/ArtCraftStudio"))
            .unwrap_or_else(|| std::env::temp_dir().join("ArtCraftStudio"))
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|h| h.join(".local/share"))
                    .unwrap_or_else(std::env::temp_dir)
            })
            .join("ArtCraftStudio")
    }
}

impl State {
    pub fn load() -> Self {
        std::fs::read_to_string(data_dir().join("state.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = data_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join("state.json"), s);
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn log_path() -> PathBuf {
    data_dir().join("logs").join("launcher.log")
}

pub fn log(msg: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{}] {msg}", now_secs());
    }
}

// ---------------------------------------------------------------- formatting

/// Unix seconds -> "Oct 14, 2026" (UTC, no external date crate).
pub fn fmt_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {}, {}", M[(m - 1) as usize], d, y)
}

pub fn fmt_size(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.0} MB", b / 1e6)
    } else {
        format!("{:.0} KB", b / 1e3)
    }
}
