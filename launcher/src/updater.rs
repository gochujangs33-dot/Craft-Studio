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

impl UpdateManager {
    pub fn new(exe_dir: PathBuf) -> Self {
        let (sender, receiver) = channel();
        Self {
            _exe_dir: exe_dir,
            sender,
            receiver,
            in_progress: Arc::new(Mutex::new(HashSet::new())),
            latest_versions: Arc::new(Mutex::new(HashMap::new())),
            is_checking: false,
        }
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

    /// Checks GitHub Releases for all 13 apps in a background thread
    pub fn check_all_updates(&mut self, repos: Vec<(String, String)>) {
        if self.is_checking {
            return;
        }
        self.is_checking = true;
        let latest_store = Arc::clone(&self.latest_versions);
        let tx = self.sender.clone();

        thread::spawn(move || {
            let mut found = 0;
            for (id, repo) in repos {
                let repo_path = repo.trim_start_matches("https://github.com/").to_string();
                let ps_cmd = format!(
                    "$ProgressPreference = 'SilentlyContinue'; \
                     try {{ \
                         $r = Invoke-RestMethod 'https://api.github.com/repos/{}/releases/latest' -Headers @{{ 'User-Agent' = 'ArtCraftStudio' }} -TimeoutSec 8; \
                         $r.tag_name -replace '^v','' \
                     }} catch {{ '' }}",
                    repo_path
                );

                if let Ok(output) = Command::new("powershell")
                    .args(["-NoProfile", "-NonInteractive", "-Command", &ps_cmd])
                    .output()
                {
                    let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !tag.is_empty() {
                        if let Ok(mut map) = latest_store.lock() {
                            map.insert(id.clone(), tag);
                            found += 1;
                        }
                    }
                }
            }
            let _ = tx.send(("__check_all__".to_string(), true, format!("{found}개 프로그램 최신 정보 확인 완료")));
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
                Ok(s) if s.success() => (true, format!("{app_id} 업데이트 완료 (동일 위치 대체 적용됨)")),
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
