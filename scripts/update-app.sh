#!/bin/sh
# In-place Replacement Updater for ArtCraft apps on macOS / Linux
set -e

APP_ID="$1"
REPO_URL="$2"
DEST_DIR="$3"
SUITE_REPO="${4:-gochujangs33-dot/Craft-Studio}"

if [ -z "$APP_ID" ] || [ -z "$DEST_DIR" ]; then
    echo "Usage: update-app.sh <AppId> <RepoUrl> <DestDir> [SuiteRepo]"
    exit 1
fi

echo "PROGRESS:5:업데이트 정보 확인 중..."
mkdir -p "$DEST_DIR"

TMP_DIR="/tmp/ArtCraft_Update_${APP_ID}"
rm -rf "$TMP_DIR"
mkdir -p "$TMP_DIR"

DOWNLOAD_URL=""
LATEST_TAG="latest"

# 1. Check user suite repo releases first
if [ -n "$SUITE_REPO" ]; then
    SUITE_JSON=$(curl -k -L -s --max-time 15 -A "ArtCraftStudio-Updater" "https://api.github.com/repos/${SUITE_REPO}/releases/latest" || true)
    APP_PKG=$(echo "$SUITE_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "${APP_ID}.*(macos|darwin).*\.dmg" | head -n 1 || true)
    if [ -z "$APP_PKG" ]; then
        APP_PKG=$(echo "$SUITE_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "${APP_ID}.*(macos|darwin|universal).*\.(zip|dmg|tar\.gz)" | head -n 1 || true)
    fi
    DOWNLOAD_URL="$APP_PKG"
fi

# 2. Fallback to upstream repository releases
if [ -z "$DOWNLOAD_URL" ] && [ -n "$REPO_URL" ]; then
    REPO_PATH=$(echo "$REPO_URL" | sed 's|^https://github.com/||')
    UPSTREAM_JSON=$(curl -k -L -s --max-time 15 -A "ArtCraftStudio-Updater" "https://api.github.com/repos/${REPO_PATH}/releases/latest" || true)
    LATEST_TAG=$(echo "$UPSTREAM_JSON" | grep -o '"tag_name": *"[^"]*"' | head -n 1 | sed -E 's/.*"v?([^"]*)".*/\1/')
    
    # Prefer macOS universal DMG
    APP_PKG=$(echo "$UPSTREAM_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "(macos|darwin).*\.dmg" | head -n 1 || true)
    if [ -z "$APP_PKG" ]; then
        APP_PKG=$(echo "$UPSTREAM_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "(macos|darwin).*\.(zip|tar\.gz)" | grep -v "cli" | head -n 1 || true)
    fi
    if [ -z "$APP_PKG" ]; then
        APP_PKG=$(echo "$UPSTREAM_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "(macos|darwin).*\.(zip|dmg|tar\.gz)" | head -n 1 || true)
    fi
    DOWNLOAD_URL="$APP_PKG"
fi

if [ -z "$DOWNLOAD_URL" ]; then
    echo "GitHub에서 ${APP_ID} macOS 릴리스 패키지를 찾을 수 없습니다." >&2
    exit 2
fi

echo "PROGRESS:20:다운로드 서버 연결 중..."
ARCHIVE_PATH="${TMP_DIR}/package.archive"

# Download using curl
echo "PROGRESS:25:다운로드 중..."
curl -k -L --max-time 300 -A "ArtCraftStudio-Updater" "$DOWNLOAD_URL" -o "$ARCHIVE_PATH"
echo "PROGRESS:75:다운로드 완료"

echo "PROGRESS:80:압축 파일 해제 중..."
EXTRACT_DIR="${TMP_DIR}/extracted"
mkdir -p "$EXTRACT_DIR"

if echo "$DOWNLOAD_URL" | grep -q '\.zip$'; then
    unzip -q -o "$ARCHIVE_PATH" -d "$EXTRACT_DIR"
elif echo "$DOWNLOAD_URL" | grep -q '\.tar\.gz$'; then
    tar -xzf "$ARCHIVE_PATH" -C "$EXTRACT_DIR"
elif echo "$DOWNLOAD_URL" | grep -q '\.dmg$'; then
    # Mount DMG and copy application bundle
    MOUNT_POINT="${TMP_DIR}/dmg_mount"
    mkdir -p "$MOUNT_POINT"
    hdiutil attach "$ARCHIVE_PATH" -mountpoint "$MOUNT_POINT" -nobrowse -quiet
    cp -R "$MOUNT_POINT"/*.app "$EXTRACT_DIR"/ 2>/dev/null || cp -R "$MOUNT_POINT"/* "$EXTRACT_DIR"/ || true
    hdiutil detach "$MOUNT_POINT" -quiet || true
fi

echo "PROGRESS:90:기존 앱 안전 백업 중..."
BACKUP_DIR="/tmp/ArtCraft_Backup_${APP_ID}"
rm -rf "$BACKUP_DIR"
mkdir -p "$BACKUP_DIR"
if [ -d "$DEST_DIR" ]; then
    cp -R "$DEST_DIR"/* "$BACKUP_DIR"/ 2>/dev/null || true
fi

echo "PROGRESS:95:최신 프로그램 동일 위치 안전 교체..."
# Copy extracted content to DEST_DIR
SOURCE_DIR="$EXTRACT_DIR"
# If single directory inside and NOT a .app bundle, flatten
SUB_ITEM=$(find "$EXTRACT_DIR" -mindepth 1 -maxdepth 1 | head -n 1)
SUB_COUNT=$(find "$EXTRACT_DIR" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
if [ "$SUB_COUNT" = "1" ] && [ -d "$SUB_ITEM" ] && ! echo "$SUB_ITEM" | grep -q '\.app$'; then
    SOURCE_DIR="$SUB_ITEM"
fi

mkdir -p "$DEST_DIR"
cp -R "$SOURCE_DIR"/* "$DEST_DIR"/

# Strip Intel (x86_64) code to keep Apple Silicon (arm64) only
if command -v lipo >/dev/null 2>&1; then
    find "$DEST_DIR" -type f | while read -r f; do
        if file "$f" 2>/dev/null | grep -q "Mach-O universal binary"; then
            if lipo -info "$f" 2>/dev/null | grep -q "arm64"; then
                lipo -thin arm64 "$f" -output "${f}.arm64" 2>/dev/null && mv -f "${f}.arm64" "$f"
            fi
        fi
    done
fi

# Remove quarantine attributes and grant execute permissions
xattr -cr "$DEST_DIR" 2>/dev/null || true
chmod -R +x "$DEST_DIR" 2>/dev/null || true

printf "%s" "$LATEST_TAG" > "${DEST_DIR}/.version"

# Cleanup
rm -rf "$TMP_DIR"
rm -rf "$BACKUP_DIR"

echo "PROGRESS:100:v${LATEST_TAG} 교체 완료"
exit 0
