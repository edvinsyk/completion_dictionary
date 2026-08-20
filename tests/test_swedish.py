from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

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


class EnsureDownloadTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temp = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp.cleanup)
        self._data = Path(self._temp.name)
        env = {"XDG_DATA_HOME": str(self._data)}
        self._data_patch = patch.dict("os.environ", env, clear=True)
        self._data_patch.start()
        self.addCleanup(self._data_patch.stop)

    def _source_dir(self, name: str) -> Path:
        return self._data / "completion-dictionary" / "sources" / name

    def test_downloads_hunspell_source_to_cached_location(self) -> None:
        payload = b"arbete\narbetar\n"

        class FakeResponse:
            def __init__(self, body: bytes) -> None:
                self._body = body

            def __enter__(self) -> "FakeResponse":
                return self

            def __exit__(self, *args: object) -> None:
                return None

            def read(self) -> bytes:
                return self._body

        destination = self._source_dir("hunspell-sv") / "sv_SE_expanded.dic"
        self.assertFalse(destination.exists())

        with patch(
            "completion_dictionary.swedish.urlopen",
            return_value=FakeResponse(payload),
        ) as urlopen:
            result = swedish.ensure_hunspell_source()

        self.assertEqual(result, destination)
        self.assertEqual(destination.read_bytes(), payload)
        urlopen.assert_called_once_with(swedish.SWEDISH_HUNSPELL_URL)
        # Atomic write leaves no temporary file behind.
        self.assertFalse(destination.with_suffix(destination.suffix + ".tmp").exists())

    def test_skips_download_when_destination_already_exists(self) -> None:
        destination = self._source_dir("mythes-sv") / "th_sv_SE.dat"
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(b"cached")

        with patch("completion_dictionary.swedish.urlopen") as urlopen:
            result = swedish.ensure_thesaurus_source()

        self.assertEqual(result, destination)
        self.assertEqual(destination.read_bytes(), b"cached")
        urlopen.assert_not_called()

    def test_exposes_pinned_source_constants(self) -> None:
        self.assertTrue(swedish.SWEDISH_HUNSPELL_URL.endswith("sv_SE_expanded.dic"))
        self.assertTrue(swedish.MYTHES_URL.endswith("th_sv_SE.dat"))


class ParseThesaurusTests(unittest.TestCase):
    def test_finds_exact_word(self) -> None:
        thesaurus = swedish.parse_thesaurus(FIXTURES / "th_sv_test.dat")

        self.assertIn("betydelsefull", thesaurus)
        self.assertEqual(
            thesaurus["betydelsefull"],
            (("mäktig", "inflytelserik", "viktigt", "signifikant", "väsentlig", "viktig", "betydande"),),
        )

    def test_handles_swedish_characters(self) -> None:
        thesaurus = swedish.parse_thesaurus(FIXTURES / "th_sv_test.dat")

        self.assertIn("hälsa", thesaurus)
        self.assertEqual(thesaurus["hälsa"], (("hälsning", "hälsande"),))
        self.assertIn("överenskommelse", thesaurus)
        self.assertEqual(thesaurus["överenskommelse"], (("avtal",),))

    def test_preserves_synonym_groups(self) -> None:
        thesaurus = swedish.parse_thesaurus(FIXTURES / "th_sv_test.dat")

        self.assertEqual(
            thesaurus["flitig"],
            (("ivrig", "ambitiös"), ("arbetsam", "strävsam")),
        )

    def test_unknown_word_absent(self) -> None:
        thesaurus = swedish.parse_thesaurus(FIXTURES / "th_sv_test.dat")

        self.assertNotIn("finnsintte", thesaurus)


class LookupSynonymsTests(unittest.TestCase):
    def test_returns_groups_for_exact_word(self) -> None:
        groups = swedish.lookup_synonyms("betydelsefull", FIXTURES / "th_sv_test.dat")

        self.assertEqual(
            groups,
            (("mäktig", "inflytelserik", "viktigt", "signifikant", "väsentlig", "viktig", "betydande"),),
        )

    def test_returns_none_for_unknown_word(self) -> None:
        self.assertIsNone(swedish.lookup_synonyms("finnsintte", FIXTURES / "th_sv_test.dat"))


class LookupWordTests(unittest.TestCase):
    def test_reports_completion_and_thesaurus(self) -> None:
        in_dictionary, groups = swedish.lookup_word(
            "överenskommelse",
            hunspell_path=FIXTURES / "sv_test.dic",
            thesaurus_path=FIXTURES / "th_sv_test.dat",
        )

        self.assertTrue(in_dictionary)
        self.assertEqual(groups, (("avtal",),))

    def test_reports_in_dictionary_but_no_thesaurus(self) -> None:
        in_dictionary, groups = swedish.lookup_word(
            "arbete",
            hunspell_path=FIXTURES / "sv_test.dic",
            thesaurus_path=FIXTURES / "th_sv_test.dat",
        )

        self.assertTrue(in_dictionary)
        self.assertIsNone(groups)

    def test_reports_thesaurus_but_not_in_dictionary(self) -> None:
        in_dictionary, groups = swedish.lookup_word(
            "betydelsefull",
            hunspell_path=FIXTURES / "sv_test.dic",
            thesaurus_path=FIXTURES / "th_sv_test.dat",
        )

        self.assertFalse(in_dictionary)
        self.assertIsNotNone(groups)

    def test_reports_neither_for_unknown_word(self) -> None:
        in_dictionary, groups = swedish.lookup_word(
            "någotutanträff",
            hunspell_path=FIXTURES / "sv_test.dic",
            thesaurus_path=FIXTURES / "th_sv_test.dat",
        )

        self.assertFalse(in_dictionary)
        self.assertIsNone(groups)


class RenderDocTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temp = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp.cleanup)
        self._data = Path(self._temp.name)
        env = {"XDG_DATA_HOME": str(self._data)}
        self._data_patch = patch.dict("os.environ", env, clear=True)
        self._data_patch.start()
        self.addCleanup(self._data_patch.stop)

        # Seed the cached thesaurus source so render_doc does not download.
        source = (
            self._data
            / "completion-dictionary"
            / "sources"
            / "mythes-sv"
            / "th_sv_SE.dat"
        )
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_bytes((FIXTURES / "th_sv_test.dat").read_bytes())

    def test_renders_synonyms_for_single_group(self) -> None:
        output = swedish.render_doc("betydelsefull")

        self.assertIn("Synonymer", output)
        self.assertIn("viktig", output)
        # No English gloss / ILI / Sense metadata leaks through.
        self.assertNotIn("gloss", output)
        self.assertNotIn("ILI", output)
        self.assertNotIn("Sense", output)

    def test_numbers_multiple_synonym_groups(self) -> None:
        output = swedish.render_doc("flitig")

        self.assertIn("1. ivrig, ambitiös", output)
        self.assertIn("2. arbetsam, strävsam", output)

    def test_returns_empty_string_for_unknown_word(self) -> None:
        self.assertEqual(swedish.render_doc("någotutanträff"), "")


class BuildDictionaryTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temp = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp.cleanup)
        self._data = Path(self._temp.name)
        env = {"XDG_DATA_HOME": str(self._data)}
        self._data_patch = patch.dict("os.environ", env, clear=True)
        self._data_patch.start()
        self.addCleanup(self._data_patch.stop)

    def _source_dir(self) -> Path:
        return self._data / "completion-dictionary" / "sources" / "hunspell-sv"

    def test_build_writes_sorted_casefolded_forms(self) -> None:
        # Pre-seed the cached source so no download is needed.
        source = self._source_dir() / "sv_SE_expanded.dic"
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(
            "4\narbetat\nBeta\nbeta\növerenskommelse\n",
            encoding="utf-8",
        )

        output_path = Path(self._temp.name) / "sv-hunspell.dict"
        with patch("completion_dictionary.swedish.urlopen") as urlopen:
            exit_code = swedish.build_dictionary(output_path)

        self.assertEqual(exit_code, 0)
        urlopen.assert_not_called()
        self.assertEqual(
            output_path.read_text(encoding="utf-8"),
            "arbetat\nBeta\nbeta\növerenskommelse\n",
        )


if __name__ == "__main__":
    unittest.main()
