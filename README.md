# `completion-dictionary`

`completion-dictionary` is a small CLI for building newline-delimited dictionary files and serving rich documentation for [blink-cmp-dictionary](https://github.com/Kaiser-Yang/blink-cmp-dictionary).

The current release supports two profiles, each backed by a different data source.

- `swedish` — completion vocabulary from the [hunspell-sv](https://github.com/yeager/hunspell-sv) Swedish dictionary (including inflected forms), and synonyms from the Synlex / LibreOffice Swedish thesaurus.
- `english` — lemma completion and WordNet documentation from the [Open English WordNet](https://en-word.net/) through the [wn](https://github.com/goodmami/wn) Python package.

Third-party data is downloaded at build time, not bundled in the package. See
[`THIRD_PARTY_DATA.md`](THIRD_PARTY_DATA.md) for sources, licenses, and attribution.

`blink-cmp-dictionary` reads completion candidates from a plain text dictionary file.
Its documentation hook can call an external command for richer item details.
`completion-dictionary` handles both sides: it builds the static `.dict` file and renders documentation for individual lemmas on demand.

## Install

Install directly from GitHub with `uv`:

```bash
uv tool install git+https://github.com/edvinsyk/completion_dictionary
```

To install a specific tag, branch, or commit:

```bash
uv tool install git+https://github.com/edvinsyk/completion_dictionary@<ref>
```

Examples:

```bash
uv tool install git+https://github.com/edvinsyk/completion_dictionary@main
uv tool install git+https://github.com/edvinsyk/completion_dictionary@v0.1.0
```

If you want to update an existing installation from GitHub:

```bash
uv tool install --force git+https://github.com/edvinsyk/completion_dictionary@main
```

For local development from a checkout, use an editable install:

```bash
uv tool install --editable .
```

If you update local code later, reinstall with:

```bash
uv tool install --editable --force .
```

To see where `uv` exposes tool executables:

```bash
uv tool dir --bin
```

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

- download and cache the required data for the selected profile if needed
- collect the completion vocabulary from the profile's source
- deduplicate it exactly
- sort it with `casefold()` for deterministic output

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

For the `swedish` profile it reports whether the word is present in the
completion dictionary (built from Hunspell forms), whether a thesaurus entry
exists, and lists its synonyms:

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

This prints the text that `blink-cmp-dictionary` should show in the documentation window.
If no match exists, it prints nothing and exits successfully.

For the `swedish` profile the doc shows the headword followed by a `Synonymer`
(synonyms) block:

```text
säng

Synonymer
bädd, brits
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
local completion_dictionary = vim.fn.expand("~/.local/bin/completion-dictionary")
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

If your `uv tool dir --bin` output is not `~/.local/bin`, update `completion_dictionary` to that absolute path.

## Notes

- Generated dictionary files are UTF-8 text with one completion candidate per line.
- Completion candidates come from the static `.dict` files, not from Python at typing time.
- Python is only used for `lookup` and `doc`, so candidate lookup stays fast.
- Generated dictionaries are not intended to be committed to the repository or shipped in the Python package.
- The `english` profile is lemma-only: each line is a single WordNet lemma with no inflected forms. The `swedish` profile is not lemma-only — it is built from Hunspell expanded forms, so inflected forms are included as completion candidates.
- Third-party source data is downloaded at build time and cached under the user data directory. See [`THIRD_PARTY_DATA.md`](THIRD_PARTY_DATA.md).
