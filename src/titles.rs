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
