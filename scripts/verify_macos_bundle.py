"""Verify Quill's complete signature and hardened-runtime microphone access."""

import argparse
import plistlib
import subprocess
from pathlib import Path

AUDIO_INPUT = "com.apple.security.device.audio-input"


def verify_bundle(app: Path) -> list[str]:
    subprocess.run(
        ["codesign", "--verify", "--deep", "--strict", "--all-architectures", str(app)],
        check=True,
    )
    with (app / "Contents/Info.plist").open("rb") as source:
        info = plistlib.load(source)
    if info.get("CFBundleIdentifier") != "dev.quill.dictation":
        raise ValueError("Expected Quill's bundle identifier")
    if not info.get("NSMicrophoneUsageDescription"):
        raise ValueError("Missing microphone usage description")
    executable = app / "Contents/MacOS" / info["CFBundleExecutable"]
    architectures = subprocess.check_output(["lipo", "-archs", str(executable)], text=True).split()
    if not architectures:
        raise ValueError("No executable architectures found")
    for architecture in architectures:
        details = subprocess.run(
            ["codesign", "-d", "--arch", architecture, "--verbose=4", str(app)],
            capture_output=True, text=True, check=True,
        ).stderr
        # Read the CodeDirectory flags, not the unrelated Runtime Version line.
        directory = next((line for line in details.splitlines() if line.startswith("CodeDirectory ")), "")
        if "runtime" not in directory:
            raise ValueError(f"{architecture}: hardened runtime is not enabled")
        encoded = subprocess.check_output(
            ["codesign", "-d", "--arch", architecture, "--entitlements", "-", "--xml", str(app)],
            stderr=subprocess.DEVNULL,
        )
        entitlements = plistlib.loads(encoded) if encoded else {}
        if entitlements.get(AUDIO_INPUT) is not True:
            raise ValueError(f"{architecture}: missing true {AUDIO_INPUT} entitlement")
        print(f"Verified {architecture}: hardened runtime and microphone entitlement")
    return architectures


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    args = parser.parse_args()
    try:
        verify_bundle(args.app)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Mac bundle verification failed: {error}\n")
