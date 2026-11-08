import hashlib
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import prepare_windows_bundler as bundler


class BundlerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.payload = b"verified fixture"
        self.asset = ("owner/repository", 123, hashlib.sha256(self.payload).hexdigest())

    def request(self, *args, stdout, **kwargs):
        stdout.write(self.payload)

    @patch.object(bundler.time, "sleep")
    def test_verified_download_retries_transport_failure(self, sleep):
        with patch.object(bundler.subprocess, "run") as run:
            def request(*args, **kwargs):
                if run.call_count == 1:
                    raise subprocess.CalledProcessError(1, "gh")
                self.request(*args, **kwargs)
            run.side_effect = request
            destination = self.root / "asset"
            bundler.download_verified(self.asset, destination)
            self.assertEqual(destination.read_bytes(), self.payload)
            self.assertEqual(run.call_count, 2)
            sleep.assert_called_once_with(5)
            self.assertEqual(run.call_args.args[0], [
                "gh", "api", "repos/owner/repository/releases/assets/123",
                "-H", "Accept: application/octet-stream"])
            self.assertEqual(run.call_args.kwargs["timeout"], 180)

    @patch.object(bundler.time, "sleep")
    def test_failure_is_bounded_and_removes_partial_download(self, sleep):
        destination = self.root / "asset"
        def timeout(*args, stdout, **kwargs):
            stdout.write(b"partial")
            raise subprocess.TimeoutExpired("gh", 180)
        with patch.object(bundler.subprocess, "run", side_effect=timeout) as run:
            with self.assertRaises(subprocess.TimeoutExpired):
                bundler.download_verified(self.asset, destination)
            self.assertEqual(run.call_count, 3)
        self.assertFalse(destination.exists())
        self.assertEqual(sleep.call_count, 2)

    def test_checksum_mismatch_fails_without_retry(self):
        destination = self.root / "asset"
        with patch.object(bundler.subprocess, "run", side_effect=self.request) as run:
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                bundler.download_verified(("owner/repository", 123, "0" * 64), destination)
            self.assertEqual(run.call_count, 1)
        self.assertFalse(destination.exists())

    def test_archive_cannot_escape_destination(self):
        archive = self.root / "bad.zip"
        with zipfile.ZipFile(archive, "w") as output:
            output.writestr("../escaped.exe", b"untrusted")
        with self.assertRaisesRegex(ValueError, "escapes"):
            bundler.extract_verified_archive(archive, self.root / "tools")
        self.assertFalse((self.root / "escaped.exe").exists())

    def fixture(self, asset, destination):
        name = next(name for name, candidate in bundler.ASSETS.items() if candidate == asset)
        if name == "plugin":
            destination.write_bytes(b"plugin")
        else:
            with zipfile.ZipFile(destination, "w") as output:
                files = bundler.WIX_FILES if name == "wix" else (
                    "nsis-3.11/makensis.exe", "nsis-3.11/Bin/makensis.exe",
                    "nsis-3.11/Include/MUI2.nsh")
                for filename in files:
                    output.writestr(filename, b"fixture")

    def test_prepares_cache_layout_without_executing_tools(self):
        cache = self.root / "cache"
        with patch.object(bundler, "download_verified", side_effect=self.fixture):
            bundler.prepare(cache)
        self.assertTrue((cache / "WixTools314/candle.exe").is_file())
        self.assertEqual((cache / "NSIS" / bundler.PLUGIN_PATH).read_bytes(), b"plugin")
        self.assertEqual(sorted(path.name for path in cache.iterdir()), ["NSIS", "WixTools314"])

    def test_invalid_archive_preserves_existing_cache(self):
        cache = self.root / "cache"
        old = cache / "WixTools314/old.exe"
        old.parent.mkdir(parents=True)
        old.write_bytes(b"existing")
        def empty_archive(asset, destination):
            with zipfile.ZipFile(destination, "w"):
                pass
        with patch.object(bundler, "download_verified", side_effect=empty_archive):
            with self.assertRaisesRegex(ValueError, "WiX archive is missing"):
                bundler.prepare(cache)
        self.assertEqual(old.read_bytes(), b"existing")
        self.assertEqual(list(cache.iterdir()), [old.parent])


if __name__ == "__main__":
    unittest.main()
