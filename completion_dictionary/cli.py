from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

# Imported by require_wn() on first English use; importing wn costs ~60 ms.
wn: Any = None
wn_ili: Any = None

from completion_dictionary import swedish
from completion_dictionary.frequency import sort_by_frequency
from completion_dictionary.paths import atomic_output, data_root


ENGLISH_WORDNET_SPEC = "oewn:2025+"
#: How many of the most common English words to add to the WordNet lemmas.
#: WordNet lacks function words ("from", "the") and inflected forms ("was").
ENGLISH_COMMON_WORDS = 20000
POS_NAMES = {
    "n": "noun",
    "v": "verb",
    "a": "adjective",
    "s": "adjective satellite",
    "r": "adverb",
}


@dataclass(frozen=True)
class Profile:
    name: str
    backend: str
    default_output_name: str

    @property
    def default_output(self) -> Path:
        return data_root() / self.default_output_name


PROFILES = {
    "swedish": Profile(
        name="swedish",
        backend="swedish",
        default_output_name="sv-hunspell.dict",
    ),
    "english": Profile(
        name="english",
        backend="wordnet",
        default_output_name="en-oewn.dict",
    ),
}


@dataclass(frozen=True)
class SenseSummary:
    lemma: str
    pos: str
    sibling_lemmas: tuple[str, ...]
    ili_id: str | None
    gloss: str | None


def fail(message: str) -> int:
    print(message, file=sys.stderr)
    return 1


def require_wn() -> None:
    global wn, wn_ili
    if wn is not None:
        return
    try:
        import wn
        from wn import ili as wn_ili
    except ImportError:  # pragma: no cover - exercised in the CLI
        raise SystemExit(
            fail(
                "Missing dependency 'wn'. Install it with "
                "'uv tool install completion-dictionary' or "
                "'python -m pip install wn' and try again."
            )
        )


def load_profile(name: str) -> Profile:
    try:
        return PROFILES[name]
    except KeyError as exc:
        raise SystemExit(fail(f"Unknown profile: {name}")) from exc


def ensure_wordnet(download: bool = True) -> Any:
    """Return the English WordNet, or ``None`` if it is missing and ``download`` is false."""
    require_wn()
    if not wn.lexicons(lexicon=ENGLISH_WORDNET_SPEC):
        if not download:
            return None
        wn.download(ENGLISH_WORDNET_SPEC, progress_handler=None)
    return wn.Wordnet(ENGLISH_WORDNET_SPEC, expand="")


def lookup_words(wordnet: Any, query: str) -> list[Any]:
    candidates: list[Any] = []
    seen_ids: set[str] = set()
    for variant in dict.fromkeys((query, query.lower())):
        for word in wordnet.words(variant):
            if word.lemma().casefold() != query.casefold():
                continue
            if word.id in seen_ids:
                continue
            seen_ids.add(word.id)
            candidates.append(word)
    return candidates


def resolve_gloss(synset: Any) -> str | None:
    definition = synset.definition()
    if definition:
        return definition
    if not synset.ili:
        return None

    ili_record = wn_ili.get(synset.ili)
    if ili_record is None:
        return None
    return ili_record.definition()


def collect_senses(
    query: str,
    primary_wordnet: Any,
) -> list[SenseSummary]:
    senses: list[SenseSummary] = []
    seen_sense_ids: set[str] = set()
    for word in lookup_words(primary_wordnet, query):
        for sense in word.senses():
            if sense.id in seen_sense_ids:
                continue
            seen_sense_ids.add(sense.id)
            synset = sense.synset()
            sibling_lemmas = tuple(dict.fromkeys(synset.lemmas()))
            senses.append(
                SenseSummary(
                    lemma=word.lemma(),
                    pos=word.pos,
                    sibling_lemmas=sibling_lemmas,
                    ili_id=synset.ili,
                    gloss=resolve_gloss(synset),
                )
            )
    return senses


def format_pos(pos: str) -> str:
    return POS_NAMES.get(pos, pos)


def build_dictionary(
    profile: Profile, output_path: Path | None, refresh: bool = False
) -> int:
    if profile.backend == "swedish":
        return swedish.build_dictionary(output_path or profile.default_output, refresh)
    if refresh:
        return fail("--refresh is only supported for the swedish profile.")
    primary_wordnet = ensure_wordnet()
    actual_output = output_path or profile.default_output
    # Multiword lemmas ("ice cream") never match a single typed keyword.
    lemmas = {
        lemma
        for word in primary_wordnet.words()
        if (lemma := word.lemma().strip()) and " " not in lemma
    }
    from wordfreq import top_n_list

    lemmas.update(
        word for word in top_n_list("en", ENGLISH_COMMON_WORDS) if word.isalpha()
    )
    sorted_lemmas = sort_by_frequency(lemmas, "en")
    with atomic_output(actual_output) as tmp_path:
        tmp_path.write_text("\n".join(sorted_lemmas) + "\n", encoding="utf-8")
    print(f"Wrote {len(sorted_lemmas)} lemmas to {actual_output}")
    return 0


def run_lookup(profile: Profile, query: str) -> int:
    if profile.backend == "swedish":
        in_dictionary, groups = swedish.lookup_word(query, profile.default_output)
        print(f"Profile: {profile.name}")
        print(f"Word: {query}")
        print(f"Completion dictionary: {'yes' if in_dictionary else 'no'}")
        print(f"Thesaurus: {'yes' if groups else 'no'}")
        if groups:
            print()
            print("Synonyms:")
            for group in groups:
                for synonym in group:
                    print(f"  {synonym}")
        return 0

    primary_wordnet = ensure_wordnet()
    senses = collect_senses(query, primary_wordnet)
    print(f"Profile: {profile.name}")
    print(f"Lemma: {query}")
    print(f"Matches: {len(senses)} sense(s)")
    for index, sense in enumerate(senses, start=1):
        print()
        print(f"Sense {index}")
        print(f"  Lemma: {sense.lemma}")
        print(f"  Part of speech: {format_pos(sense.pos)}")
        print(f"  Synonyms: {', '.join(sense.sibling_lemmas)}")
        print(f"  ILI: {sense.ili_id or 'n/a'}")
        print(f"  English gloss: {sense.gloss or 'n/a'}")
    return 0


def render_doc(profile: Profile, query: str) -> str:
    if profile.backend == "swedish":
        return swedish.render_doc(query)

    # The editor calls doc on every completion; never download from there.
    primary_wordnet = ensure_wordnet(download=False)
    if primary_wordnet is None:
        return ""
    senses = collect_senses(query, primary_wordnet)
    if not senses:
        return ""

    lines = [query]
    for index, sense in enumerate(senses[:3], start=1):
        lines.append("")
        lines.append(f"Sense {index} [{format_pos(sense.pos)}]")
        lines.append(f"Synonyms: {', '.join(sense.sibling_lemmas)}")
        lines.append(f"ILI: {sense.ili_id or 'n/a'}")
        if sense.gloss:
            lines.append(f"English gloss: {sense.gloss}")
        else:
            lines.append("English gloss: unavailable")
    return "\n".join(lines)


def synonym_groups(profile: Profile, query: str) -> list[dict[str, Any]]:
    """Return synonym groups for ``query``, without the query itself.

    Each group is ``{"words": [...], "pos": str | None, "gloss": str | None}``.
    Swedish groups have no part of speech or gloss.  Groups left with no
    words are dropped.
    """
    if profile.backend == "swedish":
        groups = [
            {"words": list(group), "pos": None, "gloss": None}
            for group in swedish.lookup_synonyms(query) or ()
        ]
    elif (primary_wordnet := ensure_wordnet(download=False)) is None:
        groups = []
    else:
        groups = [
            {
                "words": list(sense.sibling_lemmas),
                "pos": format_pos(sense.pos),
                "gloss": sense.gloss,
            }
            for sense in collect_senses(query, primary_wordnet)
        ]

    for group in groups:
        group["words"] = [
            word for word in group["words"] if word.casefold() != query.casefold()
        ]
    return [group for group in groups if group["words"]]


def print_path(profile: Profile, output_path: Path | None) -> int:
    print(output_path or profile.default_output)
    return 0


def add_profile_argument(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--profile",
        choices=sorted(PROFILES),
        default="swedish",
        help="Which dictionary profile to use. Default: swedish",
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Build language dictionaries and documentation backends for blink-cmp-dictionary."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    build_parser = subparsers.add_parser(
        "build",
        help="Generate the newline-delimited dictionary file for a profile.",
    )
    add_profile_argument(build_parser)
    build_parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Where to write the dictionary file. Defaults to the profile's standard path.",
    )
    build_parser.add_argument(
        "--refresh",
        action="store_true",
        help="Redownload upstream dictionary data before building (swedish only).",
    )

    path_parser = subparsers.add_parser(
        "path",
        help="Print the default dictionary path for a profile.",
    )
    add_profile_argument(path_parser)
    path_parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Echo a custom output path instead of the default.",
    )

    lookup_parser = subparsers.add_parser(
        "lookup",
        help="Show a learning/debug view for a lemma.",
    )
    add_profile_argument(lookup_parser)
    lookup_parser.add_argument("word", help="The lemma to inspect.")

    doc_parser = subparsers.add_parser(
        "doc",
        help="Print the documentation text for blink-cmp-dictionary.",
    )
    add_profile_argument(doc_parser)
    doc_parser.add_argument("word", help="The lemma to document.")

    synonyms_parser = subparsers.add_parser(
        "synonyms",
        help="Print synonym groups for a word as JSON, for editor pickers.",
    )
    add_profile_argument(synonyms_parser)
    synonyms_parser.add_argument("word", help="The word to find synonyms for.")

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    profile = load_profile(args.profile)

    if args.command == "build":
        return build_dictionary(profile, args.output, args.refresh)
    if args.command == "path":
        return print_path(profile, args.output)
    if args.command == "lookup":
        return run_lookup(profile, args.word)
    if args.command == "doc":
        output = render_doc(profile, args.word)
        if output:
            print(output)
        return 0
    if args.command == "synonyms":
        print(json.dumps(synonym_groups(profile, args.word), ensure_ascii=False))
        return 0
    return fail(f"Unknown command: {args.command}")
