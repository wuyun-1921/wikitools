use std::path::{Path, PathBuf};

use rayon::prelude::*;

use clap::Parser;

mod download;
mod dsl;
mod error;
mod escape;
mod pair;

use download::{ensure_wikidata_dump, get_dump_date, wikidata_listing_url};
use error::Result;
use dsl::{write_dsl, compress_dictzip};
use escape::escape_dsl;
use pair::parse_dump;

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

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let cache_dir = if cli.cache_dir.to_str() == Some("~/.cache/wikidict") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache/wikidict")
    } else {
        cli.cache_dir
    };

    let dump_path = ensure_wikidata_dump(&cache_dir, cli.download)?;
    
    // Get dump date from Wikidata
    let dump_date = get_dump_date(&wikidata_listing_url(), "wb_items_per_site.sql.gz")
        .unwrap_or_else(|_| "latest".to_string());
    
    // Default output filename: wikipedia-titlepair-en-zh-20250702.dsl
    let output = cli.output.unwrap_or_else(|| {
        PathBuf::from(format!("wikipedia-titlepair-{}-{}-{}.dsl", cli.lang_a, cli.lang_b, dump_date))
    });

    eprintln!("\nParsing dump...");
    let entries = parse_dump(&dump_path, &cli.lang_a, &cli.lang_b)?;
    let entry_count = entries.len();

    // Pre-compute escaped DSL strings in parallel
    eprintln!("  Escaping {} entries to DSL...", entry_count);
    // Body is pre-formatted with << >> for DSL cross-reference syntax
    let escaped: Vec<(String, String)> = entries
        .into_par_iter()
        .map(|(a, b)| (escape_dsl(&a), format!("<<{}>>", escape_dsl(&b))))
        .collect();

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

    Ok(())
}

