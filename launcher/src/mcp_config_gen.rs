//! ArtCraft Studio - MCP Configuration Generator
//! Generates and merges MCP (Model Context Protocol) configurations with live `--bridge` mode
//! for external AI tools: Cursor, Claude Desktop, Antigravity (.mcp.json), Claude Code, and Codex.

use crate::apps::{log, AppEntry};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const PREFIX: &str = "artcraft-";
pub const DEFAULT_CONTROL_TOKEN: &str = crate::apps::COMMON_CONTROL_TOKEN;

/// Returns the control port for an app.
/// Delegates to `crate::apps::get_app_control_port` (PhotoCraft=49152, VectorCraft=49153, FilmCraft=49154, ...).
pub fn get_app_port(id: &str, order: u32) -> u16 {
    if let Some(port) = crate::apps::get_app_control_port(id) {
        port
    } else {
        (49151 + order) as u16
    }
}

/// Returns the CLI arguments for running an app in MCP mode with live application bridge.
pub fn get_bridge_args(id: &str, port: u16, token: &str, saves_dir: Option<&Path>) -> Vec<String> {
    let host_port = format!("127.0.0.1:{port}");
    let port_str = port.to_string();
    let saves_str = saves_dir.map(|p| p.to_string_lossy().to_string());

    match id.to_lowercase().as_str() {
        "photocraft" => {
            let mut args = vec![
                "mcp".to_string(),
                "--bridge".to_string(),
                host_port,
                "--control-token".to_string(),
                token.to_string(),
            ];
            if let Some(saves) = saves_str {
                args.push("--automation-read-root".to_string());
                args.push(saves.clone());
                args.push("--automation-write-root".to_string());
                args.push(saves);
            }
            args
        }
        "filmcraft" => {
            // filmcraft-cli [OPTIONS] mcp, e.g. --bridge 127.0.0.1:PORT mcp
            vec!["--bridge".to_string(), host_port, "mcp".to_string()]
        }
        "effectcraft" => {
            vec!["mcp".to_string(), "--bridge".to_string(), port_str]
        }
        "figcraft" => {
            vec!["mcp".to_string(), "--attach".to_string(), port_str]
        }
        "wordcraft" => {
            vec!["mcp".to_string(), "--connect".to_string(), host_port]
        }
        "gridcraft" => {
            vec!["mcp".to_string(), "--connect".to_string(), port_str]
        }
        "cadcraft" => {
            vec!["mcp".to_string(), "--connect".to_string(), host_port]
        }
        "soundcraft" => {
            vec!["mcp".to_string(), "--connect".to_string(), port_str]
        }
        "designcraft" => {
            vec!["mcp".to_string(), "--connect".to_string(), port_str]
        }
        "lightcraft" => {
            vec!["mcp".to_string(), "--connect".to_string(), host_port]
        }
        "vectorcraft" => {
            vec!["mcp".to_string(), "--connect".to_string(), host_port]
        }
        "pdfcraft" => {
            vec!["mcp".to_string()]
        }
        "hop" => {
            vec!["mcp".to_string(), "--connect".to_string(), host_port]
        }
        _ => {
            // Generic bridge fallback
            vec![
                "mcp".to_string(),
                "--bridge".to_string(),
                host_port,
                "--control-token".to_string(),
                token.to_string(),
            ]
        }
    }
}

/// Searches for the CLI executable for an app across common directories:
/// - app_dir provided by AppEntry
/// - dist/apps/<id>/
/// - apps/<id>/
/// - custom_apps/<id>/
pub fn find_app_cli(id: &str, app_dir: Option<&Path>) -> Option<PathBuf> {
    let mut names = vec![format!("{id}-cli.exe"), format!("{id}-cli")];
    if id.eq_ignore_ascii_case("photocraft") {
        names.push("photocraft-cli.exe".into());
        names.push("photocraft-cli".into());
    } else if id.eq_ignore_ascii_case("hop") {
        names.push("hop.exe".into());
        names.push("hop".into());
        names.push("hop-desktop.exe".into());
    }

    // 1. Check inside provided app_dir
    if let Some(dir) = app_dir {
        if dir.is_dir() {
            for name in &names {
                let direct = dir.join(name);
                if direct.is_file() {
                    return canonical_or_abs(direct);
                }
            }
            if let Some(found) = walk_for_cli(dir, &names, 0) {
                return canonical_or_abs(found);
            }
        }
    }

    // 2. Check candidate search roots relative to current exe
    let mut root_dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            root_dirs.push(parent.to_path_buf());
            root_dirs.push(parent.join("dist"));
            if let Some(grand) = parent.parent() {
                root_dirs.push(grand.to_path_buf());
                root_dirs.push(grand.join("dist"));
            }
        }
    }

    // 3. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        root_dirs.push(cwd.clone());
        root_dirs.push(cwd.join("dist"));
    }

    for root in root_dirs {
        let app_folders = [
            root.join("apps").join(id),
            root.join("custom_apps").join(id),
            root.join("dist").join("apps").join(id),
        ];
        for folder in app_folders {
            if folder.is_dir() {
                for name in &names {
                    let cand = folder.join(name);
                    if cand.is_file() {
                        return canonical_or_abs(cand);
                    }
                }
                if let Some(found) = walk_for_cli(&folder, &names, 0) {
                    return canonical_or_abs(found);
                }
            }
        }
    }

    None
}

fn walk_for_cli(dir: &Path, names: &[String], depth: u32) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            subdirs.push(p);
        } else if p
            .file_name()
            .map(|n| names.iter().any(|w| n.to_string_lossy().eq_ignore_ascii_case(w)))
            .unwrap_or(false)
        {
            return Some(p);
        }
    }
    subdirs.into_iter().find_map(|d| walk_for_cli(&d, names, depth + 1))
}

fn canonical_or_abs(p: PathBuf) -> Option<PathBuf> {
    if let Ok(can) = p.canonicalize() {
        // Strip Windows extended-length prefix (\\?\) for broad JSON compatibility
        let s = can.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return Some(PathBuf::from(stripped));
        }
        return Some(can);
    }
    if let Ok(abs) = std::path::absolute(&p) {
        return Some(abs);
    }
    Some(p)
}

/// Generates the list of (server_name, json_definition) for all apps.
pub fn generate_mcp_server_entries(
    entries: &[AppEntry],
    saves_base: Option<&Path>,
) -> Vec<(String, Value)> {
    let mut result = Vec::new();

    for e in entries {
        let id = &e.meta.id;
        let cli_opt = find_app_cli(id, Some(&e.dir));
        let Some(cli) = cli_opt else {
            continue;
        };

        let port = get_app_port(id, e.meta.order);
        let saves_dir = saves_base.map(|s| s.join(e.meta.name())).unwrap_or_else(|| e.save_dir.clone());
        let args = get_bridge_args(id, port, DEFAULT_CONTROL_TOKEN, Some(&saves_dir));

        let server_name = format!("{PREFIX}{id}");
        let mut entry_obj = Map::new();
        entry_obj.insert("command".to_string(), json!(cli.to_string_lossy()));
        entry_obj.insert("args".to_string(), json!(args));

        // Inject bridge environment variables for live connection
        let mut env_map = Map::new();
        env_map.insert("ARTCRAFT_BRIDGE_PORT".to_string(), json!(port.to_string()));
        env_map.insert("ARTCRAFT_CONTROL_TOKEN".to_string(), json!(DEFAULT_CONTROL_TOKEN));
        let upper = id.to_ascii_uppercase().replace('-', "_");
        env_map.insert(format!("{upper}_CONTROL_PORT"), json!(port.to_string()));
        env_map.insert(format!("{upper}_CONTROL_TOKEN"), json!(DEFAULT_CONTROL_TOKEN));
        entry_obj.insert("env".to_string(), Value::Object(env_map));

        result.push((server_name, Value::Object(entry_obj)));
    }

    result
}

// ---------------------------------------------------------------------------
// File paths for external AI tools
// ---------------------------------------------------------------------------

pub fn user_home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    std::env::var_os(var).map(PathBuf::from)
}

/// Target paths for Cursor MCP configuration:
/// - %USERPROFILE%\.cursor\mcp.json
/// - %APPDATA%\Cursor\User\globalStorage\rooveterinaryinc.cursor-mcp\mcp.json (if exists)
pub fn get_cursor_mcp_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = user_home_dir() {
        paths.push(home.join(".cursor").join("mcp.json"));
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
        let ext_path = appdata
            .join("Cursor")
            .join("User")
            .join("globalStorage")
            .join("rooveterinaryinc.cursor-mcp")
            .join("mcp.json");
        if ext_path.is_file() {
            paths.push(ext_path);
        }
    }
    paths
}

/// Target paths for Claude Desktop configuration:
/// - %APPDATA%\Claude\claude_desktop_config.json
/// - MSIX Package Roaming path if installed from Microsoft Store
/// - macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
pub fn get_claude_desktop_mcp_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            if let Ok(rd) = std::fs::read_dir(local.join("Packages")) {
                for ent in rd.flatten() {
                    if ent.file_name().to_string_lossy().starts_with("Claude_") {
                        let p = ent
                            .path()
                            .join("LocalCache")
                            .join("Roaming")
                            .join("Claude")
                            .join("claude_desktop_config.json");
                        paths.push(p);
                    }
                }
            }
        }
        if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
            paths.push(appdata.join("Claude").join("claude_desktop_config.json"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = user_home_dir() {
            paths.push(home.join("Library").join("Application Support").join("Claude").join("claude_desktop_config.json"));
        }
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        if let Some(home) = user_home_dir() {
            let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
            paths.push(base.join("Claude").join("claude_desktop_config.json"));
        }
    }
    paths
}

/// Target paths for Antigravity & Workspace root:
/// - Current working directory `.mcp.json`
/// - Parent / Workspace root `.mcp.json`
pub fn get_antigravity_mcp_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join(".mcp.json"));
        if let Some(p) = cwd.parent() {
            let parent_cand = p.join(".mcp.json");
            if parent_cand.is_file() {
                paths.push(parent_cand);
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            let exe_cand = p.join(".mcp.json");
            if !paths.contains(&exe_cand) {
                paths.push(exe_cand);
            }
            if let Some(grand) = p.parent() {
                let grand_cand = grand.join(".mcp.json");
                if grand_cand.is_file() && !paths.contains(&grand_cand) {
                    paths.push(grand_cand);
                }
            }
        }
    }
    paths
}

// ---------------------------------------------------------------------------
// JSON Configuration Merging Engine
// ---------------------------------------------------------------------------

/// Merges `artcraft-*` MCP server entries into a target JSON configuration file.
/// Preserves any existing non-artcraft entries and other top-level keys.
pub fn merge_mcp_json_file(path: &Path, servers: &[(String, Value)]) -> Result<usize, String> {
    if servers.is_empty() {
        return Err("No ArtCraft CLI tools found to register".into());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create dir {}: {e}", parent.display()))?;
    }

    let mut root = if path.is_file() {
        let content = std::fs::read_to_string(path).map_err(|e| format!("read error {}: {e}", path.display()))?;
        let cleaned = content.trim_start_matches('\u{feff}').trim();
        if cleaned.is_empty() {
            Map::new()
        } else {
            match serde_json::from_str::<Value>(cleaned) {
                Ok(Value::Object(m)) => m,
                Ok(_) => return Err(format!("{}: top level is not a JSON object", path.display())),
                Err(e) => return Err(format!("invalid JSON in {}: {e}", path.display())),
            }
        }
    } else {
        Map::new()
    };

    // Make backup on first change
    if path.is_file() {
        let backup = path.with_extension("json.artcraft-backup");
        if !backup.exists() {
            let _ = std::fs::copy(path, &backup);
        }
    }

    let mcp_servers_entry = root.entry("mcpServers".to_string()).or_insert_with(|| Value::Object(Map::new()));
    let Some(servers_map) = mcp_servers_entry.as_object_mut() else {
        return Err(format!("{}: 'mcpServers' is not a JSON object", path.display()));
    };

    // Remove old ArtCraft servers to replace with latest paths and bridge args
    servers_map.retain(|k, _| !k.starts_with(PREFIX));

    for (k, v) in servers {
        servers_map.insert(k.clone(), v.clone());
    }

    let count = servers.len();
    let updated_json = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| format!("serialization error: {e}"))?;

    let tmp_path = path.with_extension("json.artcraft-tmp");
    std::fs::write(&tmp_path, updated_json).map_err(|e| format!("write error {}: {e}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("rename error to {}: {e}", path.display()))?;

    log(&format!("mcp_config_gen: merged {count} MCP bridge servers into {}", path.display()));
    Ok(count)
}

// ---------------------------------------------------------------------------
// Orchestrator: Sync all AI tools
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SyncReport {
    pub client_name: String,
    pub path: PathBuf,
    pub count: usize,
    pub success: bool,
    pub error: Option<String>,
}

/// Scans installed Craft apps and merges bridge configurations into Cursor, Claude Desktop, Antigravity, etc.
pub fn sync_all_ai_mcp_configs(entries: &[AppEntry], saves_base: Option<&Path>) -> Vec<SyncReport> {
    let mut reports = Vec::new();
    let servers = generate_mcp_server_entries(entries, saves_base);

    if servers.is_empty() {
        reports.push(SyncReport {
            client_name: "ArtCraft Suite".to_string(),
            path: PathBuf::new(),
            count: 0,
            success: false,
            error: Some("CLIs for Craft apps not found in dist/apps/ or apps/".to_string()),
        });
        return reports;
    }

    // 1. Cursor
    for path in get_cursor_mcp_paths() {
        match merge_mcp_json_file(&path, &servers) {
            Ok(count) => reports.push(SyncReport {
                client_name: "Cursor".to_string(),
                path,
                count,
                success: true,
                error: None,
            }),
            Err(e) => reports.push(SyncReport {
                client_name: "Cursor".to_string(),
                path,
                count: 0,
                success: false,
                error: Some(e),
            }),
        }
    }

    // 2. Claude Desktop
    for path in get_claude_desktop_mcp_paths() {
        match merge_mcp_json_file(&path, &servers) {
            Ok(count) => reports.push(SyncReport {
                client_name: "Claude Desktop".to_string(),
                path,
                count,
                success: true,
                error: None,
            }),
            Err(e) => reports.push(SyncReport {
                client_name: "Claude Desktop".to_string(),
                path,
                count: 0,
                success: false,
                error: Some(e),
            }),
        }
    }

    // 3. Antigravity / Workspace Root (.mcp.json)
    for path in get_antigravity_mcp_paths() {
        match merge_mcp_json_file(&path, &servers) {
            Ok(count) => reports.push(SyncReport {
                client_name: "Antigravity (.mcp.json)".to_string(),
                path,
                count,
                success: true,
                error: None,
            }),
            Err(e) => reports.push(SyncReport {
                client_name: "Antigravity (.mcp.json)".to_string(),
                path,
                count: 0,
                success: false,
                error: Some(e),
            }),
        }
    }

    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_port_allocation() {
        assert_eq!(get_app_port("photocraft", 1), 49152);
        assert_eq!(get_app_port("vectorcraft", 2), 49153);
        assert_eq!(get_app_port("filmcraft", 3), 49154);
        assert_eq!(get_app_port("wordcraft", 8), 49159);
        assert_eq!(get_app_port("gridcraft", 10), 49161);
        assert_eq!(get_app_port("hop", 14), 49165);
    }

    #[test]
    fn test_bridge_arguments_formatting() {
        let token = DEFAULT_CONTROL_TOKEN;
        let photo_args = get_bridge_args("photocraft", 49152, token, None);
        assert_eq!(photo_args[0], "mcp");
        assert_eq!(photo_args[1], "--bridge");
        assert_eq!(photo_args[2], "127.0.0.1:49152");
        assert_eq!(photo_args[3], "--control-token");
        assert_eq!(photo_args[4], token);

        let film_args = get_bridge_args("filmcraft", 49154, token, None);
        assert_eq!(film_args, vec!["--bridge", "127.0.0.1:49154", "mcp"]);

        let word_args = get_bridge_args("wordcraft", 49159, token, None);
        assert_eq!(word_args, vec!["mcp", "--connect", "127.0.0.1:49159"]);
    }

    #[test]
    fn test_merge_preserves_foreign_and_updates_artcraft() {
        let temp_dir = std::env::temp_dir().join(format!("artcraft_mcp_gen_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let config_file = temp_dir.join("test_mcp.json");

        let initial_json = json!({
            "theme": "dark",
            "mcpServers": {
                "sqlite": {
                    "command": "uvx",
                    "args": ["mcp-server-sqlite"]
                },
                "artcraft-old": {
                    "command": "old-cli.exe",
                    "args": ["mcp"]
                }
            }
        });
        std::fs::write(&config_file, serde_json::to_string_pretty(&initial_json).unwrap()).unwrap();

        let new_servers = vec![
            (
                "artcraft-photocraft".to_string(),
                json!({
                    "command": "C:\\dist\\apps\\photocraft\\photocraft-cli.exe",
                    "args": ["mcp", "--bridge", "127.0.0.1:49152"]
                }),
            ),
        ];

        let count = merge_mcp_json_file(&config_file, &new_servers).unwrap();
        assert_eq!(count, 1);

        let result_str = std::fs::read_to_string(&config_file).unwrap();
        let parsed: Value = serde_json::from_str(&result_str).unwrap();

        assert_eq!(parsed["theme"], "dark");
        let servers = parsed["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("sqlite"));
        assert!(!servers.contains_key("artcraft-old"));
        assert!(servers.contains_key("artcraft-photocraft"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sync_real_apps_to_workspace_mcp() {
        let reg = crate::apps::load_registry(Path::new(".."));
        let entries = crate::apps::scan(
            &reg,
            Path::new("../dist/apps"),
            Path::new("../dist"),
            Path::new("../dist/SavedFiles"),
        );
        let servers = generate_mcp_server_entries(&entries, Some(Path::new("../dist/SavedFiles")));
        assert!(!servers.is_empty(), "Should discover CLI servers from dist/apps");
        let workspace_mcp = Path::new("../.mcp.json");
        let res = merge_mcp_json_file(workspace_mcp, &servers);
        assert!(res.is_ok(), "Merge into workspace .mcp.json must succeed");
    }

    #[test]
    fn test_sync_all_ai_mcp_configs_execution() {
        let reg = crate::apps::load_registry(Path::new(".."));
        let entries = crate::apps::scan(
            &reg,
            Path::new("../dist/apps"),
            Path::new("../dist"),
            Path::new("../dist/SavedFiles"),
        );
        let reports = sync_all_ai_mcp_configs(&entries, Some(Path::new("../dist/SavedFiles")));
        println!("Sync Reports: {reports:#?}");
        let successes: Vec<_> = reports.iter().filter(|r| r.success).collect();
        assert!(!successes.is_empty(), "At least one target config should be synced successfully");
    }
}
