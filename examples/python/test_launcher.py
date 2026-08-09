import tempfile
import unittest
from pathlib import Path

from launcher import replace_installation, validate_manifest


def valid_manifest() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "version": "1.0.0",
        "files": [
            {
                "bucket": "game-updates",
                "key": "releases/1.0.0/game/update.pak",
                "path": "game/update.pak",
                "sizeBytes": 8,
                "sha256": "a" * 64,
                "order": 10,
            }
        ],
    }


class ManifestTests(unittest.TestCase):
    def test_accepts_a_safe_release(self) -> None:
        manifest = validate_manifest(valid_manifest())
        self.assertEqual(manifest["version"], "1.0.0")

    def test_rejects_parent_traversal(self) -> None:
        manifest = valid_manifest()
        manifest["files"][0]["path"] = "../outside.pak"
        with self.assertRaisesRegex(ValueError, "unsafe release path"):
            validate_manifest(manifest)

    def test_rejects_duplicate_paths(self) -> None:
        manifest = valid_manifest()
        manifest["files"].append(dict(manifest["files"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate release path"):
            validate_manifest(manifest)

    def test_replaces_an_installation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            installed = root / "installed"
            staging = root / "staging"
            installed.mkdir()
            staging.mkdir()
            (installed / "old.txt").write_text("old")
            (staging / "new.txt").write_text("new")
            replace_installation(installed, staging)
            self.assertFalse((installed / "old.txt").exists())
            self.assertEqual((installed / "new.txt").read_text(), "new")


if __name__ == "__main__":
    unittest.main()
