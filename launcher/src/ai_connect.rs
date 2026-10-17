//! Registers each installed Craft app's built-in MCP server with AI clients
//! (Claude Desktop, Cursor, Gemini CLI). Every Craft app ships `<id>-cli mcp`,
//! so the launcher only has to write the client's config file.
//!
//! Only entries whose name starts with `artcraft-` are ever added or removed;
//! everything else in the user's config is preserved. A one-time backup of the
//! original file is written next to it before the first change.

use crate::apps::{log, AppEntry};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const PREFIX: &str = "artcraft-";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Client {
    ClaudeDesktop,
    ClaudeCode,
    Codex,
    Grok,
    Cursor,
    GeminiCli,
}

impl Client {
    pub const ALL: [Client; 6] = [
        Client::ClaudeDesktop,
        Client::ClaudeCode,
        Client::Codex,
        Client::Grok,
        Client::Cursor,
        Client::GeminiCli,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Client::ClaudeDesktop => "Claude Desktop",
            Client::ClaudeCode => "Claude Code (CLI)",
            Client::Codex => "OpenAI Codex",
            Client::Grok => "xAI Grok",
            Client::Cursor => "Cursor",
            Client::GeminiCli => "Gemini CLI",
        }
    }

    /// Config file the client reads its `mcpServers` from, per OS.
    pub fn config_path(self) -> Option<PathBuf> {
        let home = home_dir()?;
        Some(match self {
            Client::ClaudeDesktop => claude_desktop_dir(&home)?.join("claude_desktop_config.json"),
            Client::ClaudeCode => home.join(".claude.json"),
            Client::Codex => home.join(".codex").join("config.toml"),
            Client::Grok => home.join(".grok").join("config.toml"),
            Client::Cursor => home.join(".cursor").join("mcp.json"),
            Client::GeminiCli => home.join(".gemini").join("settings.json"),
        })
    }

    /// The client looks installed when its config folder or file exists.
    pub fn is_present(self) -> bool {
        match self {
            Client::ClaudeCode => self.config_path().map(|p| p.is_file()).unwrap_or(false),
            _ => self
                .config_path()
                .map(|p| p.is_file() || p.parent().map(Path::is_dir).unwrap_or(false))
                .unwrap_or(false),
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    std::env::var_os(var).map(PathBuf::from)
}

fn claude_desktop_dir(home: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let _ = home;
        // The Microsoft Store (MSIX) build of Claude redirects %APPDATA% into its package folder.
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            if let Ok(rd) = std::fs::read_dir(local.join("Packages")) {
                for ent in rd.flatten() {
                    if ent.file_name().to_string_lossy().starts_with("Claude_") {
                        let p = ent.path().join("LocalCache").join("Roaming").join("Claude");
                        if p.is_dir() {
                            return Some(p);
                        }
                    }
                }
            }
        }
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Claude"))
    }
    #[cfg(target_os = "macos")]
    {
        Some(home.join("Library").join("Application Support").join("Claude"))
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
        Some(base.join("Claude"))
    }
}

/// Finds `<id>-cli` / `<id>-cli.exe` inside the app's install folder (up to 3 levels deep,
/// which also covers `<App>.app/Contents/MacOS/` on macOS).
pub fn find_cli(dir: &Path, id: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, names: &[String], depth: u32) -> Option<PathBuf> {
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
        subdirs.into_iter().find_map(|d| walk(&d, names, depth + 1))
    }
    walk(dir, &[format!("{id}-cli.exe"), format!("{id}-cli")], 0)
}

/// MCP server entries for every installed app that has a CLI and declares `mcp_args`.
pub fn server_entries(entries: &[AppEntry]) -> Vec<(String, Value)> {
    entries
        .iter()
        .filter(|e| e.exe.is_some() && !e.meta.mcp_args.is_empty())
        .filter_map(|e| {
            let cli = find_cli(&e.dir, &e.meta.id)?;
            let saves = e.save_dir.to_string_lossy().to_string();
            let args: Vec<String> = e.meta.mcp_args.iter().map(|a| a.replace("{saves}", &saves)).collect();
            Some((
                format!("{PREFIX}{}", e.meta.id),
                json!({ "command": cli.to_string_lossy(), "args": args }),
            ))
        })
        .collect()
}

/// Formats the current machine's MCP configuration as pretty JSON for pasting into any custom AI client.
pub fn mcp_json_string(entries: &[AppEntry]) -> String {
    let servers = server_entries(entries);
    let mut map = Map::new();
    for (k, v) in servers {
        map.insert(k, v);
    }
    serde_json::to_string_pretty(&json!({ "mcpServers": map })).unwrap_or_default()
}

fn read_config(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(format!("{}: top level is not a JSON object", path.display())),
        // Never overwrite a file we can't parse (e.g. JSON with comments) — the user would lose it.
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write_config(path: &Path, root: &Map<String, Value>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if path.is_file() {
        let backup = path.with_extension("json.artcraft-backup");
        if !backup.exists() {
            std::fs::copy(path, &backup).map_err(|e| format!("backup failed: {e}"))?;
        }
    }
    let text = serde_json::to_string_pretty(&Value::Object(root.clone())).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.artcraft-tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn servers_mut(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    let v = root.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    v.as_object_mut().ok_or_else(|| "\"mcpServers\" is not a JSON object".to_string())
}

// ---------------------------------------------------------------------------
// TOML Manipulation for OpenAI Codex (~/.codex/config.toml)
// ---------------------------------------------------------------------------

fn is_toml_table_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('[') && trimmed.ends_with(']')
}

fn is_artcraft_toml_header(line: &str) -> bool {
    let trimmed = line.trim();
    if !is_toml_table_header(trimmed) {
        return false;
    }
    let inner = trimmed.trim_matches(|c| c == '[' || c == ']').trim();
    let lower = inner.to_ascii_lowercase();
    lower.starts_with("mcp_servers.artcraft-")
        || lower.starts_with("mcp_servers.\"artcraft-")
        || lower.starts_with("mcp_servers.'artcraft-")
}

fn strip_artcraft_toml_sections(content: &str) -> (String, usize) {
    let newline = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(content.len());
    let mut skipping = false;
    let mut removed = 0;

    for line in content.lines() {
        if is_toml_table_header(line) {
            if is_artcraft_toml_header(line) {
                skipping = true;
                removed += 1;
                continue;
            } else {
                skipping = false;
            }
        }
        if !skipping {
            out.push_str(line);
            out.push_str(newline);
        }
    }
    (out, removed)
}

fn toml_status(path: &Path) -> Result<usize, String> {
    if !path.is_file() {
        return Ok(0);
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let count = content.lines().filter(|l| is_artcraft_toml_header(l)).count();
    Ok(count)
}

fn write_toml_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if path.is_file() {
        let backup = path.with_extension("toml.artcraft-backup");
        if !backup.exists() {
            std::fs::copy(path, &backup).map_err(|e| format!("backup failed: {e}"))?;
        }
    }
    let tmp = path.with_extension("toml.artcraft-tmp");
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn toml_disconnect(path: &Path) -> Result<usize, String> {
    if !path.is_file() {
        return Ok(0);
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let newline = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let (stripped, removed) = strip_artcraft_toml_sections(&content);
    if removed > 0 {
        let mut clean = stripped.trim_end().to_string();
        if !clean.is_empty() {
            clean.push_str(newline);
        }
        write_toml_file(path, &clean)?;
    }
    Ok(removed)
}

fn toml_connect(path: &Path, entries: &[AppEntry]) -> Result<usize, String> {
    let servers = server_entries(entries);
    if servers.is_empty() {
        return Err("no installed app with a CLI was found".into());
    }
    let existing = if path.is_file() {
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?
    } else {
        String::new()
    };
    let newline = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let (stripped, _) = strip_artcraft_toml_sections(&existing);
    let mut base = stripped.trim_end().to_string();
    if !base.is_empty() {
        base.push_str(newline);
        base.push_str(newline);
    }

    let n = servers.len();
    for (name, val) in servers {
        let cmd = val.get("command").and_then(Value::as_str).unwrap_or("");
        let args_val = val.get("args").cloned().unwrap_or(Value::Array(Vec::new()));
        let cmd_escaped = serde_json::to_string(cmd).unwrap_or_else(|_| format!("\"{}\"", cmd));
        let args_escaped = serde_json::to_string(&args_val).unwrap_or_else(|_| "[]".into());

        base.push_str(&format!("[mcp_servers.{name}]{newline}"));
        base.push_str(&format!("command = {cmd_escaped}{newline}"));
        base.push_str(&format!("args = {args_escaped}{newline}{newline}"));
    }

    let final_content = format!("{}{newline}", base.trim_end());
    write_toml_file(path, &final_content)?;
    Ok(n)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Number of `artcraft-*` servers currently registered with the client.
pub fn status(client: Client) -> Result<usize, String> {
    let path = client.config_path().ok_or("no home directory")?;
    if client == Client::Codex || client == Client::Grok {
        return toml_status(&path);
    }
    let root = read_config(&path)?;
    Ok(root
        .get("mcpServers")
        .and_then(Value::as_object)
        .map(|m| m.keys().filter(|k| k.starts_with(PREFIX)).count())
        .unwrap_or(0))
}

/// Replaces all `artcraft-*` servers in the client's config with the current set.
pub fn connect(client: Client, entries: &[AppEntry]) -> Result<usize, String> {
    let path = client.config_path().ok_or("no home directory")?;
    if client == Client::Codex || client == Client::Grok {
        let n = toml_connect(&path, entries)?;
        log(&format!("ai_connect: registered {n} MCP servers with {} ({})", client.label(), path.display()));
        return Ok(n);
    }
    let servers = server_entries(entries);
    if servers.is_empty() {
        return Err("no installed app with a CLI was found".into());
    }
    let mut root = read_config(&path)?;
    let map = servers_mut(&mut root)?;
    map.retain(|k, _| !k.starts_with(PREFIX));
    let n = servers.len();
    for (k, v) in servers {
        map.insert(k, v);
    }
    write_config(&path, &root)?;
    log(&format!("ai_connect: registered {n} MCP servers with {} ({})", client.label(), path.display()));
    Ok(n)
}

/// Removes every `artcraft-*` server from the client's config.
pub fn disconnect(client: Client) -> Result<usize, String> {
    let path = client.config_path().ok_or("no home directory")?;
    if client == Client::Codex || client == Client::Grok {
        let removed = toml_disconnect(&path)?;
        log(&format!("ai_connect: removed {removed} MCP servers from {}", client.label()));
        return Ok(removed);
    }
    if !path.is_file() {
        return Ok(0);
    }
    let mut root = read_config(&path)?;
    let map = servers_mut(&mut root)?;
    let before = map.len();
    map.retain(|k, _| !k.starts_with(PREFIX));
    let removed = before - map.len();
    if removed > 0 {
        write_config(&path, &root)?;
    }
    log(&format!("ai_connect: removed {removed} MCP servers from {}", client.label()));
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_preserves_foreign_entries_and_disconnect_removes_only_ours() {
        let dir = std::env::temp_dir().join(format!("artcraft_ai_connect_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("cfg.json");
        std::fs::write(&path, r#"{"theme":"dark","mcpServers":{"other":{"command":"x"},"artcraft-old":{"command":"y"}}}"#)
            .unwrap();

        let mut root = read_config(&path).unwrap();
        let map = servers_mut(&mut root).unwrap();
        map.retain(|k, _| !k.starts_with(PREFIX));
        map.insert("artcraft-photocraft".into(), json!({"command": "p"}));
        write_config(&path, &root).unwrap();

        let back = read_config(&path).unwrap();
        assert_eq!(back["theme"], "dark");
        let s = back["mcpServers"].as_object().unwrap();
        assert!(s.contains_key("other"));
        assert!(s.contains_key("artcraft-photocraft"));
        assert!(!s.contains_key("artcraft-old"));
        assert!(path.with_extension("json.artcraft-backup").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unparsable_config_is_refused() {
        let dir = std::env::temp_dir().join(format!("artcraft_ai_connect_bad_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("cfg.json");
        std::fs::write(&path, "{ // comment\n \"a\": 1 }").unwrap();
        assert!(read_config(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toml_codex_preserves_foreign_tables_and_updates_cleanly() {
        let dir = std::env::temp_dir().join(format!("artcraft_ai_connect_toml_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");

        let initial_toml = r#"# OpenAI Codex config
model = "gpt-6-astra"

[mcp_servers.originkit]
url = "https://mcp.originkit.dev/mcp"
enabled_tools = ["list_components"]

[mcp_servers.artcraft-old]
command = "old-cli.exe"
args = ["mcp"]

[desktop]
ambient-suggestions-enabled = true

[projects.'c:\users\test\project']
trust_level = "trusted"
"#;
        std::fs::write(&path, initial_toml).unwrap();

        assert_eq!(toml_status(&path).unwrap(), 1);

        let (stripped, removed) = strip_artcraft_toml_sections(initial_toml);
        assert_eq!(removed, 1);
        assert!(!stripped.contains("artcraft-old"));
        assert!(stripped.contains("model = \"gpt-6-astra\""));
        assert!(stripped.contains("[mcp_servers.originkit]"));
        assert!(stripped.contains("[desktop]"));
        assert!(stripped.contains("[projects.'c:\\users\\test\\project']"));

        let removed = toml_disconnect(&path).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(toml_status(&path).unwrap(), 0);

        // Verify backup was made
        assert!(path.with_extension("toml.artcraft-backup").is_file());

        let after_disconnect = std::fs::read_to_string(&path).unwrap();
        assert!(!after_disconnect.contains("artcraft-old"));
        assert!(after_disconnect.contains("[mcp_servers.originkit]"));
        assert!(after_disconnect.contains("[projects.'c:\\users\\test\\project']"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
