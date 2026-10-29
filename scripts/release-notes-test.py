#!/usr/bin/env python3
"""Exercise the real CLI with an isolated Git fixture under the lane target/."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("release-notes.py").resolve()
ROOT = SCRIPT.parent.parent


class ReleaseNotesTest(unittest.TestCase):
    def test_release_and_rollback(self):
        (ROOT / "target").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT / "target") as directory:
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], text=True).strip()

            git("init", "-q")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            git("commit", "--allow-empty", "-qm", "Previous release")
            start = git("rev-parse", "HEAD")
            git("commit", "--allow-empty", "-qm", "Handle empty inputs\n\nTodo: fixture-42\nTodo: fixture-43")
            change = git("rev-parse", "HEAD")
            git("commit", "--allow-empty", "-qm", 'Revert "Handle empty inputs"\n\nTodo: fixture-rollback')
            end = git("rev-parse", "HEAD")
            changelog = Path(directory) / "notes.md"
            original = "# Changelog\n\n## [Unreleased]\n\n## [1.0.0] - 2026-01-01\n\n- Old release\n"
            changelog.write_text(original)
            command = [os.sys.executable, str(SCRIPT), "--since", start, "--until", end,
                       "--version", "1.0.1", "--date", "2026-10-29", "--changelog", str(changelog)]
            result = subprocess.check_output(command, cwd=directory, text=True)
            expected = ('## [1.0.1] - 2026-10-29\n\n### Changed\n\n'
                        f'- Handle empty inputs (`{change}`)\n  Todo: fixture-42\n  Todo: fixture-43\n'
                        f'- Revert "Handle empty inputs" (`{end}`)\n  Todo: fixture-rollback\n')
            self.assertEqual(result, original.replace("## [Unreleased]\n", "## [Unreleased]\n\n" + expected))
            self.assertEqual(changelog.read_text(), original)
            self.assertEqual(subprocess.check_output(command, cwd=directory, text=True), result)
            changelog.write_text(result)
            self.assertNotEqual(subprocess.run(command, cwd=directory, capture_output=True).returncode, 0)
            changelog.write_text(original)
            for flag, value in [("--since", end), ("--until", "--all"), ("--date", "2026-02-30")]:
                invalid = command.copy()
                invalid[invalid.index(flag) + 1] = value
                self.assertNotEqual(subprocess.run(invalid, cwd=directory, capture_output=True).returncode, 0)


if __name__ == "__main__":
    unittest.main()
