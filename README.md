# wikititlepair

Generate bidirectional DSL and MDX dictionaries from Wikipedia interlanguage links
via Wikidata. Any two languages. Inspired by ZZ's wikipedia titlepair.

## Releases

Pre-built **English ↔ Chinese** dictionaries (DSL + MDX) in [releases](https://github.com/wuyun-1921/wikititlepair/releases).
Updated weekly when Wikidata dump changes.

## Usage

```bash
wikititlepair en zh           # any two Wikipedia language codes
wikititlepair en zh --download # fetch dump if not cached
```

## Build

```sh
cargo build --release
# requires dictzip (dictd package) for .dsl.dz compression
```

## Formats

| Format | Output | Reader |
|--------|--------|--------|
| DSL | `.dsl.dz` | ABBYY Lingvo, GoldenDict-ng |
| MDX | `.mdx` | MDict, GoldenDict-ng |

MDX conversion: `python scripts/dsl2mdx.py output.dsl`

## Data

[Wikidata `wb_items_per_site` dump](https://dumps.wikimedia.org/wikidatawiki/latest/wikidatawiki-latest-wb_items_per_site.sql.gz) (~1.8 GB). Cached at `~/.cache/wikidict/`. Only article titles (no Category/Template/Wikipedia pages).
