#!/bin/bash
# ==============================================================================
# ArtCraft Studio - macOS Application Bundle & DMG Packaging Script
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
APP_BUNDLE="${DIST_DIR}/ArtCraftStudio.app"
CONTENTS="${APP_BUNDLE}/Contents"
MACOS_DIR="${CONTENTS}/MacOS"
RESOURCES_DIR="${CONTENTS}/Resources"
RELEASE_DIR="${ROOT_DIR}/release"

echo "==> Building ArtCraft Studio for macOS..."

# 1. Build release binary using cargo if not already provided
TARGET_DIR="${ROOT_DIR}/launcher/target"
BINARY_SRC=""

if [ -n "${PREBUILT_BINARY:-}" ]; then
    if [ -f "${PREBUILT_BINARY}" ]; then
        BINARY_SRC="${PREBUILT_BINARY}"
    elif [ -f "${ROOT_DIR}/${PREBUILT_BINARY}" ]; then
        BINARY_SRC="${ROOT_DIR}/${PREBUILT_BINARY}"
    else
        echo "==> Error: Specified PREBUILT_BINARY not found: ${PREBUILT_BINARY}" >&2
        exit 1
    fi
    echo "==> Using prebuilt binary: ${BINARY_SRC}"
elif [ -f "${TARGET_DIR}/universal/ArtCraftStudio" ]; then
    BINARY_SRC="${TARGET_DIR}/universal/ArtCraftStudio"
else
    cd "${ROOT_DIR}/launcher"
    TARGET="$(uname -m)"
    if [ "$TARGET" = "arm64" ]; then
        cargo build --release --target aarch64-apple-darwin || cargo build --release
        BINARY_SRC="${TARGET_DIR}/aarch64-apple-darwin/release/ArtCraftStudio"
        [ -f "$BINARY_SRC" ] || BINARY_SRC="${TARGET_DIR}/release/ArtCraftStudio"
    else
        cargo build --release --target x86_64-apple-darwin || cargo build --release
        BINARY_SRC="${TARGET_DIR}/x86_64-apple-darwin/release/ArtCraftStudio"
        [ -f "$BINARY_SRC" ] || BINARY_SRC="${TARGET_DIR}/release/ArtCraftStudio"
    fi
fi

# 2. Prepare .app structure in dist/
echo "==> Packaging ${APP_BUNDLE}..."
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

# Copy binary
if [ -f "$BINARY_SRC" ]; then
    cp -f "$BINARY_SRC" "${MACOS_DIR}/ArtCraftStudio"
    chmod +x "${MACOS_DIR}/ArtCraftStudio"
else
    echo "==> Warning: Binary not found at ${BINARY_SRC}"
fi

# Write Info.plist
cat << 'EOF' > "${CONTENTS}/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>ko</string>
    <key>CFBundleDisplayName</key>
    <string>ArtCraft Studio</string>
    <key>CFBundleExecutable</key>
    <string>ArtCraftStudio</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.artcraft.studio</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>ArtCraft Studio</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>ArtCraft Studio Contributors</string>
</dict>
</plist>
EOF

# Write PkgInfo
echo -n "APPL????" > "${CONTENTS}/PkgInfo"

# Copy app assets & registries
if [ -f "${ROOT_DIR}/registry.json" ]; then
    cp -f "${ROOT_DIR}/registry.json" "${RESOURCES_DIR}/"
fi
if [ -f "${ROOT_DIR}/version.json" ]; then
    cp -f "${ROOT_DIR}/version.json" "${RESOURCES_DIR}/"
fi
if [ -d "${DIST_DIR}/apps" ]; then
    mkdir -p "${RESOURCES_DIR}/apps"
    cp -R "${DIST_DIR}/apps"/* "${RESOURCES_DIR}/apps/" 2>/dev/null || true
    echo "==> Copied pre-bundled apps into ${RESOURCES_DIR}/apps"
fi
if [ -d "${DIST_DIR}/custom_apps" ]; then
    mkdir -p "${RESOURCES_DIR}/custom_apps"
    cp -R "${DIST_DIR}/custom_apps"/* "${RESOURCES_DIR}/custom_apps/" 2>/dev/null || true
fi
if [ -d "${DIST_DIR}/SavedFiles" ]; then
    mkdir -p "${RESOURCES_DIR}/SavedFiles"
fi

# Ensure Gatekeeper quarantine flag is cleared on all bundled binaries
xattr -cr "${APP_BUNDLE}" 2>/dev/null || true

# 3. Create Release Archives
mkdir -p "${RELEASE_DIR}"
echo "==> Creating macOS distribution packages..."

# Create ZIP archive
cd "${DIST_DIR}"
ZIP_ITEMS=("ArtCraftStudio.app")
[ -f "registry.json" ] && ZIP_ITEMS+=("registry.json")
[ -f "version.json" ] && ZIP_ITEMS+=("version.json")
[ -f "AI_연동_가이드.txt" ] && ZIP_ITEMS+=("AI_연동_가이드.txt")
[ -d "apps" ] && ZIP_ITEMS+=("apps")
[ -d "custom_apps" ] && ZIP_ITEMS+=("custom_apps")
[ -d "SavedFiles" ] && ZIP_ITEMS+=("SavedFiles")

zip -r -q "${RELEASE_DIR}/ArtCraftStudio-macos-universal.zip" "${ZIP_ITEMS[@]}"
cp -f "${RELEASE_DIR}/ArtCraftStudio-macos-universal.zip" "${RELEASE_DIR}/ArtCraftStudio-macos-x64.zip" 2>/dev/null || true

# If on macOS, build .dmg using hdiutil
if command -v hdiutil >/dev/null 2>&1; then
    DMG_TMP="${ROOT_DIR}/target/dmg_tmp"
    rm -rf "${DMG_TMP}"
    mkdir -p "${DMG_TMP}"
    cp -R "${APP_BUNDLE}" "${DMG_TMP}/"
    ln -s /Applications "${DMG_TMP}/Applications"
    hdiutil create -volname "ArtCraft Studio" -srcfolder "${DMG_TMP}" -ov -format UDZO "${RELEASE_DIR}/ArtCraftStudio-macos.dmg"
    rm -rf "${DMG_TMP}"
    echo "==> Created ${RELEASE_DIR}/ArtCraftStudio-macos.dmg"
fi

echo "==> macOS build complete! Artifacts:"
echo "    App Bundle: ${APP_BUNDLE}"
echo "    Release:    ${RELEASE_DIR}/ArtCraftStudio-macos-x64.zip"
