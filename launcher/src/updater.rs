//! In-place replacement updater module (3M-style in-place app replacement)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Creates a completely silent background process with no console window popping up
fn silent_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Fetches response body from a URL using curl, automatically stripping UTF-8 BOM
fn fetch_url_string(url: &str) -> Option<String> {
    let prog = if cfg!(windows) { "curl.exe" } else { "curl" };
    let output = silent_command(prog)
        .args(["-k", "-L", "-s", "--max-time", "6", "-A", "ArtCraftStudio", url])
        .output()
        .ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let cleaned = text.trim_start_matches('\u{feff}').trim().to_string();
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }
    None
}

pub const UPDATE_SCRIPT_PS1: &str = include_str!("../../scripts/update-app.ps1");
#[allow(dead_code)]
pub const UPDATE_SCRIPT_SH: &str = include_str!("../../scripts/update-app.sh");

pub struct UpdateManager {
    _exe_dir: PathBuf,
    sender: Sender<(String, bool, String)>,
    receiver: Receiver<(String, bool, String)>,
    in_progress: Arc<Mutex<HashSet<String>>>,
    latest_versions: Arc<Mutex<HashMap<String, String>>>,
    custom_sources: Arc<Mutex<HashMap<String, String>>>,
    progress: Arc<Mutex<HashMap<String, (f32, String)>>>,
    last_results: Arc<Mutex<HashMap<String, (bool, String)>>>,
    total_targets: Arc<Mutex<usize>>,
    completed_count: Arc<Mutex<usize>>,
    pub is_checking: bool,
}

/// Compares two version strings: returns true if `remote` is strictly newer than `local`
pub fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect()
    };
    let r = parse(remote);
    let l = parse(local);
    for (rv, lv) in r.iter().zip(l.iter()) {
        if rv > lv {
            return true;
        } else if rv < lv {
            return false;
        }
    }
    r.len() > l.len()
}

impl UpdateManager {
    pub fn new(exe_dir: PathBuf) -> Self {
        let (sender, receiver) = channel();
        let initial_versions = Self::read_local_manifest(&exe_dir);
        Self {
            _exe_dir: exe_dir,
            sender,
            receiver,
            in_progress: Arc::new(Mutex::new(HashSet::new())),
            latest_versions: Arc::new(Mutex::new(initial_versions)),
            custom_sources: Arc::new(Mutex::new(HashMap::new())),
            progress: Arc::new(Mutex::new(HashMap::new())),
            last_results: Arc::new(Mutex::new(HashMap::new())),
            total_targets: Arc::new(Mutex::new(1)),
            completed_count: Arc::new(Mutex::new(0)),
            is_checking: false,
        }
    }

    pub fn start_batch(&self, count: usize) {
        if let Ok(mut t) = self.total_targets.lock() {
            *t = count.max(1);
        }
        if let Ok(mut c) = self.completed_count.lock() {
            *c = 0;
        }
        if let Ok(mut m) = self.last_results.lock() {
            m.clear();
        }
    }

    pub fn custom_source(&self, app_id: &str) -> Option<String> {
        self.custom_sources.lock().ok().and_then(|m| m.get(app_id).cloned())
    }

    #[allow(dead_code)]
    pub fn last_result(&self, app_id: &str) -> Option<(bool, String)> {
        self.last_results.lock().ok().and_then(|m| m.get(app_id).cloned())
    }

    pub fn overall_result(&self) -> Option<(bool, String)> {
        if let Ok(m) = self.last_results.lock() {
            if m.is_empty() {
                return None;
            }
            if let Some((_, (false, err_msg))) = m.iter().find(|(_, (succ, _))| !*succ) {
                return Some((false, err_msg.clone()));
            }
            if let Some((_, (true, msg))) = m.iter().next() {
                return Some((true, msg.clone()));
            }
        }
        None
    }

    pub fn clear_results(&self) {
        if let Ok(mut m) = self.last_results.lock() {
            m.clear();
        }
        if let Ok(mut c) = self.completed_count.lock() {
            *c = 0;
        }
    }

    pub fn get_progress(&self, app_id: &str) -> Option<(f32, String)> {
        self.progress.lock().ok().and_then(|m| m.get(app_id).cloned())
    }

    pub fn overall_progress(&self) -> Option<(f32, String)> {
        let total = self.total_targets.lock().ok().map(|t| *t).unwrap_or(1).max(1);
        let completed = self.completed_count.lock().ok().map(|c| *c).unwrap_or(0);

        if let Ok(m) = self.progress.lock() {
            if m.is_empty() {
                if completed >= total {
                    return Some((1.0, "업데이트 완료".to_string()));
                }
                return None;
            }
            let active_sum: f32 = m.values().map(|(p, _)| *p).sum();
            let effective = ((completed as f32 + active_sum) / total as f32).clamp(0.01, 1.0);
            let first_desc = m.values().next().map(|(_, d)| d.clone()).unwrap_or_else(|| "업데이트 처리 중...".to_string());
            let desc = if total > 1 {
                let cur_idx = (completed + 1).min(total);
                format!("[{cur_idx}/{total}] {first_desc}")
            } else {
                first_desc
            };
            Some((effective, desc))
        } else {
            None
        }
    }

    fn read_local_manifest(exe_dir: &Path) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let candidates = [
            exe_dir.join("version.json"),
            exe_dir.join("../version.json"),
            PathBuf::from("version.json"),
        ];
        for path in &candidates {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(apps) = v.get("apps").and_then(|a| a.as_object()) {
                            for (id, app_info) in apps {
                                if let Some(ver) = app_info.get("version").and_then(|s| s.as_str()) {
                                    map.insert(id.clone(), ver.trim_start_matches('v').to_string());
                                }
                            }
                            if !map.is_empty() {
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Also check actual installed .version files inside apps/ and dist/apps/
        let app_roots = [exe_dir.join("apps"), exe_dir.join("dist/apps"), PathBuf::from("dist/apps")];
        for root in &app_roots {
            if let Ok(rd) = std::fs::read_dir(root) {
                for ent in rd.flatten() {
                    let ver_file = ent.path().join(".version");
                    if let Ok(content) = std::fs::read_to_string(ver_file) {
                        let v = content.trim().trim_start_matches('v').to_string();
                        let id = ent.file_name().to_string_lossy().to_string();
                        if let Some(existing) = map.get(&id) {
                            if is_newer(&v, existing) {
                                map.insert(id, v);
                            }
                        } else {
                            map.insert(id, v);
                        }
                    }
                }
            }
        }

        map
    }

    pub fn is_updating(&self, app_id: &str) -> bool {
        self.in_progress.lock().map(|set| set.contains(app_id)).unwrap_or(false)
    }

    pub fn has_active_updates(&self) -> bool {
        self.in_progress.lock().map(|set| !set.is_empty()).unwrap_or(false)
    }

    pub fn latest_version(&self, app_id: &str) -> Option<String> {
        self.latest_versions.lock().ok().and_then(|map| map.get(app_id).cloned())
    }

    pub fn mark_updated(&self, app_id: &str, current_ver: &str) {
        if let Ok(mut map) = self.latest_versions.lock() {
            map.insert(app_id.to_string(), current_ver.to_string());
        }
    }

    /// Checks GitHub Releases & version.json for all apps in a background thread
    pub fn check_all_updates(&mut self, repos: Vec<(String, String)>) {
        if self.is_checking {
            return;
        }
        self.is_checking = true;
        let latest_store = Arc::clone(&self.latest_versions);
        let custom_sources = Arc::clone(&self.custom_sources);
        let tx = self.sender.clone();
        let exe_dir = self._exe_dir.clone();

        thread::spawn(move || {
            let mut found = 0;
            let suite_repo = "gochujangs33-dot/Craft-Studio";
            let user_reg = crate::apps::get_user_registry_path(&exe_dir);

            // Tier 0: Direct app-specific and custom repository check (e.g. user's own GitHub releases)
            for (id, repo_url) in &repos {
                let repo_path = repo_url
                    .trim_start_matches("https://github.com/")
                    .trim_start_matches("http://github.com/")
                    .trim_matches('/');
                if repo_path.is_empty() || repo_path.eq_ignore_ascii_case(suite_repo) {
                    continue;
                }
                let rel_url = format!("https://api.github.com/repos/{repo_path}/releases/latest");
                if let Some(rel_json) = fetch_url_string(&rel_url) {
                    if let Ok(rel) = serde_json::from_str::<serde_json::Value>(&rel_json) {
                        if let Some(tag) = rel.get("tag_name").and_then(|t| t.as_str()) {
                            let clean_ver = tag.trim_start_matches('v').to_string();
                            if clean_ver.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                if let Ok(mut map) = latest_store.lock() {
                                    map.insert(id.clone(), clean_ver.clone());
                                }
                                if let Ok(mut sources) = custom_sources.lock() {
                                    sources.insert(id.clone(), repo_path.to_string());
                                }
                                found += 1;
                            }
                        }
                    }
                }
            }

            // Tier 1: Sync remote registry.json so new cards appear automatically
            let reg_url = format!("https://raw.githubusercontent.com/{suite_repo}/main/registry.json");
            if let Some(reg_json) = fetch_url_string(&reg_url) {
                if reg_json.starts_with('{') {
                    if let Some(parent) = user_reg.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&user_reg, reg_json);
                }
            }

            // Tier 2: Check remote version.json manifest
            let ver_url = format!("https://raw.githubusercontent.com/{suite_repo}/main/version.json");
            if let Some(ver_json) = fetch_url_string(&ver_url) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&ver_json) {
                    if let Some(apps) = v.get("apps").and_then(|a| a.as_object()) {
                        if let Ok(mut map) = latest_store.lock() {
                            for (id, val) in apps {
                                // If already customized from direct repo, don't overwrite with older manifest
                                if let Ok(sources) = custom_sources.lock() {
                                    if sources.contains_key(id) {
                                        continue;
                                    }
                                }
                                if let Some(ver) = val.get("version").and_then(|v| v.as_str()) {
                                    let clean_ver = ver.trim_start_matches('v').to_string();
                                    if let Some(existing) = map.get(id) {
                                        if is_newer(&clean_ver, existing) {
                                            map.insert(id.clone(), clean_ver);
                                        }
                                    } else {
                                        map.insert(id.clone(), clean_ver);
                                    }
                                    found += 1;
                                }
                            }
                        }
                    }
                }
            }

            // Tier 3: Check GitHub Releases latest assets if version.json yielded few results
            if found < 5 {
                let rel_url = format!("https://api.github.com/repos/{suite_repo}/releases/latest");
                if let Some(rel_json) = fetch_url_string(&rel_url) {
                    if let Ok(rel) = serde_json::from_str::<serde_json::Value>(&rel_json) {
                        if let Some(assets) = rel.get("assets").and_then(|a| a.as_array()) {
                            if let Ok(mut map) = latest_store.lock() {
                                for a in assets {
                                    if let Some(name) = a.get("name").and_then(|n| n.as_str()) {
                                        let lower = name.to_lowercase();
                                        let parts: Vec<&str> = lower.split('-').collect();
                                        if parts.len() >= 2 {
                                            let id = parts[0].to_string();
                                            let ver = parts[1].trim_start_matches('v').to_string();
                                            if ver.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                                                if let Some(existing) = map.get(&id) {
                                                    if is_newer(&ver, existing) {
                                                        map.insert(id, ver);
                                                    }
                                                } else {
                                                    map.insert(id, ver);
                                                }
                                                found += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let success = found > 0;
            let _ = tx.send(("__check_all__".to_string(), success, format!("{found}개 프로그램 최신 정보 매칭 완료")));
        });
    }

    /// Triggers an in-place replacement update for a specific app
    pub fn update_app(&self, app_id: &str, repo_url: &str, dest_dir: &Path) -> bool {
        if self.is_updating(app_id) {
            return false;
        }

        // Add to in-progress set
        if let Ok(mut set) = self.in_progress.lock() {
            set.insert(app_id.to_string());
        }

        let app_id = app_id.to_string();
        let repo_url = repo_url.to_string();
        let dest_dir = dest_dir.to_path_buf();
        let in_progress = Arc::clone(&self.in_progress);
        let progress_store = Arc::clone(&self.progress);
        let latest_store = Arc::clone(&self.latest_versions);
        let last_results = Arc::clone(&self.last_results);
        let completed_count = Arc::clone(&self.completed_count);
        let tx = self.sender.clone();

        // Clear previous result for this app
        if let Ok(mut map) = last_results.lock() {
            map.remove(&app_id);
        }

        // Initialize progress
        if let Ok(mut map) = progress_store.lock() {
            map.insert(app_id.clone(), (0.05, "업데이트 준비 중...".to_string()));
        }

        // Ensure update script exists on disk
        #[cfg(windows)]
        let script_path = std::env::temp_dir().join("artcraft_update_app.ps1");
        #[cfg(windows)]
        let _ = std::fs::write(&script_path, UPDATE_SCRIPT_PS1);

        #[cfg(not(windows))]
        let script_path = std::env::temp_dir().join("artcraft_update_app.sh");
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::write(&script_path, UPDATE_SCRIPT_SH);
            let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
        }

        thread::spawn(move || {
            let script_str = script_path.display().to_string();
            let dest_str = dest_dir.display().to_string();

            #[cfg(windows)]
            let child = silent_command("powershell")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &script_str,
                    "-AppId",
                    &app_id,
                    "-RepoUrl",
                    &repo_url,
                    "-DestDir",
                    &dest_str,
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            #[cfg(not(windows))]
            let child = silent_command("sh")
                .args([
                    &script_str,
                    &app_id,
                    &repo_url,
                    &dest_str,
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            let mut last_status = None;
            let mut err_out = String::new();
            if let Ok(mut proc) = child {
                let stderr_handle = proc.stderr.take();
                let stderr_thread = std::thread::spawn(move || {
                    let mut s = String::new();
                    if let Some(mut err_pipe) = stderr_handle {
                        use std::io::Read;
                        let _ = err_pipe.read_to_string(&mut s);
                    }
                    s
                });

                if let Some(stdout) = proc.stdout.take() {
                    use std::io::{BufRead, BufReader};
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().flatten() {
                        if let Some(rest) = line.strip_prefix("PROGRESS:") {
                            if let Some((p_str, desc)) = rest.split_once(':') {
                                if let Ok(p_val) = p_str.parse::<f32>() {
                                    let norm = (p_val / 100.0).clamp(0.0, 1.0);
                                    if let Ok(mut map) = progress_store.lock() {
                                        map.insert(app_id.clone(), (norm, desc.trim().to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
                last_status = proc.wait().ok();
                err_out = stderr_thread.join().unwrap_or_default();
            }

            let (success, msg) = match last_status {
                Some(s) if s.success() => {
                    let ver_file = dest_dir.join(".version");
                    let mut ver_str = String::new();
                    if let Ok(v) = std::fs::read_to_string(&ver_file) {
                        ver_str = v.trim().to_string();
                    }
                    if ver_str.is_empty() {
                        if let Ok(map) = latest_store.lock() {
                            if let Some(l) = map.get(&app_id) {
                                ver_str = l.clone();
                                let _ = std::fs::write(&ver_file, &ver_str);
                            }
                        }
                    } else if let Ok(mut map) = latest_store.lock() {
                        map.insert(app_id.clone(), ver_str.clone());
                    }
                    (true, format!("{app_id} 최신 버전(v{ver_str}) 업데이트 완료"))
                }
                Some(s) => {
                    let err_detail = if !err_out.trim().is_empty() {
                        format!(": {}", err_out.trim())
                    } else {
                        String::new()
                    };
                    (false, format!("{app_id} 업데이트 실패 (코드: {:?}{err_detail})", s.code()))
                }
                None => (false, format!("{app_id} 업데이트 프로세스 실행 실패")),
            };

            if success {
                if let Ok(mut c) = completed_count.lock() {
                    *c += 1;
                }
            }

            if let Ok(mut set) = in_progress.lock() {
                set.remove(&app_id);
            }
            if let Ok(mut map) = progress_store.lock() {
                map.remove(&app_id);
            }
            if let Ok(mut map) = last_results.lock() {
                map.insert(app_id.clone(), (success, msg.clone()));
            }

            let _ = tx.send((app_id, success, msg));
        });

        true
    }

    /// Polls completed update events: returns vec of (app_id, success, message)
    pub fn poll_events(&mut self) -> Vec<(String, bool, String)> {
        let mut events = Vec::new();
        while let Ok(ev) = self.receiver.try_recv() {
            if ev.0 == "__check_all__" {
                self.is_checking = false;
            }
            events.push(ev);
        }
        events
    }
}
