from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from completion_dictionary.paths import atomic_output
from completion_dictionary.swedish import write_lemma_index


class AtomicOutputTests(unittest.TestCase):
    def test_readers_see_old_contents_until_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "output.dict"
            destination.write_text("old", encoding="utf-8")
            destination.chmod(0o640)
            original_mode = destination.stat().st_mode
            with atomic_output(destination) as temporary:
                temporary.write_text("new", encoding="utf-8")
                self.assertEqual(destination.read_text(encoding="utf-8"), "old")
            self.assertEqual(destination.read_text(encoding="utf-8"), "new")
            self.assertEqual(destination.stat().st_mode, original_mode)
            self.assertEqual(list(destination.parent.iterdir()), [destination])

    def test_failed_index_write_preserves_old_contents(self) -> None:
        write_text = Path.write_text

        def interrupted_write(path: Path, text: str, **kwargs: object) -> None:
            write_text(path, text[:3], encoding="utf-8")
            raise OSError("disk full")

        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "index.tsv"
            destination.write_text("old", encoding="utf-8")
            with patch.object(Path, "write_text", interrupted_write):
                with self.assertRaisesRegex(OSError, "disk full"):
                    write_lemma_index([("forms", "base")], destination)
            self.assertEqual(destination.read_text(encoding="utf-8"), "old")
            self.assertEqual(list(destination.parent.iterdir()), [destination])

    def test_overlapping_writers_use_distinct_temporary_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "nested" / "output.dict"
            with atomic_output(destination) as first:
                first.write_text("first", encoding="utf-8")
                with atomic_output(destination) as second:
                    self.assertNotEqual(first, second)
                    second.write_text("second", encoding="utf-8")
                self.assertEqual(destination.read_text(encoding="utf-8"), "second")
            self.assertEqual(destination.read_text(encoding="utf-8"), "first")
            self.assertEqual(list(destination.parent.iterdir()), [destination])


if __name__ == "__main__":
    unittest.main()
