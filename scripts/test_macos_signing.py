"""Check configuration and real signed fixtures without accessing the microphone."""

import json
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from verify_macos_bundle import AUDIO_INPUT, verify_bundle

ROOT = Path(__file__).resolve().parents[1]


class MicrophoneSigningConfigTests(unittest.TestCase):
    def test_mac_bundle_declares_microphone_entitlement(self):
        tauri = ROOT / "apps/desktop/src-tauri"
        base = json.loads((tauri / "tauri.conf.json").read_text())
        override = json.loads((tauri / "tauri.macos.conf.json").read_text())
        mac = {**base["bundle"]["macOS"], **override["bundle"].get("macOS", {})}
        self.assertTrue(mac.get("hardenedRuntime", True))
        self.assertIn("entitlements", mac, "Mac signing must include microphone access")
        with (tauri / mac["entitlements"]).open("rb") as source:
            self.assertIs(plistlib.load(source).get(AUDIO_INPUT), True)


@unittest.skipUnless(sys.platform == "darwin", "Real codesign fixtures require macOS")
class SignedBundleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="quill-signing-test-")
        cls.root = Path(cls.temporary.name)
        source = cls.root / "main.c"
        source.write_text("int main(void) { return 0; }\n")
        cls.binary = cls.root / "quill"
        subprocess.run(["clang", "-arch", "arm64", "-arch", "x86_64", str(source), "-o", str(cls.binary)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def fixture(self, entitlement=None, hardened=True):
        app = self.root / (self._testMethodName + ".app")
        contents = app / "Contents"
        (contents / "MacOS").mkdir(parents=True)
        shutil.copy2(self.binary, contents / "MacOS/quill")
        with (contents / "Info.plist").open("wb") as destination:
            plistlib.dump({"CFBundleIdentifier": "dev.quill.dictation", "CFBundleExecutable": "quill", "CFBundlePackageType": "APPL", "NSMicrophoneUsageDescription": "Test fixture; never launched."}, destination)
        command = ["codesign", "--force", "--sign", "-"]
        if hardened:
            command += ["--options", "runtime"]
        if entitlement is not None:
            path = self.root / (self._testMethodName + ".plist")
            path.write_bytes(plistlib.dumps({AUDIO_INPUT: entitlement}))
            command += ["--entitlements", str(path)]
        subprocess.run([*command, str(app)], check=True, capture_output=True)
        return app

    def test_rejects_valid_signature_without_microphone_entitlement(self):
        with self.assertRaisesRegex(ValueError, "missing true .*audio-input"):
            verify_bundle(self.fixture())

    def test_rejects_false_entitlement(self):
        with self.assertRaisesRegex(ValueError, "missing true .*audio-input"):
            verify_bundle(self.fixture(False))

    def test_rejects_disabling_hardened_runtime(self):
        with self.assertRaisesRegex(ValueError, "hardened runtime is not enabled"):
            verify_bundle(self.fixture(True, hardened=False))

    def test_accepts_microphone_entitlement_on_both_architectures(self):
        self.assertEqual(set(verify_bundle(self.fixture(True))), {"arm64", "x86_64"})

    def test_rejects_modified_bundle(self):
        app = self.fixture(True)
        with (app / "Contents/MacOS/quill").open("ab") as binary:
            binary.write(b"invalidates signature")
        with self.assertRaises(subprocess.CalledProcessError):
            verify_bundle(app)


if __name__ == "__main__":
    unittest.main()
