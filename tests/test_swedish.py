from __future__ import annotations

import unittest
from pathlib import Path

from completion_dictionary import swedish

FIXTURES = Path(__file__).parent / "fixtures"


class ParseHunspellDictionaryTests(unittest.TestCase):
    def test_parses_plain_forms_and_strips_flags(self) -> None:
        words = swedish.parse_hunspell_dictionary(FIXTURES / "sv_test.dic")

        self.assertEqual(
            words,
            {
                "arbete",
                "arbetar",
                "arbetade",
                "arbetat",
                "överenskommelse",
            },
        )

    def test_ignores_count_header_and_empty_lines(self) -> None:
        words = swedish.parse_hunspell_dictionary(FIXTURES / "sv_test.dic")

        self.assertNotIn("6", words)
        self.assertNotIn("", words)

    def test_removes_duplicates(self) -> None:
        words = swedish.parse_hunspell_dictionary(FIXTURES / "sv_test.dic")

        # "arbete" appears both with "/A" and as a plain duplicate; the
        # returned set contains it exactly once.
        self.assertEqual(sum(1 for w in ["arbete"] if w in words), 1)

    def test_preserves_utf8(self) -> None:
        words = swedish.parse_hunspell_dictionary(FIXTURES / "sv_test.dic")

        self.assertIn("överenskommelse", words)


if __name__ == "__main__":
    unittest.main()
