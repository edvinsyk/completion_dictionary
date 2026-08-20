from __future__ import annotations

from pathlib import Path
from urllib.request import urlopen

#: Upstream Swedish Hunspell dictionary (yeager/hunspell-sv), fully expanded forms.
SWEDISH_HUNSPELL_URL = (
    "https://raw.githubusercontent.com/yeager/hunspell-sv/main/"
    "sv_SE_expanded.dic"
)
SWEDISH_HUNSPELL_VERSION = "main"

#: LibreOffice Swedish thesaurus (MyThes / Synlex).
MYTHES_URL = (
    "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/"
    "sv_SE/th_sv_SE.dat"
)
MYTHES_VERSION = "master"

#: Relative layout of downloaded sources under the data directory.
HUNSPELL_SOURCE_DIR = Path("sources") / "hunspell-sv"
MYTHES_SOURCE_DIR = Path("sources") / "mythes-sv"


def _ensure_download(url: str, destination: Path) -> Path:
    """Download ``url`` to ``destination`` unless it already exists.

    The write is atomic: bytes land in a ``.tmp`` sibling first and are
    moved into place with :meth:`Path.replace`, so an interrupted download
    cannot leave a corrupt source file.  An existing destination is left
    untouched (cache hit).
    """
    if destination.exists():
        return destination

    destination.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = destination.with_suffix(destination.suffix + ".tmp")
    with urlopen(url) as response:
        tmp_path.write_bytes(response.read())
    tmp_path.replace(destination)
    return destination


def ensure_hunspell_source() -> Path:
    """Return the path to the cached Swedish Hunspell dictionary, downloading it if needed."""
    from completion_dictionary.cli import data_root

    destination = data_root() / HUNSPELL_SOURCE_DIR / "sv_SE_expanded.dic"
    return _ensure_download(SWEDISH_HUNSPELL_URL, destination)


def ensure_thesaurus_source() -> Path:
    """Return the path to the cached Swedish MyThes thesaurus, downloading it if needed."""
    from completion_dictionary.cli import data_root

    destination = data_root() / MYTHES_SOURCE_DIR / "th_sv_SE.dat"
    return _ensure_download(MYTHES_URL, destination)


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


def build_dictionary(output_path: Path) -> int:
    """Build the Swedish completion dictionary from the Hunspell source.

    Downloads/caches the source if needed, parses it into unique word forms,
    sorts them deterministically by ``(casefold, word)``, and writes a
    newline-delimited UTF-8 file.  Returns ``0`` on success.
    """
    source = ensure_hunspell_source()
    words = parse_hunspell_dictionary(source)

    sorted_words = sorted(words, key=lambda word: (word.casefold(), word))

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(sorted_words) + "\n", encoding="utf-8")
    print(f"Wrote {len(sorted_words)} word forms to {output_path}")
    return 0
