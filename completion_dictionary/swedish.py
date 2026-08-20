from __future__ import annotations

import codecs
import functools
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


def parse_thesaurus(path: Path) -> dict[str, tuple[tuple[str, ...], ...]]:
    """Parse a MyThes ``.dat`` file into ``{headword: synonym_groups}``.

    The MyThes text format is::

        ENCODING
        word|N
        |syn1|syn2|...
        |syn1|syn2|...   # N group lines in total

    The first line declares the file's character encoding, and each entry is a
    ``word|N`` headword line followed by exactly ``N`` synonym-group lines
    (each group is a ``|``-separated list of synonyms).  Synonym groups are
    preserved as separate tuples so callers can number or select among them.

    The declared encoding is honoured when it names a codec; otherwise UTF-8
    is tried and ISO-8859-1 used as a fallback.
    """
    raw = path.read_bytes()
    if raw.startswith(b"\xef\xbb\xbf"):
        raw = raw[3:]

    text = _decode_thesaurus(raw)
    thesaurus: dict[str, tuple[tuple[str, ...], ...]] = {}

    lines = text.splitlines()
    i = 0
    while i < len(lines):
        headword, group_count = _parse_headword(lines[i])
        if headword is None:
            i += 1
            continue

        groups: list[tuple[str, ...]] = []
        for offset in range(1, group_count + 1):
            idx = i + offset
            if idx >= len(lines):
                break
            synonyms = _split_group(lines[idx])
            if synonyms:
                groups.append(tuple(synonyms))

        if headword not in thesaurus:
            thesaurus[headword] = tuple(groups)
        i += group_count + 1

    return thesaurus


def _decode_thesaurus(raw: bytes) -> str:
    """Decode MyThes bytes using the encoding declared on the first line."""
    header_end = raw.find(b"\n")
    if header_end != -1:
        header = raw[:header_end].strip().decode("ascii", "ignore")
        try:
            return raw.decode(header)
        except (LookupError, UnicodeDecodeError):
            pass
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("latin-1")


def _parse_headword(line: str) -> tuple[str | None, int]:
    """Return ``(word, group_count)`` if ``line`` is a MyThes headword line.

    A headword line is ``word|N`` with no leading ``|`` and a numeric count
    after the final ``|``.  Returns ``(None, 0)`` for non-headword lines.
    """
    if line.startswith("|"):
        return None, 0
    word, sep, count = line.rpartition("|")
    if not sep or not count.isdigit():
        return None, 0
    return word, int(count)


def _split_group(line: str) -> list[str]:
    """Split a ``|syn1|syn2|...`` line into its non-empty synonyms."""
    return [synonym for synonym in line.split("|") if synonym]


@functools.lru_cache(maxsize=None)
def _parse_thesaurus_cached(path: Path) -> dict[str, tuple[tuple[str, ...], ...]]:
    return parse_thesaurus(path)


def lookup_synonyms(
    word: str, path: Path | None = None
) -> tuple[tuple[str, ...], ...] | None:
    """Return the synonym groups for ``word``, or ``None`` if not found.

    ``path`` defaults to the cached Swedish thesaurus source (downloading it
    if needed); tests pass a fixture path to avoid any download.
    """
    source = ensure_thesaurus_source() if path is None else path
    return _parse_thesaurus_cached(source).get(word)


def render_doc(query: str) -> str:
    """Render the blink-cmp-dictionary doc for ``query`` from the thesaurus.

    Returns the headword followed by a ``Synonymer`` block.  When a word has
    a single synonym group its members are joined on one line; multiple
    groups are numbered (``1.``, ``2.``, ...).  Returns ``""`` when there is
    no thesaurus entry for ``query`` (e.g. an inflected form with no direct
    headword), which the CLI treats as exit 0 with no output.
    """
    groups = lookup_synonyms(query)
    if groups is None:
        return ""

    lines = [query, "", "Synonymer"]
    if len(groups) == 1:
        lines.append(", ".join(groups[0]))
    else:
        for index, group in enumerate(groups, start=1):
            lines.append(f"{index}. {', '.join(group)}")
    return "\n".join(lines)
