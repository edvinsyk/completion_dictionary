from __future__ import annotations

import argparse
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import wn
    from wn import ili as wn_ili
except ImportError:  # pragma: no cover - exercised in the CLI
    wn = None
    wn_ili = None

from completion_dictionary import swedish


CILI_SPEC = "cili:1.0"
POS_NAMES = {
    "n": "noun",
    "v": "verb",
    "a": "adjective",
    "s": "adjective satellite",
    "r": "adverb",
}


def data_root() -> Path:
    xdg_data_home = os.environ.get("XDG_DATA_HOME")
    if xdg_data_home:
        return Path(xdg_data_home) / "completion-dictionary"
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / "completion-dictionary"
    if os.name == "nt":
        local_app_data = os.environ.get("LOCALAPPDATA")
        if local_app_data:
            return Path(local_app_data) / "completion-dictionary"
        return Path.home() / "AppData" / "Local" / "completion-dictionary"
    return Path.home() / ".local" / "share" / "completion-dictionary"


@dataclass(frozen=True)
class Profile:
    name: str
    primary_spec: str
    fallback_spec: str | None
    default_output_name: str
    doc_language: str

    @property
    def default_output(self) -> Path:
        return data_root() / self.default_output_name


PROFILES = {
    "swedish": Profile(
        name="swedish",
        primary_spec="omw-sv:2.0",
        fallback_spec="omw-en:2.0",
        default_output_name="sv-hunspell.dict",
        doc_language="English",
    ),
    "english": Profile(
        name="english",
        primary_spec="oewn:2025+",
        fallback_spec=None,
        default_output_name="en-oewn.dict",
        doc_language="English",
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
    if wn is None:
        raise SystemExit(
            fail(
                "Missing dependency 'wn'. Install it with "
                "'uv tool install completion-dictionary' or "
                "'python -m pip install wn' and try again."
            )
        )


def ensure_package(spec: str) -> None:
    if spec == CILI_SPEC:
        ili_record = wn_ili.get("i1")
        if ili_record is None or ili_record.definition() is None:
            wn.download(spec, progress_handler=None)
        return

    if not wn.lexicons(lexicon=spec):
        wn.download(spec, progress_handler=None)


def load_profile(name: str) -> Profile:
    try:
        return PROFILES[name]
    except KeyError as exc:
        raise SystemExit(fail(f"Unknown profile: {name}")) from exc


def ensure_wordnets(profile: Profile) -> tuple[Any, Any | None]:
    require_wn()
    ensure_package(profile.primary_spec)
    primary = wn.Wordnet(profile.primary_spec, expand="")

    fallback = None
    if profile.fallback_spec:
        ensure_package(profile.fallback_spec)
        fallback = wn.Wordnet(profile.fallback_spec, expand="")
        ensure_package(CILI_SPEC)

    return primary, fallback


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


def resolve_gloss(synset: Any, fallback_wordnet: Any | None) -> str | None:
    definition = synset.definition()
    if definition:
        return definition
    if not synset.ili:
        return None

    if fallback_wordnet is not None:
        for fallback_synset in fallback_wordnet.synsets(ili=synset.ili):
            fallback_definition = fallback_synset.definition()
            if fallback_definition:
                return fallback_definition

    ili_record = wn_ili.get(synset.ili)
    if ili_record is None:
        return None
    return ili_record.definition()


def collect_senses(
    query: str,
    primary_wordnet: Any,
    fallback_wordnet: Any | None,
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
                    gloss=resolve_gloss(synset, fallback_wordnet),
                )
            )
    return senses


def format_pos(pos: str) -> str:
    return POS_NAMES.get(pos, pos)


def build_dictionary(profile: Profile, output_path: Path | None) -> int:
    if profile.name == "swedish":
        return swedish.build_dictionary(output_path or profile.default_output)
    primary_wordnet, _ = ensure_wordnets(profile)
    actual_output = output_path or profile.default_output
    lemmas = {
        word.lemma().strip()
        for word in primary_wordnet.words()
        if word.lemma().strip()
    }
    actual_output.parent.mkdir(parents=True, exist_ok=True)
    sorted_lemmas = sorted(lemmas, key=lambda lemma: (lemma.casefold(), lemma))
    actual_output.write_text("\n".join(sorted_lemmas) + "\n", encoding="utf-8")
    print(f"Wrote {len(sorted_lemmas)} lemmas to {actual_output}")
    return 0


def run_lookup(profile: Profile, query: str) -> int:
    if profile.name == "swedish":
        in_dictionary, groups = swedish.lookup_word(query)
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

    primary_wordnet, fallback_wordnet = ensure_wordnets(profile)
    senses = collect_senses(query, primary_wordnet, fallback_wordnet)
    if not senses:
        return 0

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
        print(f"  {profile.doc_language} gloss: {sense.gloss or 'n/a'}")
    return 0


def render_doc(profile: Profile, query: str) -> str:
    if profile.name == "swedish":
        return swedish.render_doc(query)

    primary_wordnet, fallback_wordnet = ensure_wordnets(profile)
    senses = collect_senses(query, primary_wordnet, fallback_wordnet)
    if not senses:
        return ""

    lines = [query]
    for index, sense in enumerate(senses[:3], start=1):
        lines.append("")
        lines.append(f"Sense {index} [{format_pos(sense.pos)}]")
        lines.append(f"Synonyms: {', '.join(sense.sibling_lemmas)}")
        lines.append(f"ILI: {sense.ili_id or 'n/a'}")
        if sense.gloss:
            lines.append(f"{profile.doc_language} gloss: {sense.gloss}")
        else:
            lines.append(f"{profile.doc_language} gloss: unavailable")
    return "\n".join(lines)


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

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    profile = load_profile(args.profile)

    if args.command == "build":
        return build_dictionary(profile, args.output)
    if args.command == "path":
        return print_path(profile, args.output)
    if args.command == "lookup":
        return run_lookup(profile, args.word)
    if args.command == "doc":
        output = render_doc(profile, args.word)
        if output:
            print(output)
        return 0
    return fail(f"Unknown command: {args.command}")
