use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use rayon::prelude::*;

use clap::Parser;
use flate2::read::GzDecoder;
use thiserror::Error;

#[derive(Parser)]
#[command(name = "wiktitlepair")]
#[command(about = "Generate bidirectional dictionary from Wikipedia interlanguage links")]
struct Cli {
    /// First language code (e.g., en, zh, ja)
    lang_a: String,

    /// Second language code (e.g., zh, ja, ko)
    lang_b: String,

    /// Output file path
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Cache directory for dumps
    #[arg(long, default_value = "~/.cache/wikidict")]
    cache_dir: PathBuf,

    /// Allow downloading if dump not found
    #[arg(long)]
    download: bool,
}

#[derive(Error, Debug)]
enum WikiDictError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),
}

type Result<T> = std::result::Result<T, WikiDictError>;

fn dump_url() -> String {
    "https://dumps.wikimedia.org/wikidatawiki/latest/wikidatawiki-latest-wb_items_per_site.sql.gz".to_string()
}

fn get_dump_date() -> Result<String> {
    let output = Command::new("curl")
        .args(["-s", "https://dumps.wikimedia.org/wikidatawiki/latest/"])
        .output()?;
    
    let html = String::from_utf8_lossy(&output.stdout);
    for line in html.lines() {
        if line.contains("wb_items_per_site.sql.gz") {
            // Manual scan for DD-Mon-YYYY pattern: 2 digits, -, uppercase, 2 lowercase, -, 4 digits
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
    
    Err(WikiDictError::Parse("Could not parse dump date".to_string()))
}

fn download_file(url: &str, path: &Path) -> Result<()> {
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
            "-C", "-", "-L", "-o", path.to_str().unwrap(), "-A", "wikidict/0.1",
            "-sS", "--fail", url,
        ])
        .status()?;

    if !status.success() {
        return Err(WikiDictError::Parse(format!(
            "curl failed for {}",
            filename
        )));
    }

    Ok(())
}

fn ensure_dump(cache_dir: &Path, allow_download: bool) -> Result<PathBuf> {
    let path = cache_dir.join("wikidatawiki-latest-wb_items_per_site.sql.gz");

    if path.exists() && std::fs::metadata(&path)?.len() > 1000 {
        eprintln!("Using cached dump");
        return Ok(path);
    }

    if !allow_download {
        return Err(WikiDictError::Parse(
            "Dump not found. Use --download to fetch it.".to_string()
        ));
    }

    std::fs::create_dir_all(cache_dir)?;

    eprintln!("Downloading Wikidata items_per_site dump...");
    let url = dump_url();
    eprintln!("  {}", url);
    download_file(&url, &path)?;
    eprintln!("  done");

    Ok(path)
}

/// Parse one INSERT statement, extracting (item_id, site_id, title) tuples
/// for rows matching our target languages.
fn parse_insert_line(line: &str, site_a: &str, site_b: &str, items_a: &mut HashMap<u32, String>, items_b: &mut HashMap<u32, String>) {
    // Find VALUES keyword
    let values_pos = match line.find("VALUES") {
        Some(p) => p + 6,
        None => return,
    };
    
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut pos = values_pos;
    
    // Skip whitespace
    while pos < len && bytes[pos] == b' ' {
        pos += 1;
    }
    
    // Parse rows: (row_id,item_id,'site_id','title'),...
    while pos < len {
        // Expect '('
        if pos >= len || bytes[pos] != b'(' {
            break;
        }
        pos += 1;
        
        // Parse fields
        let mut fields: Vec<&str> = Vec::new();
        let _field_start = pos;
        
        // Simple field parsing - find commas and closing paren at depth 0
        let mut in_quote = false;
        let mut depth = 0;
        let mut field_buf_start = pos;
        
        while pos < len {
            let b = bytes[pos];
            
            if in_quote {
                if b == b'\\' && pos + 1 < len {
                    pos += 2; // skip escaped char
                    continue;
                } else if b == b'\'' {
                    // Check for doubled quote
                    if pos + 1 < len && bytes[pos + 1] == b'\'' {
                        pos += 2;
                        continue;
                    }
                    in_quote = false;
                }
                pos += 1;
            } else {
                if b == b'\'' {
                    in_quote = true;
                    pos += 1;
                } else if b == b'(' {
                    depth += 1;
                    pos += 1;
                } else if b == b')' {
                    if depth == 0 {
                        // End of this row
                        let field = &line[field_buf_start..pos];
                        fields.push(field.trim());
                        pos += 1;
                        break;
                    }
                    depth -= 1;
                    pos += 1;
                } else if b == b',' && depth == 0 {
                    let field = &line[field_buf_start..pos];
                    fields.push(field.trim());
                    field_buf_start = pos + 1;
                    pos += 1;
                } else {
                    pos += 1;
                }
            }
        }
        
        // Skip comma between rows
        if pos < len && bytes[pos] == b',' {
            pos += 1;
        }
        
        // Need at least 4 fields: row_id, item_id, site_id, title
        if fields.len() < 4 {
            continue;
        }
        
        // Parse item_id (field 1)
        let item_id: u32 = match fields[1].parse() {
            Ok(id) => id,
            Err(_) => continue,
        };
        
        // Parse site_id (field 2, unquoted)
        let site_id = unquote(fields[2]);
        
        // Parse title (field 3, unquoted)
        let title = unquote(fields[3]).replace('_', " ");
        
        // Skip non-article namespace pages
        if is_non_article(&title) {
            continue;
        }
        
        if site_id == site_a {
            items_a.insert(item_id, title);
        } else if site_id == site_b {
            items_b.insert(item_id, title);
        }
    }
}

fn parse_dump(path: &Path, lang_a: &str, lang_b: &str) -> Result<Vec<(String, String)>> {
    // Read entire file into memory for multi-threaded processing
    let file = File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut contents = Vec::new();
    decoder.read_to_end(&mut contents)?;
    
    // Find all INSERT statement boundaries
    let site_a = format!("{}wiki", lang_a);
    let site_b = format!("{}wiki", lang_b);
    
    eprintln!("  Finding INSERT statements...");
    let mut insert_starts = Vec::new();
    let mut pos = 0;
    while pos < contents.len() {
        if contents[pos..].starts_with(b"INSERT INTO") {
            insert_starts.push(pos);
        }
        pos += 1;
    }
    eprintln!("  Found {} INSERT statements", insert_starts.len());
    
    // Process in parallel chunks
    let chunk_size = (insert_starts.len() / rayon::current_num_threads()).max(1);
    let chunks: Vec<_> = insert_starts.chunks(chunk_size).collect();
    
    eprintln!("  Processing with {} threads...", rayon::current_num_threads());
    
    let results: Vec<_> = chunks
        .par_iter()
        .enumerate()
        .map(|(chunk_idx, chunk)| {
            let mut items_a = HashMap::new();
            let mut items_b = HashMap::new();
            
            for &start in *chunk {
                // Find end of this INSERT statement (next INSERT or end of file)
                let end = insert_starts.iter()
                    .find(|&&s| s > start)
                    .copied()
                    .unwrap_or(contents.len());
                
                let stmt = &contents[start..end];
                if let Ok(line) = std::str::from_utf8(stmt) {
                    parse_insert_line(line, &site_a, &site_b, &mut items_a, &mut items_b);
                }
            }
            
            if chunk_idx % 10 == 0 {
                eprint!("\r  Chunk {}/{} done", chunk_idx + 1, chunks.len());
            }
            
            (items_a, items_b)
        })
        .collect();
    
    eprintln!("\n  Merging results...");
    
    // Parallel merge results using rayon reduce
    let (items_a, items_b) = results
        .into_par_iter()
        .reduce(
            || (HashMap::new(), HashMap::new()),
            |(mut a1, mut b1), (a2, b2)| {
                a1.extend(a2);
                b1.extend(b2);
                (a1, b1)
            },
        );

    // Build dictionary: items that exist in both languages
    let mut entries = Vec::new();
    let mut skipped = 0u64;
    for (item_id, title_a) in &items_a {
        if let Some(title_b) = items_b.get(item_id) {
            // Skip if titles are identical (case-insensitive)
            if title_a.to_lowercase() == title_b.to_lowercase() {
                skipped += 1;
                continue;
            }
            entries.push((title_a.clone(), title_b.clone()));
            entries.push((title_b.clone(), title_a.clone()));
        }
    }
    
    let matched = entries.len() / 2;
    eprintln!("  Found {} matching items ({} entries, {} skipped)", matched, entries.len(), skipped);
    
    entries.par_sort();
    entries.dedup();
    Ok(entries)
}

/// MediaWiki namespace canonical names (English). The Wikidata dump always
/// uses English canonical names regardless of the wiki's language.
/// See https://www.mediawiki.org/wiki/Manual:Namespace
static NON_ARTICLE_PREFIXES: &[&str] = &[
    "Category",
    "Template",
    "Wikipedia",
    "Portal",
    "Help",
    "Module",
    "WikiProject",
    "User",
    "File",
    "Image",
    "MediaWiki",
    "TimedText",
    "Draft",
    "Media",
    "Special",
    "Talk",
    "WP",
];

/// Returns true if the title is a non-article namespace page
/// (e.g. "Category:Music", "Template:Infobox").
fn is_non_article(title: &str) -> bool {
    for prefix in NON_ARTICLE_PREFIXES {
        let needle = [prefix, ":"].concat();
        if title.starts_with(&needle) {
            return true;
        }
    }
    false
}

/// Escape special characters in DSL headwords and cross-references.
/// DSL uses these characters for markup, so we must backslash-escape them
/// to make them literal text. Backslash itself must be doubled.
/// See docs/dsl-format.md for the full reference.
fn escape_dsl(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        match ch {
            // Backslash must come first - double it
            '\\' => result.push_str("\\\\"),
            // Parentheses mark optional parts of headword
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            // Curly braces mark unsorted parts of headword
            '{' => result.push_str("\\{"),
            '}' => result.push_str("\\}"),
            // Square brackets forbidden in heading
            '[' => result.push_str("\\["),
            ']' => result.push_str("\\]"),
            // Forbidden in heading
            '#' => result.push_str("\\#"),
            '@' => result.push_str("\\@"),
            '<' => result.push_str("\\<"),
            '>' => result.push_str("\\>"),
            // Forbidden in first heading
            '~' => result.push_str("\\~"),
            '^' => result.push_str("\\^"),
            _ => result.push(ch),
        }
    }
    result
}

fn unquote(s: &str) -> String {
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

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let cache_dir = if cli.cache_dir.to_str() == Some("~/.cache/wikidict") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache/wikidict")
    } else {
        cli.cache_dir
    };

    let dump_path = ensure_dump(&cache_dir, cli.download)?;
    
    // Get dump date from Wikidata
    let dump_date = get_dump_date().unwrap_or_else(|_| "latest".to_string());
    
    // Default output filename: wikipedia-titlepair-en-zh-20250702.dsl
    let output = cli.output.unwrap_or_else(|| {
        PathBuf::from(format!("wikipedia-titlepair-{}-{}-{}.dsl", cli.lang_a, cli.lang_b, dump_date))
    });

    eprintln!("\nParsing dump...");
    let entries = parse_dump(&dump_path, &cli.lang_a, &cli.lang_b)?;
    let entry_count = entries.len();

    // Pre-compute escaped DSL strings in parallel
    eprintln!("  Escaping {} entries to DSL...", entry_count);
    let escaped: Vec<(String, String)> = entries
        .into_par_iter()
        .map(|(a, b)| (escape_dsl(&a), escape_dsl(&b)))
        .collect();

    let mut file = BufWriter::new(File::create(&output)?);
    // Sort lang codes for consistent metadata (en zh == zh en)
    let mut meta_langs = [cli.lang_a.as_str(), cli.lang_b.as_str()];
    meta_langs.sort();
    let lang_pair = format!("{}-{}", meta_langs[0], meta_langs[1]);
    // DSL format for ABBYY Lingvo
    writeln!(file, "#NAME \"wikipedia titlepair ({})\"", lang_pair)?;
    writeln!(file, "#INDEX_LANGUAGE \"{}\"",lang_pair)?;
    writeln!(file, "#CONTENTS_LANGUAGE \"{}\"", lang_pair)?;
    writeln!(file)?;
    
    for (a, b) in &escaped {
        write!(file, "{}\n\t<<{}>>\n", a, b)?;
    }
    file.flush()?;

    eprintln!(
        "\nDone! {} entries written to {}",
        entry_count,
        output.display()
    );

    // Compress with dictzip
    let dz_output = output.with_extension("dsl.dz");
    eprintln!("Compressing with dictzip...");
    let status = Command::new("dictzip")
        .arg(output.to_str().unwrap())
        .status()?;
    if status.success() {
        eprintln!("  {} created", dz_output.display());
    } else {
        eprintln!("  dictzip failed (dsl file kept)");
    }

    Ok(())
}

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

    #[test]
    fn test_parse_insert_line() {
        // Tiny synthetic SQL: one enwiki item, one zhwiki item, same item_id
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
        // Only article (item 1): kept
        assert_eq!(items_a.get(&1).unwrap(), "Music");
        assert_eq!(items_b.get(&1).unwrap(), "音乐");
        // Category (item 2): skipped
        assert!(!items_a.contains_key(&2));
        assert!(!items_b.contains_key(&2));
    }

    #[test]
    fn test_full_pipeline_tiny_dump() {
        // Build a minimal gzipped SQL dump with 2 matched items
        let sql = "INSERT INTO wb_items_per_site VALUES (1,1,'enwiki','Music'),(2,1,'zhwiki','音乐');\nINSERT INTO wb_items_per_site VALUES (3,2,'enwiki','Hello'),(4,2,'zhwiki','你好');\nINSERT INTO wb_items_per_site VALUES (5,3,'enwiki','Same'),(6,3,'zhwiki','Same');\n";
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(sql.as_bytes()).unwrap();
        let compressed = gz.finish().unwrap();

        // Write to temp file
        let tmp = std::env::temp_dir().join("wikidict_test_dump.sql.gz");
        std::fs::write(&tmp, &compressed).unwrap();

        let entries = parse_dump(&tmp, "en", "zh").unwrap();
        std::fs::remove_file(&tmp).ok();

        // 2 matched pairs × 2 directions = 4 entries (Same skipped: identical titles)
        assert_eq!(entries.len(), 4);
        assert!(entries.contains(&("Music".to_string(), "音乐".to_string())));
        assert!(entries.contains(&("音乐".to_string(), "Music".to_string())));
        assert!(entries.contains(&("Hello".to_string(), "你好".to_string())));
        assert!(entries.contains(&("你好".to_string(), "Hello".to_string())));
        // Same→Same should be skipped (identical case-insensitive)
        assert!(!entries.contains(&("Same".to_string(), "Same".to_string())));
    }

    #[test]
    fn test_dsl_output_format() {
        // Test that DSL metadata is correct
        let entries = vec![
            ("Music".to_string(), "音乐".to_string()),
            ("音乐".to_string(), "Music".to_string()),
        ];
        let escaped: Vec<_> = entries
            .into_iter()
            .map(|(a, b)| (escape_dsl(&a), escape_dsl(&b)))
            .collect();

        let tmp = std::env::temp_dir().join("wikidict_test_output.dsl");
        {
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
        assert!(content.contains("#INDEX_LANGUAGE \"en-zh\""));
        assert!(content.contains("Music\n\t<<音乐>>"));
        assert!(content.contains("音乐\n\t<<Music>>"));
    }
}
