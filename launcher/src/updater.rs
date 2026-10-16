//! In-place replacement updater module (3M-style in-place app replacement)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

pub const UPDATE_SCRIPT: &str = include_str!("../../scripts/update-app.ps1");

pub struct UpdateManager {
    _exe_dir: PathBuf,
    sender: Sender<(String, bool, String)>,
    receiver: Receiver<(String, bool, String)>,
    in_progress: Arc<Mutex<HashSet<String>>>,
    latest_versions: Arc<Mutex<HashMap<String, String>>>,
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
            is_checking: false,
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

    /// Checks GitHub Releases & version.json for all 13 apps in a background thread
    pub fn check_all_updates(&mut self, repos: Vec<(String, String)>) {
        if self.is_checking {
            return;
        }
        self.is_checking = true;
        let latest_store = Arc::clone(&self.latest_versions);
        let tx = self.sender.clone();

        thread::spawn(move || {
            let mut found = 0;
            let suite_repo = "gochujangs33-dot/Craft-Studio";

            // Multi-tier fast version matching (version.json + releases + upstream)
            let ps_script = format!(
                "$ProgressPreference = 'SilentlyContinue'; \
                 $results = @{{}}; \
                 \
                 # Tier 1: Check remote version.json manifest \
                 try {{ \
                     $vjson = Invoke-RestMethod 'https://raw.githubusercontent.com/{suite_repo}/main/version.json' -Headers @{{ 'User-Agent' = 'ArtCraftStudio' }} -TimeoutSec 5; \
                     if ($vjson.apps) {{ \
                         foreach ($prop in $vjson.apps.PSObject.Properties) {{ \
                             if ($prop.Value.version) {{ $results[$prop.Name] = $prop.Value.version -replace '^v','' }} \
                         }} \
                     }} \
                 }} catch {{}} \
                 \
                 # Tier 2: Check GitHub Releases latest assets \
                 try {{ \
                     $rel = Invoke-RestMethod 'https://api.github.com/repos/{suite_repo}/releases/latest' -Headers @{{ 'User-Agent' = 'ArtCraftStudio' }} -TimeoutSec 6; \
                     if ($rel -and $rel.assets) {{ \
                         foreach ($a in $rel.assets) {{ \
                             if ($a.name -match '^([a-z0-9]+)-([0-9\\.]+)') {{ \
                                 $id = $matches[1]; $ver = $matches[2]; \
                                 $results[$id] = $ver; \
                             }} \
                         }} \
                     }} \
                 }} catch {{}} \
                 \
                 # Output gathered versions as id=ver lines \
                 foreach ($k in $results.Keys) {{ \"$k=$($results[$k])\" }}"
            );

            let output = Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            let mut gathered = HashMap::new();
            for line in output.lines() {
                if let Some((k, v)) = line.trim().split_once('=') {
                    if !k.is_empty() && !v.is_empty() {
                        gathered.insert(k.to_string(), v.to_string());
                    }
                }
            }

            // Fill gathered versions from user suite repository
            if let Ok(mut map) = latest_store.lock() {
                for (id, ver) in &gathered {
                    map.insert(id.clone(), ver.clone());
                    found += 1;
                }
            }

            // For any remaining apps not found in suite repo, fallback to official upstream repos
            for (id, repo) in &repos {
                if !gathered.contains_key(id) {
                    let repo_path = repo.trim_start_matches("https://github.com/").to_string();
                    let ps_fallback = format!(
                        "$ProgressPreference = 'SilentlyContinue'; \
                         try {{ \
                             $r = Invoke-RestMethod 'https://api.github.com/repos/{repo_path}/releases/latest' -Headers @{{ 'User-Agent' = 'ArtCraftStudio' }} -TimeoutSec 4; \
                             $r.tag_name -replace '^v','' \
                         }} catch {{ '' }}"
                    );
                    if let Ok(out) = Command::new("powershell")
                        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_fallback])
                        .output()
                    {
                        let tag = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if !tag.is_empty() {
                            if let Ok(mut map) = latest_store.lock() {
                                map.insert(id.clone(), tag);
                                found += 1;
                            }
                        }
                    }
                }
            }

            let _ = tx.send(("__check_all__".to_string(), true, format!("{found}개 프로그램 최신 정보 매칭 완료")));
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
        let tx = self.sender.clone();

        // Ensure update script exists on disk
        let script_path = std::env::temp_dir().join("artcraft_update_app.ps1");
        let _ = std::fs::write(&script_path, UPDATE_SCRIPT);

        thread::spawn(move || {
            let script_str = script_path.display().to_string();
            let dest_str = dest_dir.display().to_string();

            let status = Command::new("powershell")
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
                .status();

            let (success, msg) = match status {
                Ok(s) if s.success() => (true, format!("{app_id} 최신 버전 업데이트 완료")),
                Ok(s) => (false, format!("{app_id} 업데이트 실패 (코드: {:?})", s.code())),
                Err(e) => (false, format!("{app_id} 업데이트 스크립트 실행 오류: {e}")),
            };

            if let Ok(mut set) = in_progress.lock() {
                set.remove(&app_id);
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
