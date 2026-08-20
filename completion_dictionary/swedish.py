from __future__ import annotations

from pathlib import Path


def parse_hunspell_dictionary(path: Path) -> set[str]:
    """Parse a Hunspell ``.dic`` file into unique word forms.

    Tolerant of normal Hunspell syntax: an initial numeric word-count line is
    ignored, affix flags after ``/`` are stripped, whitespace is trimmed, and
    empty lines are discarded.  Returns the unique remaining word forms.
    """
    words: set[str] = set()
    lines = path.read_text(encoding="utf-8").splitlines()

    # An initial numeric word-count line is a Hunspell convention; skip it.
    if lines and lines[0].strip().isdigit():
        lines = lines[1:]

    for line in lines:
        entry = line.split("/", 1)[0].strip()
        if not entry:
            continue
        words.add(entry)
    return words
