import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import uuid

spec = importlib.util.spec_from_file_location("cleanup", Path(__file__).with_name("cleanup-failed.py"))
cleanup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleanup)


class CleanupTest(unittest.TestCase):
    def test_preview_apply_and_recovery_protection(self):
        with tempfile.TemporaryDirectory() as tmp:
            scope = Path(tmp)
            home = scope / "codex"
            home.mkdir()
            original = home / "history.jsonl"
            original.write_text("keep original")
            (scope / "target.json").write_text(json.dumps({"home": str(home)}))
            (scope / "operation.lock").touch()
            folders = {}
            for state in ["Preparing", "RolledBack", "Applying", "Restoring", "Complete", "Restored"]:
                directory = scope / "operations" / ("20261102-000000-" + str(uuid.uuid4()))
                directory.mkdir(parents=True)
                (directory / "manifest.json").write_text(json.dumps({"state": state}))
                for ext in ["before", "after"]:
                    (directory / (str(uuid.uuid4()) + "." + ext)).write_bytes(b"encrypted")
                folders[state] = directory
            preview = cleanup.cleanup(scope, home)
            self.assertTrue(all(p.exists() for p in folders.values()))
            self.assertEqual(cleanup.cleanup(scope, home, True), preview)
            for state, directory in folders.items():
                if state in {"Preparing", "RolledBack", "Restored"}:
                    self.assertFalse(directory.exists())
                else:
                    self.assertEqual(len(list(directory.glob("*.before"))), 1)
                    self.assertEqual(len(list(directory.glob("*.after"))), int(state in {"Applying", "Restoring"}))
            self.assertEqual(original.read_text(), "keep original")
            self.assertEqual(cleanup.cleanup(scope, home)[0:2], (0, 0))
            self.assertEqual(cleanup.cleanup(scope, scope / "other", True), (0, 0, []))
            with (scope / "operation.lock").open("r+b") as lock:
                cleanup.fcntl.flock(lock, cleanup.fcntl.LOCK_EX | cleanup.fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    cleanup.cleanup(scope, home, True)

    def test_symlink_is_not_followed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            link = root / "link"
            link.symlink_to(target, target_is_directory=True)
            with self.assertRaises(ValueError):
                cleanup.cleanup(link, root, True)


if __name__ == "__main__":
    unittest.main()
