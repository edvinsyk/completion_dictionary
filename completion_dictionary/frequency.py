from __future__ import annotations

from collections.abc import Iterable


def sort_by_frequency(words: Iterable[str], lang: str) -> list[str]:
    """Return ``words`` with the most common first, then alphabetically.

    A lowercase word comes before its capitalized twin (``frost``, ``Frost``),
    because wordfreq gives both the same frequency.

    The completion menu keeps this file order for equally good matches, so
    ``fro`` offers ``from`` before ``frolicsomeness``.  Words unknown to
    wordfreq sort last, alphabetically.
    """
    # Imported here: wordfreq costs ~75 ms, and doc runs on every completion.
    from wordfreq import get_frequency_dict

    frequencies = get_frequency_dict(lang)
    return sorted(
        words,
        key=lambda word: (
            -frequencies.get(word.casefold(), 0.0),
            word.casefold(),
            not word.islower(),
            word,
        ),
    )
