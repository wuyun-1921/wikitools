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
