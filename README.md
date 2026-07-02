# wikititlepair

Generate bidirectional DSL dictionaries from Wikipedia interlanguage links via Wikidata. Works with any two Wikipedia languages. Inspired by ZZ's wikipedia titlepair.

## What it does

Download the Wikidata `wb_items_per_site` dump and extract interlanguage links between any two Wikipedia language editions. Supports all ~300 Wikipedia languages. Outputs an ABBYY Lingvo `.dsl` dictionary file where clicking a word in one language jumps to its entry in the other language.

## Pre-built dictionaries

A pre-built **English ↔ Chinese** bi-directional dictionary is included in releases. A scheduled workflow checks weekly for new Wikidata dumps and publishes an updated dictionary if available - no recompilation needed if no code changes.

## Usage

```bash
# English ↔ Chinese
wikititlepair en zh

# Japanese ↔ Korean
wikititlepair ja ko

# French ↔ German (custom output)
wikititlepair fr de -o french-german.dsl

# Allow downloading dump if not cached
wikititlepair en zh --download

# Any two Wikipedia language codes work
wikititlepair es pt
```

## Output format

DSL (ABBYY Lingvo) format with clickable cross-references:

```
#NAME "wikipedia titlepair (en-zh)"
#INDEX_LANGUAGE "en"
#CONTENTS_LANGUAGE "zh"

Music
	<<音乐>>
音乐
	<<Music>>
```

Clicking `音乐` in the Music entry jumps to the `音乐` entry.

## Dependencies

- Rust 1.75+
- `dictzip` (for `.dsl.dz` compression)

## Data source

Uses the Wikidata [`wb_items_per_site` dump](https://dumps.wikimedia.org/wikidatawiki/latest/wikidatawiki-latest-wb_items_per_site.sql.gz) (~1.8GB compressed). This table maps Wikipedia page titles across all language editions via shared Wikidata item IDs.

Dump files are cached in `~/.cache/wikidict/` for reuse across runs.
