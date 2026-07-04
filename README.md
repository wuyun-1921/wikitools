# wikitools

CLI tools for building dictionaries from Wikimedia data.

## Features

- `wikitools pair` — Generate bidirectional DSL dictionaries from Wikipedia interlanguage links via Wikidata
- `wikitools titles` — Extract all article titles from a Wikimedia project and build a dictionary linking each title to its online page

## Usage

```bash
# Build a bidirectional EN↔ZH dictionary, can be any two languages
wikitools pair en zh --download

# Extract all English Wikipedia titles as a URL dictionary, can be any language
wikitools titles en --download

# Extract all Latin Wiktionary titles
wikitools titles la --project wiktionary --download
```

## Build

Requires Rust 1.75+, and `dictd` package (provides `dictzip` for .dsl.dz compression).

```sh
cargo build --release
```

## Formats

| Format | Output | Reader |
|--------|--------|--------|
| DSL | `.dsl.dz` | ABBYY Lingvo, GoldenDict-ng |
| MDX | `.mdx` | MDict, GoldenDict-ng |

MDX conversion: `python scripts/dsl2mdx.py output.dsl`

## Data Sources

- **pair**: [Wikidata `wb_items_per_site` dump](https://dumps.wikimedia.org/wikidatawiki/latest/) (~1.8 GB). Only article titles (no Category/Template/Wikipedia namespaces).
- **titles**: Wikimedia `all-titles-in-ns0` dump (~100-200 MB per language). Complete list of article titles for any Wikimedia project.

Dumps are cached at `~/.cache/wikitools/`. Use `--download` to fetch on demand.
