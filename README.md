# `completion-dictionary`

`completion-dictionary` is a small CLI for [blink-cmp-dictionary](https://github.com/Kaiser-Yang/blink-cmp-dictionary).
It builds newline-delimited dictionary files and prints documentation for single words.

The current release supports two profiles, each backed by a different data source.

- `swedish`: completion vocabulary from the [hunspell-sv](https://github.com/yeager/hunspell-sv) Swedish dictionary and the [SALDO](https://sprakbanken.se/en/resources/saldom) morphology (about 960,000 word forms, including inflected forms), and synonyms from the Synlex / LibreOffice Swedish thesaurus.
  SALDO also links inflected forms to their base word, so `viktiga` gets the synonyms of `viktig`.
- `english`: lemma completion and WordNet documentation from the [Open English WordNet](https://en-word.net/) 2025+ release.

Third-party data is downloaded at build time, not bundled in the package. See
[`THIRD_PARTY_DATA.md`](THIRD_PARTY_DATA.md) for sources, licenses, and attribution.

`blink-cmp-dictionary` reads completion candidates from a plain text dictionary file.
Its documentation hook can call an external command for richer item details.
`completion-dictionary` handles both sides: it builds the static `.dict` file and renders documentation for individual lemmas on demand.

## Install

Install directly from GitHub with `cargo`:

```bash
cargo install --git https://github.com/edvinsyk/completion_dictionary
```

To install a specific tag, branch, or commit, add `--tag`, `--branch`, or `--rev`:

```bash
cargo install --git https://github.com/edvinsyk/completion_dictionary --branch main
cargo install --git https://github.com/edvinsyk/completion_dictionary --tag v0.1.0
```

To update an existing installation, add `--force`:

```bash
cargo install --force --git https://github.com/edvinsyk/completion_dictionary
```

For local development from a checkout, install from the working directory:

```bash
cargo install --path .
```

Cargo puts the executable in `~/.cargo/bin`, unless you set `CARGO_INSTALL_ROOT` or `CARGO_HOME`.

## CLI

### Build a dictionary

Swedish:

```bash
completion-dictionary build --profile swedish
```

English:

```bash
completion-dictionary build --profile english
```

This command will:

- if the data for the selected profile is not cached, download and cache it
- collect the completion vocabulary from the profile's source
- for the `english` profile, add the 20,000 most common English words, because WordNet has no function words such as `from` and no inflected forms such as `was`
- deduplicate it exactly
- sort it with the most common words first, using [wordfreq](https://github.com/rspeer/wordfreq) word frequencies, and sort words with the same frequency alphabetically

For the `swedish` profile, the first build also downloads the SALDO morphology (about 250 MB) and takes about 15 seconds longer.
The build writes a small form index, `sv-lemmas.tsv`, to the user data directory, even with `--output`.
The `english` build also writes a sense index, `en-oewn.tsv` (about 26 MB), to the user data directory. The `doc`, `lookup`, and `synonyms` commands read it.

A normal build uses the cached source files. To download the newest Swedish sources and rebuild, add `--refresh`:

```bash
completion-dictionary build --profile swedish --refresh
```

If a download fails, the build stops and keeps the old cached file.
The `doc` and `synonyms` commands never download data. Before the first `build`, they print nothing.

By default, the tool writes into a user data directory instead of the
repo.

On macOS, the default outputs are:

- `~/Library/Application Support/completion-dictionary/sv-hunspell.dict`
- `~/Library/Application Support/completion-dictionary/en-oewn.dict`

You can print the exact path for a profile:

```bash
completion-dictionary path --profile swedish
completion-dictionary path --profile english
```

You can also choose a custom output path:

```bash
completion-dictionary build --profile english --output /tmp/en.dict
```

### Inspect a lemma

```bash
completion-dictionary lookup --profile swedish säng
completion-dictionary lookup --profile english bed
```

For the `english` profile this prints a learning/debug view with:

- lemma
- part of speech
- sibling lemmas from the same synset
- ILI id
- gloss

For the `swedish` profile it reports whether the word is in the built completion
dictionary, which merges Hunspell and SALDO forms.
It also reports whether a thesaurus entry exists, and lists the synonyms:

```text
Profile: swedish
Word: säng
Completion dictionary: yes
Thesaurus: yes

Synonyms:
  bädd
  brits
```

### Render documentation for Blink

```bash
completion-dictionary doc --profile swedish säng
completion-dictionary doc --profile english bed
```

This prints the text that `blink-cmp-dictionary` shows in the documentation window.
If no match exists, it prints nothing and exits successfully.

For the `swedish` profile the doc shows the headword followed by a `Synonymer`
(synonyms) block:

```text
säng

Synonymer
bädd, brits
```

### List synonyms for an editor picker

```bash
completion-dictionary synonyms --profile swedish viktig
completion-dictionary synonyms --profile english bed
```

This prints a JSON list of synonym groups, so an editor can show a picker and replace the word.
Each group has `words`, `pos` (part of speech), and `gloss`.
The `swedish` profile sets `pos` and `gloss` to `null`.
The word itself is left out of `words`, and groups with no other words are left out.
If no match exists, it prints `[]`.

```json
[{"words": ["bunk"], "pos": "noun", "gloss": "a piece of furniture that provides a place to sleep"}]
```

## blink.cmp configuration

Build the dictionaries first:

```bash
completion-dictionary build --profile swedish
completion-dictionary build --profile english
```

Then point `blink-cmp-dictionary` at the generated files and override the documentation command so it uses `completion-dictionary` instead of the system `wn` binary.

On macOS, the default dictionary paths are:

- `~/Library/Application Support/completion-dictionary/sv-hunspell.dict`
- `~/Library/Application Support/completion-dictionary/en-oewn.dict`

Example Blink config:

```lua
local completion_dictionary = vim.fn.expand("~/.cargo/bin/completion-dictionary")
local swedish_dict = vim.fn.expand("~/Library/Application Support/completion-dictionary/sv-hunspell.dict")
local english_dict = vim.fn.expand("~/Library/Application Support/completion-dictionary/en-oewn.dict")

local function dictionary_file_for_context()
  if vim.bo.filetype == "markdown" or vim.bo.filetype == "text" then
    return english_dict
  end
  return swedish_dict
end

local function dictionary_profile_for_context()
  if vim.bo.filetype == "markdown" or vim.bo.filetype == "text" then
    return "english"
  end
  return "swedish"
end

{
  "saghen/blink.cmp",
  dependencies = {
    "Kaiser-Yang/blink-cmp-dictionary",
  },
  opts = {
    sources = {
      default = { "dictionary", "lsp", "path", "buffer" },
      providers = {
        dictionary = {
          module = "blink-cmp-dictionary",
          name = "Dict",
          min_keyword_length = 1,
          opts = {
            dictionary_files = function()
              return { dictionary_file_for_context() }
            end,
            get_documentation = function(item)
              return {
                get_command = function()
                  return completion_dictionary
                end,
                get_command_args = function()
                  return {
                    "doc",
                    "--profile",
                    dictionary_profile_for_context(),
                    item,
                  }
                end,
                resolve_documentation = function(output)
                  return output
                end,
                on_error = function(_, _)
                  return false
                end,
              }
            end,
          },
        },
      },
    },
  },
}
```

If `cargo` installs into a different directory, update `completion_dictionary` to that absolute path.

## Notes

- Generated dictionary files are UTF-8 text with one completion candidate per line.
- Completion candidates come from the static `.dict` files, not from `completion-dictionary` at typing time.
- The editor runs `completion-dictionary` only for `doc` and `synonyms`. These commands read the prebuilt indexes and never download data.
- The editor keeps the file order for equally good matches only if it tells `fzf` to use `--tiebreak=index` and gives each item a `sortText`. [writing-tools.nvim](https://github.com/edvinsyk/writing-tools.nvim) does this.
- Generated dictionaries are not intended to be committed to the repository or shipped in the binary.
- The `english` profile merges single-word WordNet lemmas with the 20,000 most common English words from wordfreq. Only these common words add inflected forms. Multiword lemmas such as `ice cream` are left out. The `swedish` profile is not lemma-only. It merges Hunspell expanded forms with SALDO word forms, so inflected forms are included as completion candidates.
- Third-party source data is downloaded at build time and cached under the user data directory. See [`THIRD_PARTY_DATA.md`](THIRD_PARTY_DATA.md).
