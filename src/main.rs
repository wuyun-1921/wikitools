use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rayon::prelude::*;

mod download;
mod dsl;
mod error;
mod escape;
mod pair;
mod titles;

use download::{ensure_wikidata_dump, get_dump_date, wikidata_listing_url};
use dsl::{compress_dictzip, write_dsl};
use error::Result;
use escape::escape_dsl;
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
