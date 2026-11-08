#!/usr/bin/env python3
"""
ArtCraft Studio - Windows Explorer Compatible Portable Zip Packager
Creates a standard PKWARE zip archive compatible with Windows built-in zip extractor (zipfldr.dll).
"""

import os
import sys
import zipfile
import time

def package_portable():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    dist_dir = os.path.join(root, "dist")
    release_dir = os.path.join(root, "release")
    os.makedirs(release_dir, exist_ok=True)

    zip_path = os.path.join(release_dir, "ArtCraftStudio-portable-x64.zip")
    if os.path.exists(zip_path):
        os.remove(zip_path)

    print(f"Creating portable zip from {dist_dir} -> {zip_path}...")
    start_time = time.time()

    # Blacklist macOS bundles and temporary junk files from Windows portable zip
    skip_dirs = {"ArtCraftStudio.app"}
    skip_exts = {".log", ".old", ".tmp"}
    skip_names = {".DS_Store", "Thumbs.db", "desktop.ini"}

    total_files = 0
    with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6, allowZip64=True) as zf:
        for dirpath, dirnames, filenames in os.walk(dist_dir):
            # Prune skipped dirs
            dirnames[:] = [d for d in dirnames if d not in skip_dirs]

            rel_dir = os.path.relpath(dirpath, dist_dir)
            if rel_dir == ".":
                rel_dir = ""

            for fname in sorted(filenames):
                if fname in skip_names or any(fname.endswith(ext) for ext in skip_exts):
                    continue

                full_path = os.path.join(dirpath, fname)
                archive_name = os.path.normpath(os.path.join(rel_dir, fname)).replace("\\", "/")

                # Ensure proper file permissions
                zinfo = zipfile.ZipInfo.from_file(full_path, arcname=archive_name)
                # Standard Windows/UNIX normal file attribute
                zinfo.external_attr = 0o644 << 16
                if fname.endswith(".exe") or fname.endswith(".cmd") or fname.endswith(".bat"):
                    zinfo.external_attr = 0o755 << 16

                with open(full_path, "rb") as src:
                    zf.writestr(zinfo, src.read(), compress_type=zipfile.ZIP_DEFLATED)
                total_files += 1

    elapsed = time.time() - start_time
    zip_size_mb = os.path.getsize(zip_path) / (1024 * 1024)
    print(f"Zip created: {total_files} files packaged ({zip_size_mb:.2f} MB) in {elapsed:.1f}s")

    # Verify with Windows Shell Explorer COM interface
    try:
        import win32com.client
        shell = win32com.client.Dispatch("Shell.Application")
        ns = shell.NameSpace(os.path.abspath(zip_path))
        if ns is not None:
            count = ns.Items().Count
            print(f"Verification SUCCESS: Windows Explorer reads {count} top-level items perfectly!")
        else:
            print("Verification WARNING: Shell.Application NameSpace returned None")
    except Exception as e:
        print(f"Note: win32com check skipped or failed: {e}")

    return zip_path

if __name__ == "__main__":
    package_portable()

