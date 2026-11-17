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
| Version| Tracks the `main` branch (not pinned) |
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
| Version| Tracks the `master` branch (not pinned) |
| Author | Viggo Kann (KTH), Synlex project |
| License| CC-BY |

The thesaurus is based on the [Synlex](https://folkets-lexikon.csc.kth.se/synlex.html)
project led by Viggo Kann at KTH, a collaboratively built Swedish synonym
dictionary.

## Swedish word forms — SALDO morphology

Used by the `swedish` profile in two ways. Its single-word forms are added to
the completion dictionary, because the Hunspell list lacks many inflected forms
such as `viktiga` and `husen`. It also links inflected forms to their base word,
so synonym lookup works for `viktiga` (base word `viktig`). The build writes the
forms whose base word has a thesaurus entry to `sv-lemmas.tsv` in the user data
directory.

| Field  | Value |
|--------|-------|
| Source | [SALDO's morphology](https://sprakbanken.se/en/resources/saldom), Språkbanken Text, University of Gothenburg |
| File   | `saldom.xml` (LMF lexicon, about 250 MB) |
| Version| Downloads from an unversioned URL (not pinned) |
| License| [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |

Citation: Språkbanken Text. SALDO's morphology. https://doi.org/10.23695/agcm-ny22

## English — Open English WordNet (OEWN)

Used by the `english` profile. Data is provided through the
[`wn`](https://github.com/goodmami/wn) Python package (spec `oewn:2025+`), which
downloads and manages WordNet data. See the `wn` package and
[Open English WordNet](https://en-word.net/) for its data source and license.

## Word frequencies — wordfreq

Used by both profiles to put the most common words first in the completion
dictionary. The `english` profile also adds the most common English words that
WordNet lacks, such as `from` and `was`.

| Field  | Value |
|--------|-------|
| Source | [wordfreq](https://github.com/rspeer/wordfreq) Python package |
| Version| Installed as a package dependency (`wordfreq` 3.x) |
| License| Code: Apache-2.0. Data: [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) |

Unlike the other sources, the frequency data ships inside the `wordfreq`
package and is not downloaded at build time. It includes data from Wikipedia,
OpenSubtitles, and other corpora. See the wordfreq README for the full list of
sources and their attribution.

## Note on retention

Because sources are downloaded on demand at build time, upstream license texts
are not bundled here. Refer to the links above for the authoritative license
terms of each resource.
