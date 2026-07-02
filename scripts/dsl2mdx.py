#!/usr/bin/env python3
"""Convert wikititlepair DSL output to MDict MDX format.

Parses DSL directly, merges duplicate headwords, converts cross-references
to HTML links, and packs with mdict-utils. No intermediate database.

Fixes applied during conversion:
  1. Duplicate headwords merged with <br> separators
  2. DSL backslash escapes reversed for clean HTML display
  3. Headword prepended in bold
  4. <<cross-ref>> → <a href="entry://...">

Dependencies: mdict-utils  (pip install mdict-utils)

Usage:
  python scripts/dsl2mdx.py dict.dsl
  python scripts/dsl2mdx.py dict.dsl -t "My Dictionary" -o output.mdx
"""

import argparse
import gzip
import re
import shutil
import subprocess
import sys
import urllib.parse
from collections import defaultdict
from pathlib import Path
from typing import Dict, List

# Regex for DSL cross-references: <<anything without > inside>>
_CROSS_REF_RE = re.compile(r'<<([^>]*)>>')

# Mapping from DSL backslash escapes to literal characters.
# Keep in sync with escape_dsl() in src/main.rs.
_DSL_UNESCAPE = [
    ('\\\\', '\\'),  # \\ → \ (must be first, non-raw for trailing backslash)
    (r'\(', '('),
    (r'\)', ')'),
    (r'\{', '{'),
    (r'\}', '}'),
    (r'\[', '['),
    (r'\]', ']'),
    (r'\#', '#'),
    (r'\@', '@'),
    (r'\<', '<'),
    (r'\>', '>'),
    (r'\~', '~'),
    (r'\^', '^'),
]

_HTML_ESCAPE = [
    ('&', '&amp;'),
    ('<', '&lt;'),
    ('>', '&gt;'),
    ('"', '&quot;'),
]


def _check_deps() -> None:
    """Verify mdict-utils is installed."""
    if not shutil.which('mdict'):
        sys.exit(
            "Error: 'mdict' not found. Install with: pip install mdict-utils"
        )


def parse_dsl(path: Path) -> Dict[str, List[str]]:
    """Parse wikititlepair DSL file.

    Returns {headword: [body_line, ...]}. Duplicate headwords
    (same word, different translations) are accumulated in lists.
    """
    print(f"[1/3] Parsing DSL ({path.name})")

    if path.suffix == '.dz':
        fh = gzip.open(path, 'rt', encoding='utf-8')
    else:
        fh = open(path, 'r', encoding='utf-8')

    entries: Dict[str, List[str]] = defaultdict(list)
    headword = None
    count = 0

    with fh:
        for line in fh:
            line = line.rstrip('\n\r')
            if not line:
                continue

            # DSL header lines start with #, body lines with tab/space
            if line[0] in ('#', '\t', ' '):
                if headword is not None and line[0] in ('\t', ' '):
                    body = line.strip()
                    if body:
                        entries[headword].append(body)
            else:
                headword = line
                count += 1
                if count % 1_000_000 == 0:
                    print(f"  Parsed {count:,} entries...")

    merged = count - len(entries)
    print(
        f"  Parsed {count:,} DSL entries, {len(entries):,} unique headwords "
        f"({merged:,} merged)"
    )
    return entries


def _unescape_dsl(text: str) -> str:
    """Reverse DSL backslash escapes. Does not HTML-escape."""
    for pattern, replacement in _DSL_UNESCAPE:
        text = text.replace(pattern, replacement)
    return text


def _html_escape(text: str) -> str:
    """HTML-escape for safe display in MDX body."""
    for pattern, replacement in _HTML_ESCAPE:
        text = text.replace(pattern, replacement)
    return text


def _encode_link_target(target: str) -> str:
    """Return link target, URL-encoding non-ASCII characters."""
    if any(ord(c) > 127 for c in target):
        return urllib.parse.quote(target, safe='')
    return target


def _convert_cross_ref(match: re.Match) -> str:
    """Convert <<word>> to <a href="entry://word">word</a>."""
    raw = match.group(1)
    clean = _unescape_dsl(raw)
    return (
        f'<a href="entry://{_encode_link_target(clean)}">'
        f'{_html_escape(clean)}</a>'
    )


def write_mdx_source(
    entries: Dict[str, List[str]],
    txt_path: Path,
) -> int:
    """Write MDX source file from parsed entries. Returns entry count."""
    print(f"  Writing {txt_path.name}...")

    written = 0
    with open(txt_path, 'w', encoding='utf-8') as out:
        for word in sorted(entries, key=str.lower):
            bodies = entries[word]

            # Unescape + HTML-escape the headword
            word_clean = _unescape_dsl(word)
            word_html = _html_escape(word_clean)

            # Merge bodies with <br>, convert cross-refs
            combined = "<br>".join(bodies)
            combined = _CROSS_REF_RE.sub(_convert_cross_ref, combined)

            definition = f'<b>{word_html}</b><br>{combined}'

            # MDX source format: key\nbody\n</>\n
            out.write(f'{word_clean}\n{definition}\n</>\n')
            written += 1

    print(f"  Wrote {written:,} entries")
    return written


def pack_mdx(
    txt_path: Path,
    mdx_path: Path,
    title: str,
    description: str,
) -> None:
    """Pack MDX source file into .mdx via mdict-utils."""
    print(f"[2/3] Packing MDX ({mdx_path.name})")

    title_path = txt_path.parent / '_mdx_title.html'
    desc_path = txt_path.parent / '_mdx_desc.html'
    title_path.write_text(title, encoding='utf-8')
    desc_path.write_text(description, encoding='utf-8')

    try:
        subprocess.run(
            [
                'mdict',
                '--title', str(title_path),
                '--description', str(desc_path),
                '-a', str(txt_path),
                str(mdx_path),
            ],
            check=True,
        )
    finally:
        title_path.unlink(missing_ok=True)
        desc_path.unlink(missing_ok=True)

    size_mb = mdx_path.stat().st_size / 1e6
    print(f"  → {mdx_path} ({size_mb:.1f} MB)")


def main() -> None:
    _check_deps()

    parser = argparse.ArgumentParser(
        description='Convert wikititlepair DSL to MDict MDX format',
    )
    parser.add_argument(
        'dsl', type=Path, help='Input .dsl or .dsl.dz file',
    )
    parser.add_argument(
        '-o', '--output', type=Path,
        help='Output .mdx path (default: same name as input)',
    )
    parser.add_argument(
        '-t', '--title', default='Wikipedia Title Pairs',
        help='Dictionary title shown in reader',
    )
    parser.add_argument(
        '-d', '--description', default='Wikipedia title pairs from Wikidata.',
        help='Dictionary description',
    )
    parser.add_argument(
        '--keep-txt', action='store_true',
        help='Keep intermediate MDX source text file',
    )
    args = parser.parse_args()

    dsl_path = args.dsl.resolve()
    if not dsl_path.exists():
        sys.exit(f"Error: {dsl_path} not found")

    mdx_path = args.output or dsl_path.with_suffix('.mdx')
    txt_path = mdx_path.with_suffix('.txt')

    print(f"DSL → MDX: {dsl_path.name} → {mdx_path.name}\n")

    entries = parse_dsl(dsl_path)
    write_mdx_source(entries, txt_path)
    pack_mdx(txt_path, mdx_path, args.title, args.description)

    if not args.keep_txt:
        txt_path.unlink()
        print(f"[3/3] Cleaned up {txt_path.name}")

    print(f"\nDone. {mdx_path}")


if __name__ == '__main__':
    main()
