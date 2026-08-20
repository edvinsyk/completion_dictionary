# Third-party data, attribution, and licensing

`completion-dictionary` builds its Swedish and English data from third-party
sources. These sources are **downloaded at build time** into the user data
directory and are **not** copied into the Python package or committed to the
repository. This keeps the package separate from the upstream dictionary files
and avoids shipping them until their licenses and attribution are verified.

## Swedish completion vocabulary — hunspell-sv

Used by the `swedish` profile to build the completion dictionary.

| Field  | Value |
|--------|-------|
| Source | [yeager/hunspell-sv](https://github.com/yeager/hunspell-sv) |
| File   | `sv_SE_expanded.dic` (fully expanded word forms) |
| Version| `main` (pinned in `completion_dictionary/swedish.py`) |
| License| LGPL-3.0 |

The dictionary builds on a number of upstream Swedish lexical resources,
including SFOL 2.42 (Den Stora Fria Ordlistan), SALDO/SALDOM (Språkbanken,
University of Gothenburg), Folkets Lexikon (KTH), and professional translation
memory. Its expanded form is what provides inflected forms (e.g.
`akademi` → `akademin`, `akademier`, `akademierna`) in addition to lemmas.

## Swedish thesaurus — Synlex / LibreOffice Swedish thesaurus

Used by the `swedish` profile for synonym lookup (`Synonymer` output in
`lookup` and `doc`).

| Field  | Value |
|--------|-------|
| Source | [LibreOffice/dictionaries](https://github.com/LibreOffice/dictionaries), `sv_SE/th_sv_SE.dat` |
| Format | MyThes (`.dat`) thesaurus |
| Version| `master` (pinned in `completion_dictionary/swedish.py`) |
| Author | Viggo Kann (KTH), Synlex project |
| License| CC-BY |

The thesaurus is based on the [Synlex](https://folkets-lexikon.csc.kth.se/synlex.html)
project led by Viggo Kann at KTH, a collaboratively built Swedish synonym
dictionary.

## English — Open English WordNet (OEWN)

Used by the `english` profile. Data is provided through the
[`wn`](https://github.com/goodmami/wn) Python package (spec `oewn:2025+`), which
downloads and manages WordNet data. See the `wn` package and
[Open English WordNet](https://en-word.net/) for its data source and license.

## Note on retention

Because sources are downloaded on demand at build time, upstream license texts
are not bundled here. Refer to the links above for the authoritative license
terms of each resource.
