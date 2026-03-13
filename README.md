# `completion-dictionary`

`completion-dictionary` is a small CLI for building newline-delimited dictionary files and serving rich documentation for [blink-cmp-dictionary](https://github.com/Kaiser-Yang/blink-cmp-dictionary).
It uses Wordnet data through the [wn](https://github.com/goodmami/wn) Python package.

The current release supports two profiles.

- `swedish` using `omw-sv:2.0`
- `english` using `oewn:2025+`

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

- download the required `wn` data if needed
- collect lemmas from the selected lexicon
- deduplicate them exactly
- sort them with `casefold()` for deterministic output

By default, the tool writes into a user data directory instead of the
repo.

On macOS, the default outputs are:

- `~/Library/Application Support/completion-dictionary/sv-omw.dict`
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

This prints a learning/debug view with:

- lemma
- part of speech
- sibling lemmas from the same synset
- ILI id
- gloss

### Render documentation for Blink

```bash
completion-dictionary doc --profile swedish säng
completion-dictionary doc --profile english bed
```

This prints the text that `blink-cmp-dictionary` should show in the documentation window.
If no match exists, it prints nothing and exits successfully.

## blink.cmp configuration

Build the dictionaries first:

```bash
completion-dictionary build --profile swedish
completion-dictionary build --profile english
```

Then point `blink-cmp-dictionary` at the generated files and override the documentation command so it uses `completion-dictionary` instead of the system `wn` binary.

On macOS, the default dictionary paths are:

- `~/Library/Application Support/completion-dictionary/sv-omw.dict`
- `~/Library/Application Support/completion-dictionary/en-oewn.dict`

Example Blink config:

```lua
local completion_dictionary = vim.fn.expand("~/.local/bin/completion-dictionary")
local swedish_dict = vim.fn.expand("~/Library/Application Support/completion-dictionary/sv-omw.dict")
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
- V1 is lemma-only on purpose. It does not try to generate inflected forms.
