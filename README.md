# wikititlepair

Generate bidirectional DSL dictionaries from Wikidata interlanguage links. Inspired by ZZ's wikipedia titlepair.

## What it does

Downloads the Wikidata `wb_items_per_site` dump and extracts interlanguage links between any two Wikipedia language editions. Outputs an ABBYY Lingvo `.dsl` dictionary file where clicking a word in one language jumps to its entry in the other language.

## Usage

```bash
# Basic usage (outputs wikipedia-titlepair-en-zh-YYYYMMDD.dsl)
wikidict en zh

# Custom output file
wikidict en zh -o my-dictionary.dsl

# Use cached dump, don't download
wikidict en zh

# Allow downloading if dump not cached
wikidict en zh --download
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

## Data Source

Uses the Wikidata [`wb_items_per_site` dump](https://dumps.wikimedia.org/wikidatawiki/latest/wikidatawiki-latest-wb_items_per_site.sql.gz) (~1.8GB compressed). This table maps Wikipedia page titles across all language editions via shared Wikidata item IDs.

## Cache

Dump files are cached in `~/.cache/wikidict/`.
