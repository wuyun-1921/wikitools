# wikitools Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename `wikititlepair` to `wikitools`, split monolith into modules, add `titles` subcommand that extracts all article titles from a Wikimedia project dump and emits a DSL dictionary where each definition is a link to the live page.

**Architecture:** Single Rust binary using clap derive subcommands (`pair`, `titles`). Bare `wikitools` prints help. Core is split into focused modules — `error`, `escape`, `download`, `dsl`, `pair`, `titles` — each independently testable. Downloads use `~/.cache/wikitools/`, default to no-download with `--download` flag.

**Tech Stack:** Rust 2024 edition, clap 4 (derive), flate2, rayon, thiserror, dirs.

## Global Constraints

- Cache directory: `~/.cache/wikitools/`
- Bare `wikitools` with no subcommand prints CLI help (not a default subcommand)
- No automatic downloading — both subcommands require `--download` flag to fetch dumps
- `--project` flag on `titles` defaults to `wikipedia`; valid: `wikipedia`, `wiktionary`, `wikibooks`, `wikiquote`, `wikisource`, `wikinews`, `wikiversity`, `wikivoyage`
- Pair output prefix: `wikipedia-titlepair-{lang_a}-{lang_b}-{date}`
- Titles output prefix: `wikipedia-titles-{lang}-{date}`
- DSL escaping is shared across all features
- `all-titles-in-ns0.gz` dumps are already namespace-0-only — no `is_non_article` filtering needed for titles

---

### Task 1: Rename package + extract error and escape modules

**Files:**
- Modify: `Cargo.toml`
- Create: `src/error.rs`
- Create: `src/escape.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `WikiDictError` enum (Io, Parse), `pub type Result<T>`, `pub fn escape_dsl(s: &str) -> String`, `pub fn unquote(s: &str) -> String`

- [ ] **Step 1: Rename package in Cargo.toml**

```toml
[package]
name = "wikitools"
version = "0.1.0"
edition = "2024"
```

- [ ] **Step 2: Create src/error.rs**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WikiDictError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, WikiDictError>;
```

- [ ] **Step 3: Create src/escape.rs**

Copy `escape_dsl`, `unquote` functions verbatim from main.rs. No changes to logic.

```rust
/// Escape special characters in DSL headwords and cross-references.
pub fn escape_dsl(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '{' => result.push_str("\\{"),
            '}' => result.push_str("\\}"),
            '[' => result.push_str("\\["),
            ']' => result.push_str("\\]"),
            '#' => result.push_str("\\#"),
            '@' => result.push_str("\\@"),
            '<' => result.push_str("\\<"),
            '>' => result.push_str("\\>"),
            '~' => result.push_str("\\~"),
            '^' => result.push_str("\\^"),
            _ => result.push(ch),
        }
    }
    result
}

pub fn unquote(s: &str) -> String {
    let s = s.trim();
    if !s.starts_with('\'') || !s.ends_with('\'') {
        return s.to_string();
    }
    let inner = &s[1..s.len() - 1];
    let bytes = inner.as_bytes();
    let len = bytes.len();
    let mut result = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        if bytes[i] == b'\\' && i + 1 < len {
            match bytes[i + 1] {
                b'\'' => { result.push(b'\''); i += 2; }
                b'\\' => { result.push(b'\\'); i += 2; }
                b'n' => { result.push(b'\n'); i += 2; }
                b'r' => { result.push(b'\r'); i += 2; }
                b't' => { result.push(b'\t'); i += 2; }
                b'"' => { result.push(b'"'); i += 2; }
                _ => { result.push(bytes[i]); i += 1; }
            }
        } else if bytes[i] == b'\'' && i + 1 < len && bytes[i + 1] == b'\'' {
            result.push(b'\'');
            i += 2;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// MediaWiki namespace canonical names (English).
pub static NON_ARTICLE_PREFIXES: &[&str] = &[
    "Category", "Template", "Wikipedia", "Portal", "Help",
    "Module", "WikiProject", "User", "File", "Image",
    "MediaWiki", "TimedText", "Draft", "Media", "Special",
    "Talk", "WP",
];

/// Returns true if the title is a non-article namespace page.
pub fn is_non_article(title: &str) -> bool {
    for prefix in NON_ARTICLE_PREFIXES {
        let needle = [prefix, ":"].concat();
        if title.starts_with(&needle) {
            return true;
        }
    }
    false
}
```

- [ ] **Step 4: Update main.rs — add mod declarations and use statements, remove moved code**

At top of main.rs, add:
```rust
mod error;
mod escape;

use error::{WikiDictError, Result};
use escape::{escape_dsl, unquote, NON_ARTICLE_PREFIXES, is_non_article};
```

Remove the `#[derive(Error)]`, `WikiDictError`, `type Result`, `escape_dsl`, `unquote`, `NON_ARTICLE_PREFIXES`, `is_non_article` definitions from main.rs.

- [ ] **Step 5: Move escape tests from main.rs to escape.rs**

Add to end of `src/escape.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_dsl_parens() {
        assert_eq!(escape_dsl("Music (2021)"), "Music \\(2021\\)");
        assert_eq!(escape_dsl("C#"), "C\\#");
        assert_eq!(escape_dsl("A < B"), "A \\< B");
        assert_eq!(escape_dsl("x ~ y"), "x \\~ y");
        assert_eq!(escape_dsl("x^2"), "x\\^2");
        assert_eq!(escape_dsl("path\\to"), "path\\\\to");
        assert_eq!(escape_dsl("no escape"), "no escape");
        assert_eq!(escape_dsl("音乐"), "音乐");
    }

    #[test]
    fn test_is_non_article() {
        assert!(is_non_article("Category:Music"));
        assert!(is_non_article("Template:Infobox"));
        assert!(is_non_article("Wikipedia:About"));
        assert!(is_non_article("Help:Contents"));
        assert!(is_non_article("Module:Math"));
        assert!(is_non_article("User:Test"));
        assert!(!is_non_article("Music"));
        assert!(!is_non_article("Doraemon: Story"));
        assert!(!is_non_article("Star Wars: Episode IV"));
    }

    #[test]
    fn test_unquote() {
        assert_eq!(unquote("'hello'"), "hello");
        assert_eq!(unquote("'it\\'s'"), "it's");
        assert_eq!(unquote("'back\\\\slash'"), "back\\slash");
        assert_eq!(unquote("'quote\\\"test\\\"'"), "quote\"test\"");
        assert_eq!(unquote("'new\\nline'"), "new\nline");
    }
}
```

Remove corresponding `#[cfg(test)]` and test functions from main.rs.

- [ ] **Step 6: Run tests**

```bash
cargo test
```

Expected: all 7 tests pass.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: rename to wikitools, extract error and escape modules"
```

---

### Task 2: Extract download module

**Files:**
- Create: `src/download.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `pub fn ensure_dump(cache_dir: &Path, filename: &str, url: &str, allow_download: bool) -> Result<PathBuf>`, `pub fn ensure_wikidata_dump(cache_dir: &Path, allow_download: bool) -> Result<PathBuf>`, `pub fn get_dump_date(listing_url: &str, needle: &str) -> Result<String>`, `pub fn wikidata_listing_url() -> String`, `pub fn wikidata_dump_url() -> String`, `pub fn wikidata_dump_filename() -> &'static str`

- [ ] **Step 1: Create src/download.rs**

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{WikiDictError, Result};

const WIKIDATA_FILENAME: &str = "wikidatawiki-latest-wb_items_per_site.sql.gz";

pub fn wikidata_listing_url() -> String {
    "https://dumps.wikimedia.org/wikidatawiki/latest/".to_string()
}

pub fn wikidata_dump_url() -> String {
    format!(
        "https://dumps.wikimedia.org/wikidatawiki/latest/{}",
        WIKIDATA_FILENAME
    )
}

pub fn wikidata_dump_filename() -> &'static str {
    WIKIDATA_FILENAME
}

pub fn download_file(url: &str, path: &Path) -> Result<()> {
    let filename = path.file_name().unwrap().to_str().unwrap();

    if path.exists() {
        let backup = path.with_extension("sql.gz.old");
        if backup.exists() {
            std::fs::remove_file(&backup)?;
        }
        std::fs::rename(path, &backup)?;
        eprintln!("  renamed {} to {}", filename, backup.file_name().unwrap().to_str().unwrap());
    }

    let status = Command::new("curl")
        .args([
            "-C", "-", "-L", "-o", path.to_str().unwrap(), "-A", "wikitools/0.1",
            "-sS", "--fail", url,
        ])
        .status()?;

    if !status.success() {
        return Err(WikiDictError::Parse(format!("curl failed for {}", filename)));
    }

    Ok(())
}

/// Generic: ensure a dump file exists, downloading if allowed.
pub fn ensure_dump(cache_dir: &Path, filename: &str, url: &str, allow_download: bool) -> Result<PathBuf> {
    let path = cache_dir.join(filename);

    if path.exists() && std::fs::metadata(&path)?.len() > 1000 {
        eprintln!("Using cached dump: {}", filename);
        return Ok(path);
    }

    if !allow_download {
        return Err(WikiDictError::Parse(
            format!("Dump '{}' not found in {}. Use --download to fetch it.", filename, cache_dir.display())
        ));
    }

    std::fs::create_dir_all(cache_dir)?;

    eprintln!("Downloading {} ...", filename);
    eprintln!("  {}", url);
    download_file(url, &path)?;
    eprintln!("  done");

    Ok(path)
}

/// Wikidata wb_items_per_site dump.
pub fn ensure_wikidata_dump(cache_dir: &Path, allow_download: bool) -> Result<PathBuf> {
    ensure_dump(cache_dir, WIKIDATA_FILENAME, &wikidata_dump_url(), allow_download)
}

/// Scrape a Wikimedia dump listing page for a date pattern near a needle string.
pub fn get_dump_date(listing_url: &str, needle: &str) -> Result<String> {
    let output = Command::new("curl")
        .args(["-s", listing_url])
        .output()?;

    let html = String::from_utf8_lossy(&output.stdout);
    for line in html.lines() {
        if line.contains(needle) {
            let chars: Vec<char> = line.chars().collect();
            for i in 0..chars.len().saturating_sub(10) {
                if chars[i].is_ascii_digit()
                    && chars[i + 1].is_ascii_digit()
                    && chars[i + 2] == '-'
                    && chars[i + 3].is_ascii_uppercase()
                    && chars[i + 4].is_ascii_lowercase()
                    && chars[i + 5].is_ascii_lowercase()
                    && chars[i + 6] == '-'
                    && chars[i + 7].is_ascii_digit()
                    && chars[i + 8].is_ascii_digit()
                    && chars[i + 9].is_ascii_digit()
                    && chars[i + 10].is_ascii_digit()
                {
                    let date_str: String = chars[i..i + 11].iter().collect();
                    let parts: Vec<&str> = date_str.split('-').collect();
                    if parts.len() == 3 {
                        let day = parts[0];
                        let month = match parts[1] {
                            "Jan" => "01", "Feb" => "02", "Mar" => "03",
                            "Apr" => "04", "May" => "05", "Jun" => "06",
                            "Jul" => "07", "Aug" => "08", "Sep" => "09",
                            "Oct" => "10", "Nov" => "11", "Dec" => "12",
                            _ => continue,
                        };
                        let year = parts[2];
                        return Ok(format!("{}{}{}", year, month, day));
                    }
                }
            }
        }
    }

    Err(WikiDictError::Parse(format!("Could not parse dump date from {}", listing_url)))
}
```

- [ ] **Step 2: Update main.rs**

Add at top:
```rust
mod download;

use download::{ensure_wikidata_dump, get_dump_date, wikidata_listing_url};
```

Remove the now-moved functions from main.rs: `dump_url`, `get_dump_date`, `download_file`, `ensure_dump`.

Update `main()` to use the new module functions:
```rust
let dump_path = ensure_wikidata_dump(&cache_dir, cli.download)?;
let dump_date = get_dump_date(&wikidata_listing_url(), "wb_items_per_site.sql.gz")
    .unwrap_or_else(|_| "latest".to_string());
```

Remove the `use std::process::Command;` import from main.rs (if no longer directly used).

- [ ] **Step 3: Run tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: extract download module"
```

---

### Task 3: Extract dsl module

**Files:**
- Create: `src/dsl.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `pub fn write_dsl(output: &Path, name: &str, index_lang: &str, contents_lang: &str, entries: &[(String, String)]) -> Result<()>`, `pub fn compress_dictzip(path: &Path) -> bool`

- [ ] **Step 1: Create src/dsl.rs**

```rust
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::Command;

use crate::error::Result;

/// Write entries to a DSL file. Entries are pre-formatted (headword, definition body).
pub fn write_dsl(
    output: &Path,
    name: &str,
    index_lang: &str,
    contents_lang: &str,
    entries: &[(String, String)],
) -> Result<()> {
    let mut file = BufWriter::new(File::create(output)?);

    writeln!(file, "#NAME \"{}\"", name)?;
    writeln!(file, "#INDEX_LANGUAGE \"{}\"", index_lang)?;
    writeln!(file, "#CONTENTS_LANGUAGE \"{}\"", contents_lang)?;
    writeln!(file)?;

    for (headword, body) in entries {
        write!(file, "{}\n\t{}\n", headword, body)?;
    }

    file.flush()?;
    Ok(())
}

/// Compress DSL file with dictzip. Returns false if dictzip unavailable (file kept uncompressed).
pub fn compress_dictzip(path: &Path) -> bool {
    let dz_output = path.with_extension("dsl.dz");
    eprintln!("Compressing with dictzip...");
    match Command::new("dictzip").arg(path.to_str().unwrap()).status() {
        Ok(s) if s.success() => {
            eprintln!("  {} created", dz_output.display());
            true
        }
        _ => {
            eprintln!("  dictzip unavailable or failed (dsl file kept)");
            false
        }
    }
}
```

- [ ] **Step 2: Update main.rs**

Add:
```rust
mod dsl;

use dsl::{write_dsl, compress_dictzip};
```

Replace the DSL writing block in `main()` (from `let mut file = BufWriter::new(...)` through the dictzip call) with:

```rust
// Sort lang codes for consistent metadata
let mut meta_langs = [cli.lang_a.as_str(), cli.lang_b.as_str()];
meta_langs.sort();
let lang_pair = format!("{}-{}", meta_langs[0], meta_langs[1]);

write_dsl(
    &output,
    &format!("wikipedia titlepair ({})", lang_pair),
    &lang_pair,
    &lang_pair,
    &escaped,
)?;

eprintln!("\nDone! {} entries written to {}", entry_count, output.display());

compress_dictzip(&output);
```

Remove `use std::fs::File;` and `use std::io::{BufWriter, Read, Write};` from main.rs (keep `Read` if still needed by pair parsing — it's needed by `parse_dump`). Actually `parse_dump` uses `File` and `Read` for gz decoding. Keep those for now, they'll move to pair.rs in the next task.

Remove `use std::process::Command;` from main.rs (moved to dsl.rs and download.rs).

- [ ] **Step 3: Run tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: extract dsl module"
```

---

### Task 4: Extract pair module

**Files:**
- Create: `src/pair.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `pub fn parse_dump(path: &Path, lang_a: &str, lang_b: &str) -> Result<Vec<(String, String)>>`
- Consumes: `escape::escape_dsl`, `escape::is_non_article`, `escape::unquote`, `error::Result`

- [ ] **Step 1: Create src/pair.rs**

Move `parse_insert_line` and `parse_dump` from main.rs. Add imports at top:

```rust
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use rayon::prelude::*;

use crate::error::Result;
use crate::escape::{is_non_article, unquote};

/// Parse one INSERT statement, extracting (item_id, site_id, title) tuples
/// for rows matching target languages.
fn parse_insert_line(
    line: &str,
    site_a: &str,
    site_b: &str,
    items_a: &mut HashMap<u32, String>,
    items_b: &mut HashMap<u32, String>,
) {
    // ... verbatim copy from main.rs ...
}

/// Parse Wikidata wb_items_per_site dump into bidirectional title pairs.
pub fn parse_dump(path: &Path, lang_a: &str, lang_b: &str) -> Result<Vec<(String, String)>> {
    // ... verbatim copy from main.rs ...
}
```

The complete `parse_insert_line` and `parse_dump` bodies are copied verbatim from current main.rs — no logic changes.

- [ ] **Step 2: Move pair tests to pair.rs**

Add to end of `src/pair.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_insert_line() {
        let sql = "INSERT INTO wb_items_per_site VALUES (1,42,'enwiki','Music'),(2,42,'zhwiki','音乐')";
        let mut items_a = HashMap::new();
        let mut items_b = HashMap::new();
        parse_insert_line(sql, "enwiki", "zhwiki", &mut items_a, &mut items_b);
        assert_eq!(items_a.get(&42).unwrap(), "Music");
        assert_eq!(items_b.get(&42).unwrap(), "音乐");
    }

    #[test]
    fn test_parse_skips_non_article() {
        let sql = "INSERT INTO wb_items_per_site VALUES (1,1,'enwiki','Music'),(2,1,'zhwiki','音乐'),(3,2,'enwiki','Category:Music'),(4,2,'zhwiki','Category:音乐')";
        let mut items_a = HashMap::new();
        let mut items_b = HashMap::new();
        parse_insert_line(sql, "enwiki", "zhwiki", &mut items_a, &mut items_b);
        assert_eq!(items_a.get(&1).unwrap(), "Music");
        assert_eq!(items_b.get(&1).unwrap(), "音乐");
        assert!(!items_a.contains_key(&2));
        assert!(!items_b.contains_key(&2));
    }

    #[test]
    fn test_full_pipeline_tiny_dump() {
        let sql = "INSERT INTO wb_items_per_site VALUES (1,1,'enwiki','Music'),(2,1,'zhwiki','音乐');\nINSERT INTO wb_items_per_site VALUES (3,2,'enwiki','Hello'),(4,2,'zhwiki','你好');\nINSERT INTO wb_items_per_site VALUES (5,3,'enwiki','Same'),(6,3,'zhwiki','Same');\n";
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(sql.as_bytes()).unwrap();
        let compressed = gz.finish().unwrap();

        let tmp = std::env::temp_dir().join("wikidict_test_dump.sql.gz");
        std::fs::write(&tmp, &compressed).unwrap();

        let entries = parse_dump(&tmp, "en", "zh").unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(entries.len(), 4);
        assert!(entries.contains(&("Music".to_string(), "音乐".to_string())));
        assert!(entries.contains(&("音乐".to_string(), "Music".to_string())));
        assert!(entries.contains(&("Hello".to_string(), "你好".to_string())));
        assert!(entries.contains(&("你好".to_string(), "Hello".to_string())));
        assert!(!entries.contains(&("Same".to_string(), "Same".to_string())));
    }

    #[test]
    fn test_dsl_output_format() {
        let entries = vec![
            ("Music".to_string(), "音乐".to_string()),
            ("音乐".to_string(), "Music".to_string()),
        ];
        let escaped: Vec<_> = entries
            .into_iter()
            .map(|(a, b)| (crate::escape::escape_dsl(&a), crate::escape::escape_dsl(&b)))
            .collect();

        let tmp = std::env::temp_dir().join("wikidict_test_output.dsl");
        {
            use std::fs::File;
            use std::io::{BufWriter, Write};

            let mut f = BufWriter::new(File::create(&tmp).unwrap());
            writeln!(f, "#NAME \"wikipedia titlepair (en-zh)\"").unwrap();
            writeln!(f, "#INDEX_LANGUAGE \"en-zh\"").unwrap();
            writeln!(f, "#CONTENTS_LANGUAGE \"en-zh\"").unwrap();
            writeln!(f).unwrap();
            for (a, b) in &escaped {
                write!(f, "{}\n\t<<{}>>\n", a, b).unwrap();
            }
        }

        let content = std::fs::read_to_string(&tmp).unwrap();
        std::fs::remove_file(&tmp).ok();

        assert!(content.contains("#NAME \"wikipedia titlepair (en-zh)\""));
        assert!(content.contains("Music\n\t<<音乐>>"));
        assert!(content.contains("音乐\n\t<<Music>>"));
    }
}
```

Remove corresponding tests from main.rs.

- [ ] **Step 3: Update main.rs**

Add:
```rust
mod pair;

use pair::parse_dump;
```

Remove `parse_insert_line` and `parse_dump` from main.rs.
Remove `use std::collections::HashMap;` and `use std::io::Read;` from main.rs.
Remove `use flate2::read::GzDecoder;` and `use rayon::prelude::*;` from main.rs.
Keep `use clap::Parser;`, `use thiserror::Error;` (moved to error.rs — remove), `use std::path::{Path, PathBuf};`, `use std::error::Error;` in main.

Wait — at this point main.rs still uses `rayon` directly for the parallel escape step. Let me check...

Current main.rs does:
```rust
let escaped: Vec<(String, String)> = entries
    .into_par_iter()
    .map(|(a, b)| (escape_dsl(&a), escape_dsl(&b)))
    .collect();
```

That uses `rayon::prelude::*` for `into_par_iter`. So we still need that in main.rs for now. Keep `use rayon::prelude::*;`.

And we need `use std::io::Read;` — no, that was only for `GzDecoder::read_to_end`. Keep it in pair.rs.

OK, the remaining imports in main.rs should be:
```rust
use std::path::{Path, PathBuf};
use rayon::prelude::*;

use clap::Parser;

mod error;
mod escape;
mod download;
mod dsl;
mod pair;

use error::Result;
use escape::escape_dsl;
use download::{ensure_wikidata_dump, get_dump_date, wikidata_listing_url};
use dsl::{write_dsl, compress_dictzip};
use pair::parse_dump;
```

- [ ] **Step 4: Run tests**

```bash
cargo test
```

Expected: all tests pass (7 tests across modules, plus the ones in pair.rs).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: extract pair module"
```

---

### Task 5: CLI restructure with subcommands

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `Cli` enum with `Pair` and `Titles` variants, each with their own args. `main()` dispatches.
- Consumes: All extracted modules.

- [ ] **Step 1: Rewrite main.rs with clap subcommands**

```rust
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rayon::prelude::*;

mod error;
mod escape;
mod download;
mod dsl;
mod pair;
mod titles;

use error::Result;
use escape::escape_dsl;
use download::{ensure_wikidata_dump, get_dump_date, wikidata_listing_url};
use dsl::{write_dsl, compress_dictzip};
use pair::parse_dump;

#[derive(Parser)]
#[command(name = "wikitools")]
#[command(about = "Wikipedia dictionary tools")]
#[command(subcommand_required = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate bidirectional title-pair dictionary from Wikidata
    Pair {
        /// First language code (e.g., en, zh, ja)
        lang_a: String,

        /// Second language code (e.g., zh, ja, ko)
        lang_b: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Cache directory for dumps
        #[arg(long, default_value = "~/.cache/wikitools")]
        cache_dir: PathBuf,

        /// Allow downloading if dump not found
        #[arg(long)]
        download: bool,
    },

    /// Extract all article titles and generate a URL dictionary
    Titles {
        /// Language code (e.g., en, zh, ja)
        lang: String,

        /// Wikimedia project (default: wikipedia)
        #[arg(long, default_value = "wikipedia")]
        project: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Cache directory for dumps
        #[arg(long, default_value = "~/.cache/wikitools")]
        cache_dir: PathBuf,

        /// Allow downloading if dump not found
        #[arg(long)]
        download: bool,
    },
}

fn resolve_cache_dir(raw: &PathBuf) -> PathBuf {
    if raw.to_str() == Some("~/.cache/wikitools") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache/wikitools")
    } else {
        raw.clone()
    }
}

fn run_pair(
    lang_a: &str,
    lang_b: &str,
    output: Option<PathBuf>,
    cache_dir: &PathBuf,
    download: bool,
) -> Result<()> {
    let cache_dir = resolve_cache_dir(cache_dir);
    let dump_path = ensure_wikidata_dump(&cache_dir, download)?;
    let dump_date = get_dump_date(&wikidata_listing_url(), "wb_items_per_site.sql.gz")
        .unwrap_or_else(|_| "latest".to_string());

    let output = output.unwrap_or_else(|| {
        PathBuf::from(format!("wikipedia-titlepair-{}-{}-{}.dsl", lang_a, lang_b, dump_date))
    });

    eprintln!("\nParsing dump...");
    let entries = parse_dump(&dump_path, lang_a, lang_b)?;
    let entry_count = entries.len();

    eprintln!("  Escaping {} entries to DSL...", entry_count);
    let escaped: Vec<(String, String)> = entries
        .into_par_iter()
        .map(|(a, b)| (escape_dsl(&a), format!("<<{}>>", escape_dsl(&b))))
        .collect();

    let mut meta_langs = [lang_a, lang_b];
    meta_langs.sort();
    let lang_pair = format!("{}-{}", meta_langs[0], meta_langs[1]);

    write_dsl(
        &output,
        &format!("wikipedia titlepair ({})", lang_pair),
        &lang_pair,
        &lang_pair,
        &escaped,
    )?;

    eprintln!("\nDone! {} entries written to {}", entry_count, output.display());
    compress_dictzip(&output);
    Ok(())
}

fn run_titles(
    lang: &str,
    project: &str,
    output: Option<PathBuf>,
    cache_dir: &PathBuf,
    download: bool,
) -> Result<()> {
    let cache_dir = resolve_cache_dir(cache_dir);
    let dump_path = titles::ensure_titles_dump(&cache_dir, lang, project, download)?;
    let listing_url = titles::titles_listing_url(lang, project);
    let dump_date = get_dump_date(&listing_url, "all-titles-in-ns0.gz")
        .unwrap_or_else(|_| "latest".to_string());

    let output = output.unwrap_or_else(|| {
        PathBuf::from(format!("wikipedia-titles-{}-{}.dsl", lang, dump_date))
    });

    eprintln!("\nParsing titles dump...");
    let entries = titles::parse_all_titles(&dump_path, lang, project)?;
    let entry_count = entries.len();

    write_dsl(
        &output,
        &format!("wikipedia titles ({})", lang),
        lang,
        lang,
        &entries,
    )?;

    eprintln!("\nDone! {} entries written to {}", entry_count, output.display());
    compress_dictzip(&output);
    Ok(())
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Pair { lang_a, lang_b, output, cache_dir, download } => {
            run_pair(&lang_a, &lang_b, output, &cache_dir, download)?;
        }
        Command::Titles { lang, project, output, cache_dir, download } => {
            run_titles(&lang, &project, output, &cache_dir, download)?;
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test
```

Expected: compilation fails because `titles` module doesn't exist yet. This is expected — Task 6 creates it.

Actually, let me reorder: create a stub `src/titles.rs` first to make it compile.

- [ ] **Step 2 (revised): Create stub src/titles.rs**

```rust
use std::path::{Path, PathBuf};

use crate::error::Result;

pub fn titles_listing_url(lang: &str, project: &str) -> String {
    format!("https://dumps.wikimedia.org/{}{}/latest/", lang, project)
}

pub fn ensure_titles_dump(
    cache_dir: &Path,
    lang: &str,
    project: &str,
    allow_download: bool,
) -> Result<PathBuf> {
    let filename = format!("{}{}-latest-all-titles-in-ns0.gz", lang, project);
    let url = format!(
        "https://dumps.wikimedia.org/{}{}/latest/{}{}-latest-all-titles-in-ns0.gz",
        lang, project, lang, project
    );
    crate::download::ensure_dump(cache_dir, &filename, &url, allow_download)
}

pub fn parse_all_titles(
    _path: &Path,
    _lang: &str,
    _project: &str,
) -> Result<Vec<(String, String)>> {
    // TODO: implement in Task 6
    Ok(Vec::new())
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test
```

Expected: all existing tests pass (titles module is stub, not exercised).

- [ ] **Step 4: Verify CLI works**

```bash
cargo run -- --help
```

Expected: Shows wikitools help with `pair` and `titles` subcommands.

```bash
cargo run -- pair --help
cargo run -- titles --help
```

Expected: Each subcommand shows its own flags.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add clap subcommands, stub titles module"
```

---

### Task 6: Implement titles parsing

**Files:**
- Modify: `src/titles.rs`

**Interfaces:**
- Produces: `pub fn parse_all_titles(path: &Path, lang: &str, project: &str) -> Result<Vec<(String, String)>>`
- Consumes: `escape::escape_dsl`, `download::ensure_dump`, `error::Result`

- [ ] **Step 1: Write failing test in titles.rs**

Append to `src/titles.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_all_titles_basic() {
        // Build a minimal gzipped all-titles XML with 3 articles
        let xml = r#"<mediawiki xmlns="http://www.mediawiki.org/xml/export-0.11/" xsi:schemaLocation="http://www.mediawiki.org/xml/export-0.11/ http://www.mediawiki.org/xml/export-0.11.xsd" version="0.11" xml:lang="en">
  <siteinfo>
    <sitename>Wikipedia</sitename>
    <dbname>enwiki</dbname>
    <base>https://en.wikipedia.org/wiki/Main_Page</base>
  </siteinfo>
  <page>
    <title>Music</title>
    <ns>0</ns>
    <id>1</id>
  </page>
  <page>
    <title>Hello World</title>
    <ns>0</ns>
    <id>2</id>
  </page>
  <page>
    <title>C++</title>
    <ns>0</ns>
    <id>3</id>
  </page>
</mediawiki>"#;

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(xml.as_bytes()).unwrap();
        let compressed = gz.finish().unwrap();

        let tmp = std::env::temp_dir().join("wikitools_test_titles.xml.gz");
        std::fs::write(&tmp, &compressed).unwrap();

        let entries = parse_all_titles(&tmp, "en", "wikipedia").unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(entries.len(), 3);

        // Find Music entry
        let music = entries.iter().find(|(h, _)| h == "Music").unwrap();
        assert!(music.1.contains("https://en.wikipedia.org/wiki/Music"));

        // Find Hello World — space → underscore in URL
        let hello = entries.iter().find(|(h, _)| h == "Hello World").unwrap();
        assert!(hello.1.contains("Hello_World"));

        // C++ — special chars percent-encoded
        let cpp = entries.iter().find(|(h, _)| h == "C++").unwrap();
        assert!(cpp.1.contains("C%2B%2B"));
    }

    #[test]
    fn test_parse_all_titles_wiktionary() {
        let xml = r#"<mediawiki xmlns="http://www.mediawiki.org/xml/export-0.11/">
  <page><title>word</title><ns>0</ns><id>1</id></page>
</mediawiki>"#;

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(xml.as_bytes()).unwrap();
        let compressed = gz.finish().unwrap();

        let tmp = std::env::temp_dir().join("wikitools_test_wiktionary.xml.gz");
        std::fs::write(&tmp, &compressed).unwrap();

        let entries = parse_all_titles(&tmp, "en", "wiktionary").unwrap();
        std::fs::remove_file(&tmp).ok();

        assert_eq!(entries.len(), 1);
        assert!(entries[0].1.contains("https://en.wiktionary.org/wiki/word"));
    }

    #[test]
    fn test_title_url_encoding() {
        let entries = parse_all_titles_inner("en", "wikipedia", &["C# (programming language)"]);

        assert_eq!(entries.len(), 1);
        let (_headword, body) = &entries[0];
        // URL must contain percent-encoded space and #
        assert!(body.contains("C%23_(programming_language)"));
        // Headword is human-readable, DSL-escaped
        assert_eq!(entries[0].0, "C\\# \\(programming language\\)");
    }
}
```

- [ ] **Step 2: Run test to confirm failure**

```bash
cargo test test_parse_all_titles
```

Expected: FAIL — `parse_all_titles` is a stub returning empty vec.

- [ ] **Step 3: Implement parse_all_titles**

Replace the stub `parse_all_titles` in `titles.rs` with the real implementation:

```rust
use std::fs::File;
use std::io::Read;

use flate2::read::GzDecoder;
use rayon::prelude::*;

use crate::error::Result;
use crate::escape::escape_dsl;

// ... (keep titles_listing_url and ensure_titles_dump from Task 5 stub)

/// Extract all article titles from an all-titles-in-ns0 dump.
/// Returns (escaped_headword, "<a href=\"url\">url</a>") pairs.
pub fn parse_all_titles(path: &Path, lang: &str, project: &str) -> Result<Vec<(String, String)>> {
    let file = File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut contents = Vec::new();
    decoder.read_to_end(&mut contents)?;

    // Convert to string - all-titles dumps are reasonably sized
    let xml = String::from_utf8_lossy(&contents);

    let titles = extract_titles(&xml);

    let base_url = format!("https://{}.{}.org/wiki/", lang, project);

    let mut entries: Vec<(String, String)> = titles
        .par_iter()
        .map(|title| {
            let escaped = escape_dsl(title);
            let url_title = url_encode_title(title);
            let url = format!("{}{}", base_url, url_title);
            let body = format!("<a href=\"{}\">{}</a>", url, url);
            (escaped, body)
        })
        .collect();

    entries.par_sort();
    entries.dedup();
    Ok(entries)
}

/// Scan XML for <title> tags inside <page> blocks.
/// all-titles-in-ns0 dumps are namespace-0 only, so no namespace filtering needed.
fn extract_titles(xml: &str) -> Vec<String> {
    let mut titles = Vec::new();
    let bytes = xml.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;

    while pos < len {
        // Find next <page> tag
        let page_start = match find_after(bytes, pos, b"<page>") {
            Some(p) => p,
            None => break,
        };

        // Find next </page> to bound our search
        let page_end = match find_after(bytes, page_start, b"</page>") {
            Some(p) => p - 7, // back to start of </page>
            None => len,
        };

        // Find <title> within this page
        let title_start = match find_after(bytes, page_start, b"<title>") {
            Some(p) => p,
            None => {
                pos = page_end;
                continue;
            }
        };

        // Must be before </page>
        if title_start >= page_end {
            pos = page_end;
            continue;
        }

        // Find </title>
        let title_end = match find_after(bytes, title_start, b"</title>") {
            Some(p) => p - 8,
            None => {
                pos = page_end;
                continue;
            }
        };

        if title_end > title_start {
            if let Ok(s) = std::str::from_utf8(&bytes[title_start..title_end]) {
                if !s.is_empty() {
                    titles.push(s.to_string());
                }
            }
        }

        pos = page_end;
    }

    titles
}

/// Find `needle` in `haystack` starting from `start`, return position just after the match.
fn find_after(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    let pos = haystack[start..]
        .windows(needle.len())
        .position(|w| w == needle)?;
    Some(start + pos + needle.len())
}

/// Encode a page title for use in a URL path segment.
/// Spaces → underscores. Special chars → percent-encoded per RFC 3986.
fn url_encode_title(title: &str) -> String {
    let mut result = String::with_capacity(title.len() + 16);
    for b in title.as_bytes() {
        match *b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                result.push(*b as char);
            }
            b' ' => result.push('_'),
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

/// Internal helper for testing URL encoding without full XML round-trip.
#[cfg(test)]
fn parse_all_titles_inner(lang: &str, project: &str, titles: &[&str]) -> Vec<(String, String)> {
    let base_url = format!("https://{}.{}.org/wiki/", lang, project);
    titles
        .iter()
        .map(|title| {
            let escaped = escape_dsl(title);
            let url_title = url_encode_title(title);
            let url = format!("{}{}", base_url, url_title);
            let body = format!("<a href=\"{}\">{}</a>", url, url);
            (escaped, body)
        })
        .collect()
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test
```

Expected: all tests pass, including new titles tests.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement titles parsing with URL generation"
```

---

### Task 7: Update GitHub Actions workflows

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/update-dictionary.yml`

- [ ] **Step 1: Update release.yml**

Replace all `wikititlepair` references:

1. Binary name in package steps: `wikititlepair` → `wikitools`
2. Artifact names: `wikititlepair-*` → `wikitools-*`
3. Dictionary job: update binary invocation and cache path

Key changes:

```yaml
# Package steps (lines 46, 53):
tar czf ../../../wikitools-${{ matrix.target }}.tar.gz wikitools
7z a ../../../wikitools-${{ matrix.target }}.zip wikitools.exe

# Artifact names (line 59):
name: wikitools-${{ matrix.target }}
path: wikitools-${{ matrix.target }}.*

# Dictionary job: binary name and cache (lines 74, 91-97):
- name: Build wikitools
  run: cargo build --release

- name: Restore cached dump
  uses: actions/cache@v4
  with:
    path: ~/.cache/wikitools
    key: wikitools-wikidata-${{ steps.dump-meta.outputs.date }}

# Generate pair dictionary (line 97):
run: ./target/release/wikitools pair en zh --download

# Generate titles dictionaries (new step after MDX conversion):
- name: Generate titles dictionaries
  run: |
    ./target/release/wikitools titles en --download
    ./target/release/wikitools titles zh --download\n
# Bundle titles dicts in the bundle step

# Upload artifacts (lines 122-126):
name: dictionary
path: |
  wikipedia-titlepair-en-zh-*.tar.gz
  wikipedia-titles-*.tar.gz

# Release files (lines 160-163):
files: |
  wikitools-*.tar.gz
  wikitools-*.zip
  wikipedia-titlepair-*.tar.gz
  wikipedia-titles-*.tar.gz
```

- [ ] **Step 2: Update update-dictionary.yml**

Replace all `wikititlepair` → `wikitools`, `wikidict` → `wikitools`:

1. Binary name (line 87, 114): `wikititlepair` → `wikitools`
2. Cache path (line 100): `~/.cache/wikidict` → `~/.cache/wikitools`
3. Cache key (line 101): `wikidata-dump-*` → `wikitools-wikidata-*`
4. Cache download step (lines 107-110): `~/.cache/wikidict` → `~/.cache/wikitools`
5. Generate step (line 114): `./target/release/wikitools pair en zh`
6. Upload path (line 121): unchanged (still `wikipedia-titlepair-en-zh-*.dsl*`)

Add titles dictionary generation:

```yaml
# After the pair dictionary generation step, add:
- name: Generate titles dictionaries
  if: steps.check.outputs.update == 'true'
  run: |
    ./target/release/wikitools titles en --download
    ./target/release/wikitools titles zh --download

- name: Upload titles artifacts
  if: steps.check.outputs.update == 'true'
  uses: actions/upload-artifact@v5
  with:
    name: titles-dictionaries
    path: wikipedia-titles-*.dsl*
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/
git commit -m "ci: rename to wikitools, add titles dictionaries"
```

---

### Task 8: Update README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite README**

Replace content with new wikitools documentation:

```markdown
# wikitools

CLI tools for building dictionaries from Wikimedia data.

## Features

- `wikitools pair` — Generate bidirectional DSL dictionaries from Wikipedia interlanguage links via Wikidata
- `wikitools titles` — Extract all article titles from a Wikimedia project and build a dictionary linking each title to its online page

## Usage

```bash
# Build a bidirectional EN↔ZH dictionary
wikitools pair en zh --download

# Extract all English Wikipedia titles as a URL dictionary
wikitools titles en --download

# Extract all Japanese Wiktionary titles  
wikitools titles ja --project wiktionary --download
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
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README for wikitools rename and titles feature"
```

---

### Task 9: Integration verification

- [ ] **Step 1: Full test suite**

```bash
cargo test
cargo build --release
```

Expected: all tests pass, release builds clean.

- [ ] **Step 2: CLI smoke test**

```bash
./target/release/wikitools --help
./target/release/wikitools pair --help
./target/release/wikitools titles --help
```

Expected: help output for each is correct, no panic.

- [ ] **Step 3: Pair pipeline with cached dump**

If Wikidata dump is cached:

```bash
./target/release/wikitools pair en zh
```

Expected: generates DSL with pair entries.

- [ ] **Step 4: Titles pipeline**

```bash
./target/release/wikitools titles en --download
```

Expected: downloads all-titles dump, generates DSL with URL entries. Check that output DSL has correct format (headword + `<a href="...">` definition).

- [ ] **Step 5: Commit if any fixups needed**

```bash
git add -A
git commit -m "chore: integration fixes"
```
