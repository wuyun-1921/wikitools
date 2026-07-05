use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rayon::prelude::*;

mod download;
mod dsl;
mod error;
mod escape;
mod mdx;
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

        /// Include ALL titles from both languages, not just matched pairs.
        /// Unmatched titles get an empty body.
        #[arg(long)]
        full: bool,
    },

    /// Extract all article titles and generate a URL dictionary
    Titles {
        /// Language code (e.g., en, zh, ja)
        lang: String,

        /// Wikimedia project (default: wikipedia)
        #[arg(long, default_value = "wikipedia")]
        project: String,

        /// Output format: mdx (default) or dsl
        #[arg(long, default_value = "mdx")]
        format: String,

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
    full: bool,
) -> Result<()> {
    let cache_dir = resolve_cache_dir(cache_dir);
    let dump_path = ensure_wikidata_dump(&cache_dir, download)?;
    let dump_date = get_dump_date(&wikidata_listing_url(), "wb_items_per_site.sql.gz")
        .unwrap_or_else(|_| "latest".to_string());

    let output = output.unwrap_or_else(|| {
        PathBuf::from(format!("wikipedia-titlepair-{}-{}-{}.dsl", lang_a, lang_b, dump_date))
    });

    eprintln!("\nParsing dump...");
    let entries = parse_dump(&dump_path, lang_a, lang_b, full)?;
    let entry_count = entries.len();

    eprintln!("  Escaping {} entries to DSL...", entry_count);
    let escaped: Vec<(String, String)> = entries
        .into_par_iter()
        .map(|(a, b)| {
            let head = escape_dsl(&a);
            if b.is_empty() {
                (head, String::new())
            } else {
                (head, format!("<<{}>>", escape_dsl(&b)))
            }
        })
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
    format: &str,
    output: Option<PathBuf>,
    cache_dir: &PathBuf,
    download: bool,
) -> Result<()> {
    let cache_dir = resolve_cache_dir(cache_dir);
    let dump_path = titles::ensure_titles_dump(&cache_dir, lang, project, download)?;
    let listing_url = titles::titles_listing_url(lang, project);
    let dump_date = get_dump_date(&listing_url, "all-titles-in-ns0.gz")
        .unwrap_or_else(|_| "latest".to_string());

    eprintln!("\nParsing titles dump...");
    let mut entries = titles::parse_all_titles(&dump_path, lang, project)?;
    let entry_count = entries.len();

    if format == "dsl" {
        // DSL: alphabetically sorted, escaped headwords, URL as body
        entries.par_sort();
        entries.dedup();
        let escaped: Vec<(String, String)> = entries
            .into_par_iter()
            .map(|(display, path)| {
                let head = escape_dsl(&display);
                let url = format!("https://{}.{}.org{}", lang, project, path);
                (head, url)
            })
            .collect();

        let output = output.unwrap_or_else(|| {
            PathBuf::from(format!("wikipedia-titles-{}-{}.dsl", lang, dump_date))
        });

        let label = format!("wikipedia titles ({})", lang);
        write_dsl(&output, &label, lang, lang, &escaped)?;
        eprintln!("\nDone! {} entries written to {}", entry_count, output.display());
        compress_dictzip(&output);
    } else {
        // MDX: MDict sort, HTML bodies with onclick+JS
        entries.sort_by(|a, b| {
            let al = a.0.to_lowercase();
            let bl = b.0.to_lowercase();
            let as_: String = al.chars().filter(|c| !c.is_ascii_punctuation() && *c != ' ').collect();
            let bs: String = bl.chars().filter(|c| !c.is_ascii_punctuation() && *c != ' ').collect();
            match as_.cmp(&bs) {
                std::cmp::Ordering::Equal => match as_.len().cmp(&bs.len()) {
                    std::cmp::Ordering::Equal => {
                        let at = al.trim_end_matches(|c: char| c.is_ascii_punctuation());
                        let bt = bl.trim_end_matches(|c: char| c.is_ascii_punctuation());
                        bt.cmp(at)
                    }
                    other => other,
                },
                other => other,
            }
        });

        let js_name = format!("wikipedia-titles-{}.js", lang);
        for (i, (display, path)) in entries.iter_mut().enumerate() {
            let span = format!(
                "<span class=\"wl\" data-p=\"{0}\" onclick=\"go(this)\">{1}</span>",
                path, display
            );
            *path = if i == 0 {
                format!("<head><script src=\"{0}\"></script></head>{1}", js_name, span)
            } else {
                span
            };
        }

        let mdx_path = output.unwrap_or_else(|| {
            PathBuf::from(format!("wikipedia-titles-{}-{}.mdx", lang, dump_date))
        });

        eprintln!("  Packing {} entries...", entry_count);
        let proj_display = if project == "wiki" { "Wikipedia" } else { project };
        let title_str = format!("{} {} Titles", lang.to_uppercase(), proj_display);
        let desc_str = format!("{} article titles from {} {}", lang.to_uppercase(), proj_display, dump_date);
        mdx::write_mdx(&mdx_path, &title_str, &desc_str, &entries)?;

        let js_path = mdx_path.with_file_name(format!("wikipedia-titles-{}.js", lang));
        std::fs::write(&js_path, format!(
            "function go(el){{window.open('https://{}.{}.org'+el.getAttribute('data-p'),'_blank')}}",
            lang, project
        ))?;

        let size_mb = std::fs::metadata(&mdx_path)?.len() as f64 / 1e6;
        eprintln!("\nDone! {} entries → {} ({:.1} MB)", entry_count, mdx_path.display(), size_mb);
    }
    Ok(())
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Pair { lang_a, lang_b, output, cache_dir, download, full } => {
            run_pair(&lang_a, &lang_b, output, &cache_dir, download, full)?;
        }
        Command::Titles { lang, project, format, output, cache_dir, download } => {
            run_titles(&lang, &project, &format, output, &cache_dir, download)?;
        }
    }

    Ok(())
}
