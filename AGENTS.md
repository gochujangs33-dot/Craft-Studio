# AGENTS.md - Multi-Agent & LLM Operational Guide for Craft-Studio

This file specifies operating procedures, design constraints, and environment requirements for automated AI coding agents (OpenAI Codex, Claude Code, Cursor, Copilot Workspace, Antigravity).

---

## 🤖 Environment Specifications
* **OS**: Windows 10 / Windows 11 (MSVC x64 ABI)
* **Shell**: PowerShell 5.1 / PowerShell 7+
* **Rust Toolchain**: `stable-x86_64-pc-windows-msvc` (Edition 2021)
* **Build Command**: `powershell -ExecutionPolicy Bypass -File .\scripts\build-all.ps1`
* **Artifact Output**: `dist/ArtCraftStudio.exe` (Runnable bundle)
* **Packaging**: `release/ArtCraftStudio-portable-x64.zip`
* **Release Target**: `https://github.com/gochujangs33-dot/Craft-Studio`

---

## 📂 Core Module Responsibilities

| File | Purpose | Critical Invariant |
|---|---|---|
| `launcher/src/main.rs` | Application entry point | Viewport starts with `.with_maximized(true)` and safe fallback inner sizes |
| `launcher/src/apps.rs` | App scanning, metadata model, runner | `launch()` spawns detached independent processes. Working dir is set to `SavedFiles/<AppName>` |
| `launcher/src/updater.rs` | In-place replacement updater | Updates download assets and overwrite existing binaries in-place (3M-style). Never touch `SavedFiles/` |
| `launcher/src/theme.rs` | Dark visual tokens & fonts | Injects `malgun.ttf` (맑은 고딕) for crisp Korean rendering. Zero-delay tooltips (`tooltip_delay = 0.0`) |
| `launcher/src/ui.rs` | Egui immediate-mode UI | Responsive card grid with 14px gaps, 2-6 columns auto-calculated from available width |
| `registry.json` | Master app database | Single declarative JSON source for all apps, tags, descriptions, extensions, and repos |

---

## 🛠️ Modifying Code Safely
1. **Adding/Fixing UI Features**:
   - Work inside `launcher/src/ui.rs`.
   - Never hardcode Korean or English strings directly in deep components; use the `is_ko()` helper pattern.
2. **Updating App Scanning/Paths**:
   - Work inside `launcher/src/apps.rs`.
   - Preserve priority: `custom_apps/<id>/` > `apps/<id>/`.
3. **Compilation Verification**:
   - Always verify that `cargo check` compiles with **0 warnings and 0 errors** before concluding tasks:
     ```powershell
     powershell -Command "& $env:USERPROFILE\.cargo\bin\cargo.exe check"
     ```
4. **Dist Sync**:
   - Whenever `launcher` is recompiled, run `scripts/build-all.ps1` so `dist/ArtCraftStudio.exe` is updated.
