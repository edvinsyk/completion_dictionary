from __future__ import annotations

import codecs
import http.client
import xml.etree.ElementTree as ET
from collections.abc import Iterable, Iterator
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

#: SALDO morphology (Språkbanken): every inflected form with its base word.
#: It adds forms the Hunspell list lacks, such as ``viktiga`` and ``husen``.
SALDO_MORPHOLOGY_URL = (
    "https://svn.spraakbanken.gu.se/sb-arkiv/pub/lmf/saldom/saldom.xml"
)

#: Relative layout of downloaded sources under the data directory.
HUNSPELL_SOURCE_DIR = Path("sources") / "hunspell-sv"
MYTHES_SOURCE_DIR = Path("sources") / "mythes-sv"
SALDO_SOURCE_DIR = Path("sources") / "saldo"

#: Built from SALDO: inflected forms whose base word has a thesaurus entry.
LEMMA_INDEX_NAME = "sv-lemmas.tsv"

#: SALDO form tags for compound parts, which are not words on their own.
COMPOUND_MSDS = {"c", "ci", "cm", "sms"}

#: Seconds to wait on the network before giving up on a download.
DOWNLOAD_TIMEOUT = 30


def _ensure_download(url: str, destination: Path) -> Path:
    """Download ``url`` to ``destination`` unless it already exists.

    The write is atomic: bytes land in a ``.tmp`` sibling first and are
    moved into place with :meth:`Path.replace`, so an interrupted download
    cannot leave a corrupt source file.  An existing destination is left
    untouched (cache hit).  A network failure exits with a short message
    instead of a traceback.
    """
    if destination.exists():
        return destination

    destination.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = destination.with_suffix(destination.suffix + ".tmp")
    try:
        with urlopen(url, timeout=DOWNLOAD_TIMEOUT) as response:
            tmp_path.write_bytes(response.read())
    except (OSError, http.client.HTTPException) as exc:
        tmp_path.unlink(missing_ok=True)
        raise SystemExit(f"Could not download {url}: {exc}") from exc
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


def ensure_saldo_source() -> Path:
    """Return the path to the cached SALDO morphology, downloading it if needed."""
    from completion_dictionary.cli import data_root

    destination = data_root() / SALDO_SOURCE_DIR / "saldom.xml"
    return _ensure_download(SALDO_MORPHOLOGY_URL, destination)


def lemma_index_path() -> Path:
    """Return the path of the generated form-to-base-word index."""
    from completion_dictionary.cli import data_root

    return data_root() / LEMMA_INDEX_NAME


def iter_saldo_forms(saldo_path: Path) -> Iterator[tuple[str, str]]:
    """Yield ``(form, base word)`` for each standalone word form in SALDO.

    Streams the SALDO morphology XML and skips compound parts and pieces of
    multiword expressions (``msd`` values such as ``c`` or
    ``sg indef nom 1:1-2``).
    """
    for _, element in ET.iterparse(saldo_path):
        if element.tag != "LexicalEntry":
            continue
        lemma_feat = element.find("Lemma/FormRepresentation/feat[@att='writtenForm']")
        lemma = lemma_feat.get("val") if lemma_feat is not None else None
        if lemma:
            for word_form in element.iterfind("WordForm"):
                feats = {feat.get("att"): feat.get("val") for feat in word_form}
                msd = feats.get("msd") or ""
                form = feats.get("writtenForm")
                if form and msd not in COMPOUND_MSDS and ":" not in msd:
                    yield form, lemma
        element.clear()


def write_lemma_index(pairs: Iterable[tuple[str, str]], output_path: Path) -> int:
    """Write sorted ``form<TAB>base word`` lines and return how many."""
    pairs = set(pairs)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        "".join(f"{form}\t{lemma}\n" for form, lemma in sorted(pairs)),
        encoding="utf-8",
    )
    return len(pairs)


def _index_lemmas(text: str, form: str) -> list[str]:
    """Return the base words listed for ``form`` in newline-led index text.

    ``text`` is the index with a leading newline, so every line starts with
    ``\\n`` and a plain substring search finds whole forms only.
    """
    key = f"\n{form}\t"
    lemmas = []
    start = text.find(key)
    while start != -1:
        value_start = start + len(key)
        end = text.find("\n", value_start)
        lemmas.append(text[value_start : end if end != -1 else len(text)])
        start = text.find(key, value_start)
    return lemmas


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
    """Build the Swedish completion dictionary from Hunspell and SALDO.

    Downloads/caches the sources if needed and merges the Hunspell forms
    with SALDO's single-word forms, sorted deterministically by
    ``(casefold, word)`` into a newline-delimited UTF-8 file.  The same
    SALDO pass writes the form index that maps inflected forms of
    thesaurus headwords to their base word.  The thesaurus is cached here
    too, so later ``doc`` calls never download from inside the editor.
    Returns ``0`` on success.
    """
    words = parse_hunspell_dictionary(ensure_hunspell_source())
    headwords = set(parse_thesaurus(ensure_thesaurus_source()))
    index_pairs: set[tuple[str, str]] = set()
    for form, lemma in iter_saldo_forms(ensure_saldo_source()):
        # Multiword expressions never match a single typed keyword.
        if " " not in form:
            words.add(form)
        if lemma in headwords and form != lemma:
            index_pairs.add((form, lemma))

    index_path = lemma_index_path()
    form_count = write_lemma_index(index_pairs, index_path)
    print(f"Wrote {form_count} inflected forms to {index_path}")

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
    text = _read_thesaurus_text(path)
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


def _read_thesaurus_text(path: Path) -> str:
    """Read a MyThes file as text, without a UTF-8 byte order mark."""
    raw = path.read_bytes()
    if raw.startswith(b"\xef\xbb\xbf"):
        raw = raw[3:]
    return _decode_thesaurus(raw)


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


def _thesaurus_entry(text: str, word: str) -> tuple[tuple[str, ...], ...] | None:
    """Return the synonym groups of the first ``word|N`` entry in ``text``.

    ``text`` is the thesaurus with a leading newline.  Searching for one
    headword line is much faster than :func:`parse_thesaurus` for a single
    ``doc`` call.  Group lines start with ``|``, so they never match.
    """
    key = f"\n{word}|"
    start = text.find(key)
    while start != -1:
        line_end = text.find("\n", start + 1)
        if line_end == -1:
            line_end = len(text)
        headword, group_count = _parse_headword(text[start + 1 : line_end].rstrip("\r"))
        if headword == word:
            group_lines = text[line_end + 1 :].split("\n", group_count)[:group_count]
            return tuple(
                tuple(synonyms)
                for line in group_lines
                if (synonyms := _split_group(line.rstrip("\r")))
            )
        start = text.find(key, line_end)
    return None


def lookup_synonyms(
    word: str, path: Path | None = None, lemma_path: Path | None = None
) -> tuple[tuple[str, ...], ...] | None:
    """Return the synonym groups for ``word``, or ``None`` if not found.

    An exact match wins; otherwise the lowercased word is tried, so a
    capitalized word at the start of a sentence still finds its entry.
    If neither is a headword, the SALDO form index maps an inflected form
    (``viktiga``) to its base word (``viktig``) and returns those groups.
    A form with several base words (``tomten``: ``tomt`` and ``tomte``)
    returns the groups of all of them, because the intended one is unknown.

    ``path`` defaults to the cached Swedish thesaurus source (downloading it
    if needed) and ``lemma_path`` to the index written by ``build``; tests
    pass fixture paths to avoid any download.  A missing index is skipped.
    """
    if path is None:
        path = ensure_thesaurus_source()
        lemma_path = lemma_index_path() if lemma_path is None else lemma_path
    thesaurus = "\n" + _read_thesaurus_text(path)
    candidates = dict.fromkeys((word, word.lower()))
    for candidate in candidates:
        groups = _thesaurus_entry(thesaurus, candidate)
        if groups is not None:
            return groups

    if lemma_path is None or not lemma_path.exists():
        return None
    index = "\n" + lemma_path.read_text(encoding="utf-8")
    groups = (
        group
        for candidate in candidates
        for lemma in _index_lemmas(index, candidate)
        for group in _thesaurus_entry(thesaurus, lemma) or ()
    )
    return tuple(dict.fromkeys(groups)) or None


def lookup_word(
    word: str,
    dictionary_path: Path,
    thesaurus_path: Path | None = None,
    lemma_path: Path | None = None,
) -> tuple[bool, tuple[tuple[str, ...], ...] | None]:
    """Report the new-backend lookup data for ``word``.

    Returns ``(in_completion_dictionary, synonym_groups)``.  Completion
    membership is checked against the built dictionary (``dictionary_path``),
    which merges Hunspell and SALDO forms; it is ``False`` before a build.
    Synonyms come from :func:`lookup_synonyms`, with ``thesaurus_path`` and
    ``lemma_path`` passed through so tests can avoid downloads.
    """
    in_dictionary = dictionary_path.exists() and (
        f"\n{word}\n" in "\n" + dictionary_path.read_text(encoding="utf-8")
    )
    groups = lookup_synonyms(word, thesaurus_path, lemma_path)
    return in_dictionary, groups


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
