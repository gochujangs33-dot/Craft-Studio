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
CACHE_DIR = os.path.join(ROOT_DIR, ".cache", "downloads")

os.makedirs(CACHE_DIR, exist_ok=True)
os.makedirs(DIST_APPS, exist_ok=True)

# Complete list of apps to fetch/update:
# (app_id, repo, tag, asset_name)
APPS_TO_UPDATE = [
    # 14 Existing Apps
    ("photocraft", "storytold/photocraft", "0.20.1", "photocraft-0.20.1-windows-x64-portable.zip"),
    ("vectorcraft", "storytold/vectorcraft", "0.21.0", "vectorcraft-0.21.0-windows-x64-portable.zip"),
    ("filmcraft", "storytold/filmcraft", "0.17.0", "filmcraft-0.17.0-windows-x64-portable.zip"),
    ("lightcraft", "storytold/lightcraft", "0.18.0", "lightcraft-0.18.0-windows-x64-portable.zip"),
    ("pdfcraft", "storytold/pdfcraft", "0.18.1", "pdfcraft-0.18.1-windows-x64-portable.zip"),
    ("effectcraft", "storytold/effectcraft", "0.21.0", "effectcraft-0.21.0-windows-x64-portable.zip"),
    ("designcraft", "storytold/designcraft", "0.17.1", "designcraft-0.17.1-windows-x64-portable.zip"),
    ("wordcraft", "storytold/wordcraft", "0.15.1", "wordcraft-0.15.1-windows-x64-portable.zip"),
    ("deckcraft", "storytold/deckcraft", "0.15.0", "deckcraft-0.15.0-windows-x64-portable.zip"),
    ("gridcraft", "storytold/gridcraft", "0.16.0", "gridcraft-0.16.0-windows-x64-portable.zip"),
    ("cadcraft", "storytold/cadcraft", "0.17.0", "cadcraft-0.17.0-windows-x64-portable.zip"),
    ("solidcraft", "tangency-labs/solidcraft", "0.30.1", "solidcraft-0.30.1-windows-x64-portable.zip"),
    ("soundcraft", "storytold/soundcraft", "0.17.0", "soundcraft-0.17.0-windows-x64-portable.zip"),
    ("figcraft", "storytold/figcraft", "0.15.0", "figcraft-0.15.0-windows-x64-portable.zip"),
    # 3 New Apps
    ("texturecraft", "storytold/texturecraft", "0.3.1", "texturecraft-0.3.1-windows-x64-portable.zip"),
    ("materialcraft", "storytold/materialcraft", "0.1.1", "materialcraft-0.1.1-windows-x64-portable.zip"),
    ("craft-anything", "storytold/craft-anything", "0.3.0", "craft-anything-0.3.0-windows-x64-portable.zip"),
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
            with urllib.request.urlopen(req, context=ctx, timeout=120) as resp, open(zip_path, "wb") as out_f:
                shutil.copyfileobj(resp, out_f)
            print(f"[{app_id}] Download completed: {os.path.getsize(zip_path)} bytes")
        except Exception as e:
            alt_url = f"https://github.com/{repo}/releases/download/{tag}/{asset_name}"
            print(f"[{app_id}] Retrying with alt URL: {alt_url} ({e})")
            req = urllib.request.Request(alt_url, headers={"User-Agent": "ArtCraftStudio-Updater"})
            with urllib.request.urlopen(req, context=ctx, timeout=120) as resp, open(zip_path, "wb") as out_f:
                shutil.copyfileobj(resp, out_f)
            print(f"[{app_id}] Download completed: {os.path.getsize(zip_path)} bytes")
    else:
        print(f"[{app_id}] Using cached {asset_name} ({os.path.getsize(zip_path)} bytes)")

    # 2. Extract
    dest_dir = os.path.join(DIST_APPS, app_id)
    tmp_extract = os.path.join(CACHE_DIR, f"_x_{app_id}")
    if os.path.exists(tmp_extract):
        shutil.rmtree(tmp_extract, ignore_errors=True)
    os.makedirs(tmp_extract, exist_ok=True)
    
    print(f"[{app_id}] Extracting...")
    with zipfile.ZipFile(zip_path, "r") as z:
        z.extractall(tmp_extract)
        
    entries = os.listdir(tmp_extract)
    if os.path.exists(dest_dir):
        shutil.rmtree(dest_dir, ignore_errors=True)
        
    if len(entries) == 1 and os.path.isdir(os.path.join(tmp_extract, entries[0])):
        inner = os.path.join(tmp_extract, entries[0])
        shutil.move(inner, dest_dir)
        shutil.rmtree(tmp_extract, ignore_errors=True)
    else:
        shutil.move(tmp_extract, dest_dir)
        
    # Write .version
    ver_path = os.path.join(dest_dir, ".version")
    with open(ver_path, "w", encoding="utf-8") as f:
        f.write(tag)
        
    print(f"[{app_id}] Extracted & updated to v{tag} successfully.")
    return app_id, tag, asset_name

if __name__ == "__main__":
    print(f"=== Starting parallel download and extraction of {len(APPS_TO_UPDATE)} apps ===")
    results = []
    with ThreadPoolExecutor(max_workers=4) as executor:
        futures = [executor.submit(download_and_extract, item) for item in APPS_TO_UPDATE]
        for fut in futures:
            try:
                res = fut.result()
                results.append(res)
            except Exception as e:
                print(f"ERROR: {e}")
                sys.exit(1)

    print(f"\nAll {len(results)} apps successfully downloaded and extracted into dist/apps/!")

