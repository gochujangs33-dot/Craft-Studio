#!/usr/bin/env bash
# ==============================================================================
# Downloads official macOS builds of every Craft app in registry.json
# extracts them into dist/apps/<id>/
# ==============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
REGISTRY="${ROOT_DIR}/registry.json"
APPS_DIR="${ROOT_DIR}/dist/apps"
CACHE_DIR="${ROOT_DIR}/.cache/downloads_macos"

mkdir -p "${APPS_DIR}" "${CACHE_DIR}"

APP_IDS=$(grep -o '"id": *"[^"]*"' "${REGISTRY}" | sed -E 's/.*"([^"]+)".*/\1/')

AUTH_HEADER=()
if [ -n "${GITHUB_TOKEN:-}" ]; then
    AUTH_HEADER=(-H "Authorization: token ${GITHUB_TOKEN}")
fi

for id in $APP_IDS; do
    DEST="${APPS_DIR}/${id}"
    echo "==> Checking ${id} for macOS..."

    REPO_URL=$(grep -A 15 "\"id\": *\"${id}\"" "${REGISTRY}" | grep '"repo":' | head -n 1 | sed -E 's/.*"https:\/\/github.com\/([^"]+)".*/\1/')
    if [ -z "${REPO_URL}" ]; then
        echo "    [skip] No repository URL found for ${id}"
        continue
    fi

    # 1. Try checking suite repository releases first
    SUITE_REPO="gochujangs33-dot/Craft-Studio"
    DOWNLOAD_URL=""
    TAG_NAME=""

    SUITE_JSON=$(curl -sL -H "User-Agent: ArtCraftStudio" "${AUTH_HEADER[@]}" "https://api.github.com/repos/${SUITE_REPO}/releases/latest" 2>/dev/null || true)
    if [ -n "${SUITE_JSON}" ]; then
        DOWNLOAD_URL=$(echo "${SUITE_JSON}" | grep -o '"browser_download_url": *"[^"]*"' | grep -iE "${id}.*(macos|darwin).*\.dmg" | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/' || true)
        if [ -n "${DOWNLOAD_URL}" ]; then
            TAG_NAME=$(echo "${SUITE_JSON}" | grep -o '"tag_name": *"[^"]*"' | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/')
        fi
    fi

    # 2. Fallback to upstream repository releases
    if [ -z "${DOWNLOAD_URL}" ]; then
        REL_JSON=$(curl -sL -H "User-Agent: ArtCraftStudio" "${AUTH_HEADER[@]}" "https://api.github.com/repos/${REPO_URL}/releases/latest" 2>/dev/null || true)
        if [ -z "${REL_JSON}" ]; then
            echo "    [warn] Failed to query releases for ${REPO_URL}"
            continue
        fi

        TAG_NAME=$(echo "${REL_JSON}" | grep -o '"tag_name": *"[^"]*"' | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/')

        # Prefer macOS application DMG
        DOWNLOAD_URL=$(echo "${REL_JSON}" | grep -o '"browser_download_url": *"[^"]*"' | grep -iE "(macos|darwin).*\.dmg" | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/' || true)
        if [ -z "${DOWNLOAD_URL}" ]; then
            DOWNLOAD_URL=$(echo "${REL_JSON}" | grep -o '"browser_download_url": *"[^"]*"' | grep -iE "(macos|darwin).*\.(zip|tar\.gz)" | grep -v "cli" | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/' || true)
        fi
        if [ -z "${DOWNLOAD_URL}" ]; then
            DOWNLOAD_URL=$(echo "${REL_JSON}" | grep -o '"browser_download_url": *"[^"]*"' | grep -iE "(macos|darwin).*\.(zip|dmg|tar\.gz)" | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/' || true)
        fi
    fi

    CLI_URL=""
    if [ -n "${REL_JSON:-}" ]; then
        CLI_URL=$(echo "${REL_JSON}" | grep -o '"browser_download_url": *"[^"]*"' | grep -iE "${id}-cli.*(macos|darwin).*\.zip" | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/' || true)
    fi

    if [ -z "${DOWNLOAD_URL}" ] && [ -z "${CLI_URL}" ]; then
        echo "    [miss] No macOS asset found for ${id} (release ${TAG_NAME})"
        continue
    fi

    mkdir -p "${DEST}"

    # Download and extract CLI if found
    if [ -n "${CLI_URL}" ]; then
        CLI_ZIP="${CACHE_DIR}/$(basename "${CLI_URL}")"
        if [ ! -f "${CLI_ZIP}" ]; then
            echo "    Downloading CLI: $(basename "${CLI_URL}")..."
            curl -sL -H "User-Agent: ArtCraftStudio" "${AUTH_HEADER[@]}" "${CLI_URL}" -o "${CLI_ZIP}"
        fi
        unzip -q -o "${CLI_ZIP}" -d "${DEST}" 2>/dev/null || true
    fi

    # Download and extract app package if found
    if [ -n "${DOWNLOAD_URL}" ]; then
        PKG_FILE="${CACHE_DIR}/$(basename "${DOWNLOAD_URL}")"
        if [ ! -f "${PKG_FILE}" ]; then
            echo "    Downloading App: $(basename "${DOWNLOAD_URL}")..."
            curl -sL -H "User-Agent: ArtCraftStudio" "${AUTH_HEADER[@]}" "${DOWNLOAD_URL}" -o "${PKG_FILE}"
        fi

        case "${PKG_FILE}" in
            *.zip)
                unzip -q -o "${PKG_FILE}" -d "${DEST}" 2>/dev/null || true
                ;;
            *.dmg)
                if command -v hdiutil >/dev/null 2>&1; then
                    MOUNT_POINT="/tmp/artcraft_mount_${id}"
                    rm -rf "${MOUNT_POINT}"
                    mkdir -p "${MOUNT_POINT}"
                    hdiutil attach -quiet -nobrowse -mountpoint "${MOUNT_POINT}" "${PKG_FILE}" || true
                    if [ -d "${MOUNT_POINT}" ]; then
                        cp -R "${MOUNT_POINT}"/*.app "${DEST}/" 2>/dev/null || cp -R "${MOUNT_POINT}"/* "${DEST}/" || true
                        hdiutil detach -quiet "${MOUNT_POINT}" 2>/dev/null || true
                    fi
                    rm -rf "${MOUNT_POINT}"
                fi
                ;;
            *.tar.gz)
                tar -xzf "${PKG_FILE}" -C "${DEST}" 2>/dev/null || true
                ;;
        esac
    fi

    # Strip Intel (x86_64) code to keep Apple Silicon (arm64) only
    if command -v lipo >/dev/null 2>&1; then
        find "${DEST}" -type f | while read -r f; do
            if file "$f" 2>/dev/null | grep -q "Mach-O universal binary"; then
                if lipo -info "$f" 2>/dev/null | grep -q "arm64"; then
                    lipo -thin arm64 "$f" -output "${f}.arm64" 2>/dev/null && mv -f "${f}.arm64" "$f"
                fi
            fi
        done
    fi

    # Clear quarantine attributes and make binaries executable
    xattr -cr "${DEST}" 2>/dev/null || true
    find "${DEST}" -maxdepth 3 -type f -perm -111 -exec chmod +x {} + 2>/dev/null || true
    find "${DEST}" -maxdepth 3 -name "*-cli" -exec chmod +x {} + 2>/dev/null || true

    # Record version
    echo "${TAG_NAME#v}" > "${DEST}/.version"
    echo "    [ ok ] ${id} ${TAG_NAME} (arm64) installed to ${DEST}"
done

echo "==> macOS apps fetch completed."
