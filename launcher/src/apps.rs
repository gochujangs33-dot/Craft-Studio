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

/// Returns the user-writable path for cached/synced dynamic registry.json
pub fn get_user_registry_path(exe_dir: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let p = PathBuf::from(home).join("Library/Application Support/ArtCraftStudio");
            let _ = std::fs::create_dir_all(&p);
            return p.join("registry.json");
        }
    }
    let saves = exe_dir.join("SavedFiles");
    let _ = std::fs::create_dir_all(&saves);
    saves.join("registry.json")
}

/// Loads app registry. Dynamic synchronized registry overrides embedded copy.
pub fn load_registry(exe_dir: &Path) -> Registry {
    // 1. Check user application support directory (synced from online)
    let user_reg = get_user_registry_path(exe_dir);
    if let Ok(s) = std::fs::read_to_string(&user_reg) {
        if let Ok(r) = serde_json::from_str::<Registry>(&s) {
            return r;
        }
    }
    // 2. Check next to executable
    if let Ok(s) = std::fs::read_to_string(exe_dir.join("registry.json")) {
        match serde_json::from_str(&s) {
            Ok(r) => return r,
            Err(e) => log(&format!("registry.json next to exe is invalid, using embedded copy: {e}")),
        }
    }
    // 3. Fallback to embedded copy
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

/// Helper to test if a directory is writable
pub fn is_dir_writable(dir: &Path) -> bool {
    let test_dir = if dir.exists() {
        dir.to_path_buf()
    } else {
        dir.parent().unwrap_or(dir).to_path_buf()
    };
    if std::fs::create_dir_all(&test_dir).is_ok() {
        let test_file = test_dir.join(".artcraft_write_test");
        if std::fs::write(&test_file, b"ok").is_ok() {
            let _ = std::fs::remove_file(&test_file);
            return true;
        }
    }
    false
}

/// Returns a writable destination directory for downloading/installing an app
pub fn get_writable_app_dir(apps_dir: &Path, meta_id: &str) -> PathBuf {
    let target = apps_dir.join(meta_id);
    if is_dir_writable(&target) || is_dir_writable(apps_dir) {
        return target;
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let user_dir = PathBuf::from(home)
                .join("Library/Application Support/ArtCraftStudio/apps")
                .join(meta_id);
            let _ = std::fs::create_dir_all(&user_dir);
            return user_dir;
        }
    }
    target
}

/// Looks for `apps/` (installed layout) or `dist/apps/` (dev layout) walking up from the exe.
pub fn find_apps_dir(exe_dir: &Path) -> PathBuf {
    let mut cur = Some(exe_dir);
    for _ in 0..5 {
        let Some(p) = cur else { break };
        for cand in [
            p.join("apps"),
            p.join("dist").join("apps"),
            p.join("Resources").join("apps"),
            p.join("Contents").join("Resources").join("apps"),
        ] {
            if cand.is_dir() {
                return cand;
            }
        }
        cur = p.parent();
    }

    #[cfg(target_os = "macos")]
    {
        // Check user application support directory
        if let Some(home) = std::env::var_os("HOME") {
            let app_sup = PathBuf::from(home).join("Library/Application Support/ArtCraftStudio/apps");
            if app_sup.is_dir() {
                return app_sup;
            }
        }
        // If inside an .app bundle (Contents/MacOS), check Contents/Resources/apps
        let res_apps = exe_dir.join("../Resources/apps");
        if res_apps.is_dir() {
            return res_apps;
        }
    }

    exe_dir.join("apps")
}

/// Base folder where user modified files and projects are saved (`<exe_dir>/SavedFiles`).
pub fn find_saves_dir(exe_dir: &Path) -> PathBuf {
    let saves = exe_dir.join("SavedFiles");
    if is_dir_writable(&saves) {
        let _ = std::fs::create_dir_all(&saves);
        return saves;
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let user_saves = PathBuf::from(home).join("Documents/ArtCraftStudio/SavedFiles");
            let _ = std::fs::create_dir_all(&user_saves);
            return user_saves;
        }
    }
    let _ = std::fs::create_dir_all(&saves);
    saves
}

/// Folder where user can place custom-modified app binaries (`<exe_dir>/custom_apps/<id>/`).
pub fn find_custom_apps_dir(exe_dir: &Path) -> PathBuf {
    let custom = exe_dir.join("custom_apps");
    if is_dir_writable(&custom) {
        let _ = std::fs::create_dir_all(&custom);
        return custom;
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let user_custom = PathBuf::from(home).join("Library/Application Support/ArtCraftStudio/custom_apps");
            let _ = std::fs::create_dir_all(&user_custom);
            return user_custom;
        }
    }
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

            // 1. Check custom modified binary in custom_apps/<id>/
            let custom_app_dir = custom_base.join(&meta.id);
            let custom_exe = detect_exe(&custom_app_dir, &meta.id, None);

            #[cfg(target_os = "macos")]
            let mut candidates = Vec::new();
            #[cfg(not(target_os = "macos"))]
            #[allow(unused_mut)]
            let mut candidates = vec![
                apps_dir.join(&meta.id),
            ];

            #[cfg(target_os = "macos")]
            {
                // 1. User Application Support directory (updated/downloaded versions take priority!)
                if let Some(home) = std::env::var_os("HOME") {
                    candidates.push(PathBuf::from(home).join("Library/Application Support/ArtCraftStudio/apps").join(&meta.id));
                }

                // 2. Portable apps folder next to .app bundle
                if let Some(parent) = exe_dir.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
                    candidates.push(parent.join("apps").join(&meta.id));
                }

                // 3. apps_dir next to executable
                candidates.push(apps_dir.join(&meta.id));

                // 4. Resources inside .app bundle (factory pre-installed base versions)
                candidates.push(exe_dir.join("../Resources/apps").join(&meta.id));
                candidates.push(exe_dir.join("../../Resources/apps").join(&meta.id));

                // 5. Standard macOS /Applications folders (e.g. /Applications/HOP.app, /Applications/hop.app)
                for app_base in ["/Applications", "/System/Applications"] {
                    let base = PathBuf::from(app_base);
                    candidates.push(base.join(&meta.id));
                    candidates.push(base.join(format!("{}.app", meta.id)));
                    candidates.push(base.join(format!("{}.app", meta.id.to_uppercase())));
                    candidates.push(base.join(format!("{}.app", meta.name())));
                    candidates.push(base.join(format!("{}.app", meta.prefix)));
                }
                if let Some(home) = std::env::var_os("HOME") {
                    let uapps = PathBuf::from(home).join("Applications");
                    candidates.push(uapps.join(&meta.id));
                    candidates.push(uapps.join(format!("{}.app", meta.id)));
                    candidates.push(uapps.join(format!("{}.app", meta.id.to_uppercase())));
                    candidates.push(uapps.join(format!("{}.app", meta.name())));
                    candidates.push(uapps.join(format!("{}.app", meta.prefix)));
                }
            }

            let mut official_exe = None;
            let mut official_dir = apps_dir.join(&meta.id);

            for cand in candidates {
                if let Some(found_exe) = detect_exe(&cand, &meta.id, meta.exe.as_deref()) {
                    official_exe = Some(found_exe);
                    official_dir = cand;
                    break;
                }
            }

            // Ensure destination directory for updating/installing is writable
            if !is_dir_writable(&official_dir) {
                official_dir = get_writable_app_dir(apps_dir, &meta.id);
            }

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
    const SKIP: [&str; 8] = ["cli", "uninst", "crash", "helper", "update", "server", "setup", "install"];

    // If the path itself is already an executable or .app bundle
    if dir.is_dir() && dir.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) {
        return Some(dir.to_path_buf());
    }
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

    // 1. Exact name match (highest priority: wordcraft.exe, wordcraft.app, wordcraft)
    if let Some(p) = cands.iter().find(|p| {
        p.file_name().is_some_and(|n| {
            let s = n.to_string_lossy();
            s.eq_ignore_ascii_case(&want_exe)
                || s.eq_ignore_ascii_case(&want_app)
                || s.eq_ignore_ascii_case(id)
        })
    }) {
        return Some(p.clone());
    }

    // Filter out helper/cli/uninstaller binaries
    let filtered: Vec<_> = cands
        .into_iter()
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            !SKIP.iter().any(|s| n.contains(s))
        })
        .collect();

    // 2. Exact startswith on filtered non-cli binaries
    if let Some(p) = filtered.iter().find(|p| {
        p.file_name().is_some_and(|n| {
            n.to_string_lossy().to_lowercase().starts_with(&id.to_lowercase())
        })
    }) {
        return Some(p.clone());
    }

    // 3. On macOS, any .app bundle that wasn't skipped
    #[cfg(target_os = "macos")]
    if let Some(app) = filtered.iter().find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))) {
        return Some(app.clone());
    }

    // 4. Fallback to shallowest non-skipped executable
    filtered.into_iter().min_by_key(|p| p.components().count())
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
                    if m.is_file() {
                        let is_exec = m.permissions().mode() & 0o111 != 0;
                        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
                        let is_doc = matches!(ext.as_str(), "txt" | "md" | "json" | "png" | "ico" | "icns" | "plist" | "log" | "version");
                        if is_exec || !is_doc {
                            if !is_exec {
                                let mut perms = m.permissions();
                                perms.set_mode(perms.mode() | 0o755);
                                let _ = std::fs::set_permissions(&p, perms);
                            }
                            out.push(p);
                        }
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

pub const COMMON_CONTROL_TOKEN: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Returns the assigned control port for Craft applications.
/// PhotoCraft is 49152, VectorCraft is 49153, FilmCraft is 49154, ... HopCraft is 49165.
pub fn get_app_control_port(id: &str) -> Option<u16> {
    match id.to_ascii_lowercase().as_str() {
        "photocraft" => Some(49152),
        "vectorcraft" => Some(49153),
        "filmcraft" => Some(49154),
        "lightcraft" => Some(49155),
        "pdfcraft" => Some(49156),
        "effectcraft" => Some(49157),
        "designcraft" => Some(49158),
        "wordcraft" => Some(49159),
        "deckcraft" => Some(49160),
        "gridcraft" => Some(49161),
        "cadcraft" => Some(49162),
        "soundcraft" => Some(49163),
        "figcraft" => Some(49164),
        "hop" => Some(49165),
        _ => None,
    }
}

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
        {
            // Remove quarantine attribute recursively so Gatekeeper won't block downloaded apps
            let _ = Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(exe).status();
        }

        #[cfg(target_os = "macos")]
        if exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) {
            let mut cmd = Command::new("open");
            cmd.arg("-n").arg(exe);
            let mut subargs = Vec::new();
            if let Some(port) = get_app_control_port(id) {
                subargs.push("--control".to_string());
                subargs.push(port.to_string());
            }
            if let Some(f) = file {
                subargs.push(f.to_string_lossy().to_string());
            }
            if !subargs.is_empty() {
                cmd.arg("--args");
                for a in subargs {
                    cmd.arg(a);
                }
            }
            cmd.env("ARTCRAFT_PROJECTS_DIR", save_dir);
            cmd.env("ARTCRAFT_SAVES_DIR", save_dir);
            if let Some(port) = get_app_control_port(id) {
                let port_str = port.to_string();
                let upper = id.to_uppercase();
                cmd.env(format!("{upper}_CONTROL_PORT"), &port_str);
                cmd.env(format!("{upper}_CONTROL_TOKEN"), COMMON_CONTROL_TOKEN);
                cmd.env("ARTCRAFT_CONTROL_PORT", &port_str);
                cmd.env("ARTCRAFT_CONTROL_TOKEN", COMMON_CONTROL_TOKEN);
            }
            let child = cmd.spawn()?;
            let pid = child.id();
            self.children.entry(id.to_string()).or_default().push(child);
            return Ok(pid);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = exe.metadata() {
                let mut perms = meta.permissions();
                if perms.mode() & 0o111 == 0 {
                    perms.set_mode(perms.mode() | 0o755);
                    let _ = std::fs::set_permissions(exe, perms);
                }
            }
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

        // Control port, token, and environment variables for all Craft apps (including hop)
        if let Some(port) = get_app_control_port(id) {
            // Pass `--control <port>` argument so the app starts its live RPC control server
            cmd.arg("--control");
            cmd.arg(port.to_string());

            let port_str = port.to_string();
            let upper = id.to_uppercase();

            // 1. App-specific control port and token (e.g. PHOTOCRAFT_CONTROL_PORT)
            cmd.env(format!("{upper}_CONTROL_PORT"), &port_str);
            cmd.env(format!("{upper}_CONTROL_TOKEN"), COMMON_CONTROL_TOKEN);

            // 2. Generic ArtCraft suite control environment variables
            cmd.env("ARTCRAFT_CONTROL_PORT", &port_str);
            cmd.env("ARTCRAFT_CONTROL_TOKEN", COMMON_CONTROL_TOKEN);

            // 3. Automation read & write sandboxes
            let root = std::env::var("SystemDrive")
                .map(|d| format!("{d}/"))
                .unwrap_or_else(|_| if cfg!(windows) { "C:/".to_string() } else { "/".to_string() });
            cmd.env(format!("{upper}_AUTOMATION_READ_ROOT"), &root);
            cmd.env(format!("{upper}_AUTOMATION_WRITE_ROOT"), &root);
            cmd.env("ARTCRAFT_AUTOMATION_READ_ROOT", &root);
            cmd.env("ARTCRAFT_AUTOMATION_WRITE_ROOT", &root);
        }

        if let Some(f) = file {
            cmd.arg(f);
        }
        cmd.stdin(std::process::Stdio::null());
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

    /// Drops children that have exited and logs exit status.
    pub fn poll(&mut self) {
        for (id, list) in &mut self.children {
            list.retain_mut(|c| match c.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => {
                    log(&format!("app {id} (pid {}) exited: {status:?}", c.id()));
                    false
                }
                Err(e) => {
                    log(&format!("app {id} (pid {}) wait error: {e}", c.id()));
                    false
                }
            });
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

/// Returns the [left, top, right, bottom] screen rect of a running app window's client area.
/// Matches either by child process PID or by top-level window title.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn get_app_window_rect(pids: &[u32], app_name: &str) -> Option<[f32; 4]> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClientRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible,
    };

    struct Ctx {
        pids: Vec<u32>,
        name: String,
        best_area: i64,
        rect: Option<[f32; 4]>,
    }

    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let ctx = &mut *(lp as *mut Ctx);
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);

        let mut matched = !ctx.pids.is_empty() && ctx.pids.contains(&pid);
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let title = if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        };

        if !matched && !ctx.name.is_empty() && !title.is_empty() {
            if title.to_lowercase().contains(&ctx.name.to_lowercase()) {
                matched = true;
            }
        }

        if matched {
            let mut client_rc: RECT = std::mem::zeroed();
            if GetClientRect(hwnd, &mut client_rc) != 0 {
                let raw_w = client_rc.right - client_rc.left;
                let raw_h = client_rc.bottom - client_rc.top;
                // Only consider substantial application windows (exclude tooltips/assistants)
                if raw_w >= 300 && raw_h >= 200 {
                    let area = (raw_w as i64) * (raw_h as i64);
                    if area > ctx.best_area {
                        ctx.best_area = area;
                        let mut pt = POINT { x: 0, y: 0 };
                        ClientToScreen(hwnd, &mut pt);
                        let left = pt.x as f32;
                        let top = pt.y as f32;
                        let right = (pt.x + raw_w) as f32;
                        let bottom = (pt.y + raw_h) as f32;
                        ctx.rect = Some([left, top, right, bottom]);
                    }
                }
            }
        }
        1
    }

    let mut ctx = Ctx {
        pids: pids.to_vec(),
        name: app_name.to_string(),
        best_area: 0,
        rect: None,
    };
    unsafe {
        EnumWindows(Some(cb), &mut ctx as *mut Ctx as LPARAM);
    }
    ctx.rect
}

#[cfg(not(windows))]
pub fn get_app_window_rect(_pids: &[u32], _app_name: &str) -> Option<[f32; 4]> {
    None
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
    #[serde(default = "default_true")]
    pub auto_show_companion: bool,
    #[serde(default)]
    pub gemini_api_key: String,
    #[serde(default)]
    pub ai_provider: u8,
}

fn default_true() -> bool {
    true
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
            auto_show_companion: true,
            gemini_api_key: String::new(),
            ai_provider: 0,
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
