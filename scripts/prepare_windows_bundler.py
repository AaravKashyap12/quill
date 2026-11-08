"""Prepare Tauri 2.11.4's existing Windows bundler tools from official GitHub assets.

WiX checksum: tauri-apps/tauri, tauri-cli-v2.11.4,
crates/tauri-bundler/src/bundle/windows/msi/mod.rs.
NSIS SHA-256 values: the corresponding official GitHub release asset digests.
The normal Tauri bundler still checks its own required files/plugin hashes.
"""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import zipfile

ASSETS = {
    "wix": ("wixtoolset/wix3", 158158052,
            "6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31"),
    "nsis": ("tauri-apps/binary-releases", 317051073,
             "c7d27f780ddb6cffb4730138cd1591e841f4b7edb155856901cdf5f214394fa1"),
    "plugin": ("tauri-apps/nsis-tauri-utils", 345882097,
               "5ba143b5db4a87d32d6e7802e033330aae56cbceabe0d1e3ba41948385ad4709"),
}
WIX_FILES = ("candle.exe", "candle.exe.config", "darice.cub", "light.exe",
             "light.exe.config", "wconsole.dll", "winterop.dll", "wix.dll",
             "WixUIExtension.dll", "WixUtilExtension.dll")
PLUGIN_PATH = Path("Plugins/x86-unicode/additional/nsis_tauri_utils.dll")


def download_verified(asset, destination):
    repository, asset_id, expected = asset
    for attempt in range(3):
        try:
            with destination.open("wb") as output:
                subprocess.run(
                    ["gh", "api", f"repos/{repository}/releases/assets/{asset_id}",
                     "-H", "Accept: application/octet-stream"],
                    stdout=output, check=True, timeout=180,
                )
            break
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
            destination.unlink(missing_ok=True)
            if attempt == 2:
                raise
            time.sleep(5 * (attempt + 1))
    with destination.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    if actual != expected:
        destination.unlink(missing_ok=True)
        raise ValueError(f"SHA-256 mismatch for official asset {repository}/{asset_id}")
    print(f"Verified official asset {repository}/{asset_id}: {actual}")


def extract_verified_archive(archive, destination):
    with zipfile.ZipFile(archive) as source:
        root = destination.resolve()
        for entry in source.infolist():
            target = (root / entry.filename).resolve()
            if not target.is_relative_to(root):
                raise ValueError("Archive entry escapes the tool directory")
        source.extractall(destination)


def prepare(cache):
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="quill-bundler-", dir=cache) as temporary:
        staging = Path(temporary)
        for name, asset in ASSETS.items():
            download_verified(asset, staging / name)
        wix = staging / "WixTools314"
        extract_verified_archive(staging / "wix", wix)
        extract_verified_archive(staging / "nsis", staging / "nsis-unpacked")
        nsis = staging / "nsis-unpacked/nsis-3.11"
        for filename in WIX_FILES:
            if not (wix / filename).is_file():
                raise ValueError(f"WiX archive is missing {filename}")
        for filename in ("makensis.exe", "Bin/makensis.exe", "Include/MUI2.nsh"):
            if not (nsis / filename).is_file():
                raise ValueError(f"NSIS archive is missing {filename}")
        plugin = nsis / PLUGIN_PATH
        plugin.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(staging / "plugin", plugin)
        for source in (wix, nsis):
            destination = cache / ("WixTools314" if source == wix else "NSIS")
            if destination.exists():
                shutil.rmtree(destination)
            source.rename(destination)
    print(f"Prepared pinned Windows bundler tools in {cache}")


if __name__ == "__main__":
    if os.name != "nt":
        raise SystemExit("Run this helper only on Windows")
    prepare(Path(os.environ["LOCALAPPDATA"]) / "tauri")
