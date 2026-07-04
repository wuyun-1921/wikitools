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

/// Parse Wikidata wb_items_per_site dump into bidirectional title pairs.
pub fn parse_dump(path: &Path, lang_a: &str, lang_b: &str) -> Result<Vec<(String, String)>> {
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
                let end = insert_starts
                    .iter()
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
    eprintln!(
        "  Found {} matching items ({} entries, {} skipped)",
        matched,
        entries.len(),
        skipped
    );

    entries.par_sort();
    entries.dedup();
    Ok(entries)
}

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
