from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from completion_dictionary import cli


class FakeWord:
    def __init__(self, lemma: str) -> None:
        self._lemma = lemma

    def lemma(self) -> str:
        return self._lemma


class FakeWordnet:
    def __init__(self, lemmas: list[str]) -> None:
        self._lemmas = lemmas

    def words(self) -> list[FakeWord]:
        return [FakeWord(lemma) for lemma in self._lemmas]


class CliTests(unittest.TestCase):
    def test_load_profile_rejects_unknown_profile(self) -> None:
        with self.assertRaises(SystemExit) as exc:
            cli.load_profile("unknown")

        self.assertEqual(exc.exception.code, 1)

    def test_data_root_prefers_xdg_data_home(self) -> None:
        with patch.dict("os.environ", {"XDG_DATA_HOME": "/tmp/xdg-home"}, clear=True):
            self.assertEqual(
                cli.data_root(),
                Path("/tmp/xdg-home/completion-dictionary"),
            )

    def test_build_dictionary_sorts_casefolded_and_deduplicates(self) -> None:
        profile = cli.PROFILES["english"]
        fake_wordnet = FakeWordnet(["Beta", "alpha", " beta ", "Alpha", ""])

        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "en.dict"
            with patch.object(cli, "ensure_wordnets", return_value=(fake_wordnet, None)):
                exit_code = cli.build_dictionary(profile, output_path)

            self.assertEqual(exit_code, 0)
            self.assertEqual(
                output_path.read_text(encoding="utf-8"),
                "Alpha\nalpha\nBeta\nbeta\n",
            )

    def test_render_doc_limits_output_to_three_senses(self) -> None:
        senses = [
            cli.SenseSummary("bed", "n", ("bed", "bunk"), "i1", "first"),
            cli.SenseSummary("bed", "v", ("bed",), "i2", "second"),
            cli.SenseSummary("bed", "a", ("bed",), "i3", "third"),
            cli.SenseSummary("bed", "r", ("bed",), "i4", "fourth"),
        ]

        with patch.object(cli, "ensure_wordnets", return_value=("primary", None)):
            with patch.object(cli, "collect_senses", return_value=senses):
                output = cli.render_doc(cli.PROFILES["english"], "bed")

        self.assertIn("Sense 1 [noun]", output)
        self.assertIn("Sense 2 [verb]", output)
        self.assertIn("Sense 3 [adjective]", output)
        self.assertNotIn("Sense 4", output)


    def test_render_doc_dispatches_swedish_to_thesaurus(self) -> None:
        with patch.object(
            cli.swedish, "render_doc", return_value="betydelsefull\n"
        ) as mock:
            output = cli.render_doc(cli.PROFILES["swedish"], "betydelsefull")

        self.assertEqual(output, "betydelsefull\n")
        mock.assert_called_once_with("betydelsefull")

    def test_run_lookup_dispatches_swedish_to_new_backend(self) -> None:
        with patch.object(
            cli.swedish,
            "lookup_word",
            return_value=(True, (("viktig", "väsentlig", "signifikant"),)),
        ) as mock:
            with patch("builtins.print") as print_mock:
                exit_code = cli.run_lookup(cli.PROFILES["swedish"], "betydelsefull")

        self.assertEqual(exit_code, 0)
        mock.assert_called_once_with("betydelsefull")
        printed = "".join(str(call) + "\n" for call in print_mock.call_args_list)
        self.assertIn("Profile: swedish", printed)
        self.assertIn("Word: betydelsefull", printed)
        self.assertIn("Completion dictionary: yes", printed)
        self.assertIn("Thesaurus: yes", printed)
        self.assertIn("Synonyms:", printed)
        self.assertIn("  viktig", printed)

    def test_run_lookup_reports_no_thesaurus_for_inflected_word(self) -> None:
        with patch.object(
            cli.swedish, "lookup_word", return_value=(True, None)
        ):
            with patch("builtins.print") as print_mock:
                exit_code = cli.run_lookup(cli.PROFILES["swedish"], "betydelsefulla")

        self.assertEqual(exit_code, 0)
        printed = "".join(str(call) + "\n" for call in print_mock.call_args_list)
        self.assertIn("Thesaurus: no", printed)
        self.assertNotIn("Synonyms:", printed)


if __name__ == "__main__":
    unittest.main()
