#!/bin/sh
# In-place Replacement Updater for ArtCraft apps on macOS / Linux

APP_ID="$1"
REPO_URL="$2"
DEST_DIR="$3"
SUITE_REPO="${4:-gochujangs33-dot/Craft-Studio}"

if [ -z "$APP_ID" ] || [ -z "$DEST_DIR" ]; then
    echo "Usage: update-app.sh <AppId> <RepoUrl> <DestDir> [SuiteRepo]"
    exit 1
fi

echo "PROGRESS:5:업데이트 정보 확인 중..."

# Ensure destination directory is writable, fallback to user Application Support if needed
if [ -n "$HOME" ]; then
    USER_APP_DIR="${HOME}/Library/Application Support/ArtCraftStudio/apps/${APP_ID}"
    if [ ! -w "$DEST_DIR" ] && [ ! -w "$(dirname "$DEST_DIR")" ]; then
        mkdir -p "$USER_APP_DIR" 2>/dev/null || true
        if [ -w "$USER_APP_DIR" ]; then
            DEST_DIR="$USER_APP_DIR"
        fi
    fi
fi
mkdir -p "$DEST_DIR" 2>/dev/null || true

TMP_DIR="/tmp/ArtCraft_Update_${APP_ID}"
rm -rf "$TMP_DIR" 2>/dev/null || true
mkdir -p "$TMP_DIR"

DOWNLOAD_URL=""
LATEST_TAG="latest"

# Tier 0: Direct manifest version.json lookup from raw.githubusercontent.com (Immune to API rate limits!)
VJSON=$(curl -k -L -s --max-time 10 -A "ArtCraftStudio-Updater" "https://raw.githubusercontent.com/${SUITE_REPO}/main/version.json" 2>/dev/null || true)
if [ -n "$VJSON" ]; then
    APP_VER=$(echo "$VJSON" | grep -A 5 "\"${APP_ID}\":" | grep -o '"version": *"[^"]*"' | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/' || true)
    if [ -n "$APP_VER" ]; then
        LATEST_TAG="$APP_VER"
        REPO_PATH=$(echo "$REPO_URL" | sed 's|^https://github.com/||')
        
        M_ARCH=$(uname -m 2>/dev/null || echo "arm64")
        CANDIDATES=(
            "https://github.com/${REPO_PATH}/releases/download/v${APP_VER}/${APP_ID}-${APP_VER}-macos-universal.dmg"
            "https://github.com/${REPO_PATH}/releases/download/v${APP_VER}/${APP_ID}-${APP_VER}-macos-${M_ARCH}.dmg"
            "https://github.com/${REPO_PATH}/releases/download/v${APP_VER}/${APP_ID}-macos-universal.dmg"
            "https://github.com/${REPO_PATH}/releases/download/v${APP_VER}/${APP_ID}-macos-${M_ARCH}.dmg"
            "https://github.com/${REPO_PATH}/releases/download/v${APP_VER}/HOP-macos-${M_ARCH}.dmg"
            "https://github.com/${REPO_PATH}/releases/download/v${APP_VER}/HOP-macos-arm64.dmg"
            "https://github.com/${REPO_PATH}/releases/latest/download/${APP_ID}-macos-universal.dmg"
            "https://github.com/${REPO_PATH}/releases/latest/download/HOP-macos-arm64.dmg"
        )
        for cand in "${CANDIDATES[@]}"; do
            HTTP_CODE=$(curl -k -s -o /dev/null -w "%{http_code}" -L --max-time 6 "$cand" 2>/dev/null || true)
            if [ "$HTTP_CODE" = "200" ]; then
                DOWNLOAD_URL="$cand"
                echo "PROGRESS:15:매니페스트 기반 직접 다운로드 채널 연결: v${LATEST_TAG}"
                break
            fi
        done
    fi
fi

# 1. Check user suite repo releases
if [ -z "$DOWNLOAD_URL" ] && [ -n "$SUITE_REPO" ]; then
    SUITE_JSON=$(curl -k -L -s --max-time 15 -A "ArtCraftStudio-Updater" "https://api.github.com/repos/${SUITE_REPO}/releases/latest" 2>/dev/null || true)
    APP_PKG=$(echo "$SUITE_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "${APP_ID}.*(macos|darwin).*\.dmg" | head -n 1 || true)
    if [ -z "$APP_PKG" ]; then
        APP_PKG=$(echo "$SUITE_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "${APP_ID}.*(macos|darwin|universal).*\.(zip|dmg|tar\.gz)" | head -n 1 || true)
    fi
    DOWNLOAD_URL="$APP_PKG"
fi

# 2. Fallback to upstream repository releases
if [ -z "$DOWNLOAD_URL" ] && [ -n "$REPO_URL" ]; then
    REPO_PATH=$(echo "$REPO_URL" | sed 's|^https://github.com/||')
    UPSTREAM_JSON=$(curl -k -L -s --max-time 15 -A "ArtCraftStudio-Updater" "https://api.github.com/repos/${REPO_PATH}/releases/latest" 2>/dev/null || true)
    LATEST_TAG=$(echo "$UPSTREAM_JSON" | grep -o '"tag_name": *"[^"]*"' | head -n 1 | sed -E 's/.*"v?([^"]*)".*/\1/')
    
    # Prefer macOS architecture-matching DMG
    M_ARCH=$(uname -m 2>/dev/null || echo "x86_64")
    if [ "$M_ARCH" = "arm64" ]; then
        APP_PKG=$(echo "$UPSTREAM_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "(arm64|aarch64).*\.dmg" | head -n 1 || true)
    fi
    if [ -z "$APP_PKG" ]; then
        APP_PKG=$(echo "$UPSTREAM_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "(macos|darwin).*\.dmg" | head -n 1 || true)
    fi
    if [ -z "$APP_PKG" ]; then
        APP_PKG=$(echo "$UPSTREAM_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "(macos|darwin).*\.(zip|tar\.gz)" | head -n 1 || true)
    fi
    if [ -z "$APP_PKG" ]; then
        APP_PKG=$(echo "$UPSTREAM_JSON" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"([^"]+)".*/\1/' | grep -iE "(macos|darwin).*\.(zip|dmg|tar\.gz)" | head -n 1 || true)
    fi
    DOWNLOAD_URL="$APP_PKG"
fi

if [ -z "$DOWNLOAD_URL" ]; then
    echo "ERROR: GitHub에서 ${APP_ID} macOS 릴리스 패키지를 찾을 수 없습니다." >&2
    exit 2
fi

echo "PROGRESS:20:다운로드 서버 연결 중..."
ARCHIVE_PATH="${TMP_DIR}/package.archive"

# Download using curl
echo "PROGRESS:25:다운로드 중..."
curl -k -L --max-time 300 -A "ArtCraftStudio-Updater" "$DOWNLOAD_URL" -o "$ARCHIVE_PATH"
if [ ! -s "$ARCHIVE_PATH" ]; then
    echo "ERROR: 패키지 다운로드 실패" >&2
    exit 3
fi
echo "PROGRESS:75:다운로드 완료"

echo "PROGRESS:80:압축 파일 해제 중..."
EXTRACT_DIR="${TMP_DIR}/extracted"
mkdir -p "$EXTRACT_DIR"

if echo "$DOWNLOAD_URL" | grep -qi '\.zip$'; then
    unzip -q -o "$ARCHIVE_PATH" -d "$EXTRACT_DIR" 2>/dev/null || true
elif echo "$DOWNLOAD_URL" | grep -qi '\.tar\.gz$'; then
    tar -xzf "$ARCHIVE_PATH" -C "$EXTRACT_DIR" 2>/dev/null || true
elif echo "$DOWNLOAD_URL" | grep -qi '\.dmg$'; then
    # Mount DMG and copy application bundle
    MOUNT_POINT="${TMP_DIR}/dmg_mount"
    mkdir -p "$MOUNT_POINT"
    hdiutil attach "$ARCHIVE_PATH" -mountpoint "$MOUNT_POINT" -nobrowse -noautoopen -noverify -quiet 2>/dev/null || true
    cp -R "$MOUNT_POINT"/*.app "$EXTRACT_DIR"/ 2>/dev/null || cp -R "$MOUNT_POINT"/* "$EXTRACT_DIR"/ 2>/dev/null || true
    hdiutil detach "$MOUNT_POINT" -force -quiet 2>/dev/null || true
fi

echo "PROGRESS:90:기존 앱 안전 백업 중..."
BACKUP_DIR="/tmp/ArtCraft_Backup_${APP_ID}"
rm -rf "$BACKUP_DIR" 2>/dev/null || true
mkdir -p "$BACKUP_DIR"
if [ -d "$DEST_DIR" ]; then
    cp -R "$DEST_DIR"/* "$BACKUP_DIR"/ 2>/dev/null || true
fi

echo "PROGRESS:95:최신 프로그램 동일 위치 안전 교체..."
SOURCE_DIR="$EXTRACT_DIR"
# If single directory inside and NOT a .app bundle, flatten
SUB_ITEM=$(find "$EXTRACT_DIR" -mindepth 1 -maxdepth 1 2>/dev/null | head -n 1)
SUB_COUNT=$(find "$EXTRACT_DIR" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l | tr -d ' ')
if [ "$SUB_COUNT" = "1" ] && [ -d "$SUB_ITEM" ] && ! echo "$SUB_ITEM" | grep -qi '\.app$'; then
    SOURCE_DIR="$SUB_ITEM"
fi

mkdir -p "$DEST_DIR" 2>/dev/null || true
# Clean old .app bundle to prevent directory nesting collision
rm -rf "$DEST_DIR"/*.app 2>/dev/null || true
cp -R "$SOURCE_DIR"/* "$DEST_DIR"/ 2>/dev/null || true

# Strip Intel (x86_64) code to keep Apple Silicon (arm64) only
if command -v lipo >/dev/null 2>&1; then
    find "$DEST_DIR" -type f 2>/dev/null | while read -r f; do
        if file "$f" 2>/dev/null | grep -q "Mach-O universal binary"; then
            if lipo -info "$f" 2>/dev/null | grep -q "arm64"; then
                lipo -thin arm64 "$f" -output "${f}.arm64" 2>/dev/null && mv -f "${f}.arm64" "$f" 2>/dev/null || true
            fi
        fi
    done
fi

# Remove quarantine attributes and grant execute permissions
xattr -cr "$DEST_DIR" 2>/dev/null || true
chmod -R +x "$DEST_DIR" 2>/dev/null || true

if [ -n "$LATEST_TAG" ] && [ "$LATEST_TAG" != "latest" ]; then
    printf "%s" "$LATEST_TAG" > "${DEST_DIR}/.version"
fi

# Cleanup
rm -rf "$TMP_DIR" 2>/dev/null || true
rm -rf "$BACKUP_DIR" 2>/dev/null || true

echo "PROGRESS:100:v${LATEST_TAG} 교체 완료"
exit 0
