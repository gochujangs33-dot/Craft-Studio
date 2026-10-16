# CLAUDE.md - AI Coding Assistant & Developer Guide for Craft-Studio

Welcome to **Craft-Studio** (ArtCraft Studio)!  
This document serves as the single source of truth for AI agents (Claude Code, Codex, ChatGPT, Copilot, Antigravity) and human developers maintaining or extending this repository.

---

## 📌 Project Overview
* **Name**: Craft-Studio (ArtCraft Studio)
* **Goal**: A high-performance Windows desktop launcher and update manager for 13 official open-source creative/office apps written in Rust by ArtCraft.
* **Repository**: [https://github.com/gochujangs33-dot/Craft-Studio](https://github.com/gochujangs33-dot/Craft-Studio)
* **Official App Upstream**: `https://github.com/storytold/<id>`
* **Primary Language**: Rust (Edition 2021)
* **UI Framework**: `eframe` / `egui` (v0.31) with pure dark-mode theme & custom Korean typography (`malgun.ttf`)
* **Target OS**: Windows 10 / 11 (x64)

---

## 🏗️ Architecture & Codebase Map

```
Craft-Studio/
├── CLAUDE.md              # AI Agent onboarding & instruction manual (this file)
├── AGENTS.md              # Universal AI agent instruction spec
├── README.md              # Project documentation & user manual
├── registry.json          # Master registry defining all 13 apps (metadata, extensions, repos)
├── launcher/              # Core launcher application (Rust + egui)
│   ├── Cargo.toml         # Rust crate manifest & dependencies
│   ├── build.rs           # Windows .ico resource linker (winresource)
│   ├── assets/            # App icons, SVG/PNG logos, default window icons
│   └── src/
│       ├── main.rs        # Entry point, CLI file argument routing, Viewport & window settings
│       ├── apps.rs        # App scanner, registry loader, process manager, multi-window runner
│       ├── updater.rs     # 3M-style in-place replacement update manager & GitHub release polling
│       ├── theme.rs       # Colors, font injection (Malgun Gothic), custom dark design system
│       └── ui.rs          # Launcher GUI, responsive card grid, update center dialog, settings
├── scripts/               # Automation PowerShell scripts
│   ├── build-all.ps1      # Compiles release binary and assembles dist/ bundle
│   ├── package.ps1        # Creates standalone portable distribution ZIP (release/)
│   ├── fetch-apps.ps1     # Downloads official portable apps from GitHub into dist/apps/
│   ├── update-app.ps1     # 3M-style in-place file replacement updater script
│   └── prepare-release-assets.ps1 # Gathers launcher exe + full zip + version.json for GitHub Releases
├── dist/                  # Runnable distribution directory (gitignored)
│   ├── ArtCraftStudio.exe # Launcher executable
│   ├── apps/<id>/         # App binaries & .version files
│   ├── SavedFiles/        # User project outputs isolated per app
│   └── custom_apps/       # User-built custom modified binaries overriding official apps
└── release/               # Output directory for release ZIPs and upload assets (gitignored)
```

---

## ⚡ Essential Commands Cheat Sheet

| Task | Command (PowerShell) | Notes |
|---|---|---|
| **Build Launcher (Dev)** | `cd launcher; cargo check` | Fast syntax & type check (0 warnings) |
| **Run Launcher (Debug)** | `cd launcher; cargo run` | Runs launcher locally |
| **Build Full Release** | `powershell -ExecutionPolicy Bypass -File .\scripts\build-all.ps1` | Builds release exe & copies to `dist/` |
| **Package Portable ZIP** | `powershell -ExecutionPolicy Bypass -File .\scripts\package.ps1` | Creates `release/ArtCraftStudio-portable-x64.zip` |
| **Prepare GitHub Releases**| `powershell -ExecutionPolicy Bypass -File .\scripts\prepare-release-assets.ps1` | Generates files in `release/upload_assets/` |
| **Download Official Apps**| `powershell -ExecutionPolicy Bypass -File .\scripts\fetch-apps.ps1` | Populates `dist/apps/` from GitHub |

---

## 🔑 Key Engineering Rules & Invariants

1. **Multi-Window Multitasking**:
   - Clicking `[▶ LAUNCH]` on an app must **always open a new independent process/window**.
   - Do **NOT** focus/raise an existing process instead of opening a new one; artists frequently work across multiple projects or open multiple instances of the same tool simultaneously.
   - PIDs are tracked dynamically in `Runner` (`launcher/src/apps.rs`).

2. **Isolated Work Directory (`SavedFiles/`)**:
   - Every app runs with its working directory set to `<exe_dir>/SavedFiles/<AppName>/`.
   - Modifying or updating an app binary **must never delete or touch** files inside `SavedFiles/`.

3. **Custom Mod Binaries (`custom_apps/`)**:
   - If a binary exists at `<exe_dir>/custom_apps/<id>/<id>.exe`, the launcher prioritizes it over `apps/<id>/<id>.exe` and displays a purple `[MOD]` tag.

4. **3M-Style In-place Replacement Updates**:
   - Update files are downloaded from GitHub Releases, extracted, and copied over existing files in `apps/<id>/`.
   - Update script: `scripts/update-app.ps1` (embedded inside `updater.rs` via `include_str!`).

5. **Responsive Display & Zero-Delay Tooltips**:
   - The launcher window starts with `.with_maximized(true)` to adapt seamlessly to any monitor DPI scale or resolution.
   - Tooltip delay is configured to `0.0s` (`s.interaction.tooltip_delay = 0.0_f32`) for instantaneous card hover descriptions.
   - The card grid calculates columns dynamically (`cols.clamp(2, 6)`) and splits widths evenly across available viewport width.

---

## 🧩 How to Add a New App to the Suite

1. Open [registry.json](file:///c:/Users/0op64/.gemini/antigravity/scratch/ArtCraft/registry.json).
2. Append a new object under `"apps"`:
   ```json
   {
     "id": "newcraft",
     "prefix": "New",
     "group": "new",
     "order": 14,
     "tagline": "Brief description",
     "tagline_ko": "간략한 한글 설명",
     "desc_ko": "상세한 기능 설명",
     "desc_en": "Detailed feature description",
     "like": "Like CompetitorApp",
     "like_ko": "CompetitorApp 대체",
     "accent": "#HEXCOLOR",
     "repo": "https://github.com/storytold/newcraft",
     "website": "https://getartcraft.com/apps/newcraft",
     "extensions": ["ext1", "ext2"]
   }
   ```
3. Add a 64x64 PNG icon to `launcher/assets/icons/newcraft.png` (or let the dynamic glyph fallback handle it automatically).
4. Run `powershell -ExecutionPolicy Bypass -File .\scripts\build-all.ps1`.
5. The launcher UI will automatically render the new app card, register file associations, and enable updates.
