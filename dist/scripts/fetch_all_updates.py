import os
import sys
import json
import ssl
import shutil
import zipfile
import urllib.request
from concurrent.futures import ThreadPoolExecutor

ctx = ssl._create_unverified_context()

ROOT_DIR = r"c:\Users\0op64\.gemini\antigravity\scratch\ArtCraft"
DIST_APPS = os.path.join(ROOT_DIR, "dist", "apps")
LOCAL_APPS = os.path.join(ROOT_DIR, "apps")
CACHE_DIR = os.path.join(ROOT_DIR, ".cache", "downloads")

os.makedirs(CACHE_DIR, exist_ok=True)
os.makedirs(DIST_APPS, exist_ok=True)
os.makedirs(LOCAL_APPS, exist_ok=True)

# 17 Craft Apps with latest releases (Updated 2026-11-06 upstream)
APPS_TO_UPDATE = [
    ("photocraft", "storytold/photocraft", "0.21.0", "photocraft-0.21.0-windows-x64-portable.zip"),
    ("vectorcraft", "storytold/vectorcraft", "0.22.0", "vectorcraft-0.22.0-windows-x64-portable.zip"),
    ("filmcraft", "storytold/filmcraft", "0.18.0", "filmcraft-0.18.0-windows-x64-portable.zip"),
    ("lightcraft", "storytold/lightcraft", "0.19.0", "lightcraft-0.19.0-windows-x64-portable.zip"),
    ("pdfcraft", "storytold/pdfcraft", "0.19.0", "pdfcraft-0.19.0-windows-x64-portable.zip"),
    ("effectcraft", "storytold/effectcraft", "0.22.0", "effectcraft-0.22.0-windows-x64-portable.zip"),
    ("designcraft", "storytold/designcraft", "0.18.0", "designcraft-0.18.0-windows-x64-portable.zip"),
    ("wordcraft", "storytold/wordcraft", "0.16.0", "wordcraft-0.16.0-windows-x64-portable.zip"),
    ("deckcraft", "storytold/deckcraft", "0.16.0", "deckcraft-0.16.0-windows-x64-portable.zip"),
    ("gridcraft", "storytold/gridcraft", "0.17.0", "gridcraft-0.17.0-windows-x64-portable.zip"),
    ("cadcraft", "storytold/cadcraft", "0.18.0", "cadcraft-0.18.0-windows-x64-portable.zip"),
    ("solidcraft", "tangency-labs/solidcraft", "0.30.1", "solidcraft-0.30.1-windows-x64-portable.zip"),
    ("soundcraft", "storytold/soundcraft", "0.18.0", "soundcraft-0.18.0-windows-x64-portable.zip"),
    ("figcraft", "storytold/figcraft", "0.16.0", "figcraft-0.16.0-windows-x64-portable.zip"),
    ("texturecraft", "storytold/texturecraft", "0.4.0", "texturecraft-0.4.0-windows-x64-portable.zip"),
    ("materialcraft", "storytold/materialcraft", "0.2.0", "materialcraft-0.2.0-windows-x64-portable.zip"),
    ("craft-anything", "storytold/craft-anything", "0.5.0", "craft-anything-0.5.0-windows-x64-portable.zip"),
]

def download_and_extract(item):
    app_id, repo, tag, asset_name = item
    zip_path = os.path.join(CACHE_DIR, asset_name)
    download_url = f"https://github.com/{repo}/releases/download/v{tag}/{asset_name}"
    
    # 1. Download
    if not os.path.exists(zip_path) or os.path.getsize(zip_path) < 10000:
        print(f"[{app_id}] Downloading {asset_name} from {download_url}...")
        try:
            req = urllib.request.Request(download_url, headers={"User-Agent": "ArtCraftStudio-Updater"})
            with urllib.request.urlopen(req, context=ctx, timeout=180) as resp, open(zip_path, "wb") as out_f:
                shutil.copyfileobj(resp, out_f)
            print(f"[{app_id}] Download completed: {os.path.getsize(zip_path)} bytes")
        except Exception as e:
            alt_url = f"https://github.com/{repo}/releases/download/{tag}/{asset_name}"
            print(f"[{app_id}] Retrying with alt URL: {alt_url} ({e})")
            req = urllib.request.Request(alt_url, headers={"User-Agent": "ArtCraftStudio-Updater"})
            with urllib.request.urlopen(req, context=ctx, timeout=180) as resp, open(zip_path, "wb") as out_f:
                shutil.copyfileobj(resp, out_f)
            print(f"[{app_id}] Download completed: {os.path.getsize(zip_path)} bytes")
    else:
        print(f"[{app_id}] Using cached {asset_name} ({os.path.getsize(zip_path)} bytes)")

    # 2. Extract into dist/apps and apps
    tmp_extract = os.path.join(CACHE_DIR, f"_x_{app_id}")
    if os.path.exists(tmp_extract):
        shutil.rmtree(tmp_extract, ignore_errors=True)
    os.makedirs(tmp_extract, exist_ok=True)
    
    with zipfile.ZipFile(zip_path, "r") as z:
        z.extractall(tmp_extract)
        
    entries = os.listdir(tmp_extract)
    extracted_source = tmp_extract
    if len(entries) == 1 and os.path.isdir(os.path.join(tmp_extract, entries[0])):
        extracted_source = os.path.join(tmp_extract, entries[0])

    for target_base in [DIST_APPS, LOCAL_APPS]:
        dest_dir = os.path.join(target_base, app_id)
        if os.path.exists(dest_dir):
            shutil.rmtree(dest_dir, ignore_errors=True)
        shutil.copytree(extracted_source, dest_dir)
        ver_path = os.path.join(dest_dir, ".version")
        with open(ver_path, "w", encoding="utf-8") as f:
            f.write(tag)

    shutil.rmtree(tmp_extract, ignore_errors=True)
    print(f"[{app_id}] Synced to v{tag} successfully.")
    return app_id, tag, asset_name

if __name__ == "__main__":
    print(f"=== Starting parallel download and extraction of {len(APPS_TO_UPDATE)} apps ===")
    with ThreadPoolExecutor(max_workers=4) as executor:
        futures = list(executor.map(download_and_extract, APPS_TO_UPDATE))
    print(f"=== All {len(APPS_TO_UPDATE)} apps successfully updated! ===")
