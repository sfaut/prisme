"""Extracts from a page of release notes of MariaDB (mariadb.com/docs, in its
Markdown form: the URL of the page followed by `.md`) the entries that matter
for the dialects of Prisme, as Markdown.

Usage: python -I mariadb-relnotes.py <page.md> <url> <output.md>

Kept: the title, the maturity and the date of the release, and the sections
about SQL (optimizer, syntax, functions, types, character sets, JSON, views,
catalog, server), without the sections about storage engines, replication,
Galera, backups, packaging, platforms, plugins, clients, protocol and
security, nor the entries about internal failures (assertions, crashes,
sanitizers, memory), which change no result of a query. A page of the changes
of a series (`what-is-mariadb-106`) is read the same way.
"""
import re
import sys

DROPPED_SECTIONS = re.compile(
    r'storage engine|\binnodb\b|\baria\b|\bmyisam\b|\brocksdb\b|\bmroonga\b|\bspider\b'
    r'|\bconnect\b|columnstore|\bs3\b|\bcsv\b|\barchive\b|federated|oqgraph|sphinx|tokudb'
    r'|\bmemory\b|galera|\bwsrep\b|replication|binlog|binary log|backup|packag|platform'
    r'|plugin|scripts|client|connector|protocol|security|authentication|privilege'
    r'|password|encryption|\btls\b|\bssl\b|openssl|query cache|performance schema'
    r'|^changelog|known issues|installation|upgrad|operating system|docker'
    r'|repositories|windows|linux|macos|debian|systemd|\bbuild|compil|^tests?$|\bmtr\b'
    r'|contributors|thanks|^download|locking|thread pool|^list of all',
    re.I,
)
DROPPED_WORDS = re.compile(
    r'assertion|\bassert\b|crash|server exit|debug build|valgrind|\basan\b|\bubsan\b'
    r'|\bmsan\b|sanitizer|memory leak|\bleak|memory corruption|out-of-bounds|\boob\b'
    r'|overrun|use after free|\bheap\b|uninitiali[sz]ed|\bhangs?\b|race condition'
    r'|compiler|\bcompil(e|ing|ation)\b|\bcmake\b|unit test|\bmtr\b|test suite|galera'
    r'|\bwsrep\b|\breplica|\bbinlog|\binnodb\b|\baria\b|\bmyisam\b|\brocksdb\b|\bspider\b'
    r'|\bplugin\b|mariadb-backup|mariabackup|mariadb-dump|deprecation policy|repositories for'
    r'|\bfails? on (arm|windows|linux|macos)|\bmain\.[a-z_]+ fails',
    re.I,
)
HEADING = re.compile(r'^(#{2,6})\s+(.*?)\s*$')
LINK = re.compile(r'\[([^\]]*)\]\([^)]*\)')


def clean(markdown):
    """Markdown of the page without the hints, buttons, tags, tables of
    vulnerabilities and notices of the documentation site."""
    markdown = re.sub(r'\{%.*?%\}', '', markdown)
    markdown = re.sub(r'<a [^>]*class="button[^"]*"[^>]*>.*?</a>', '', markdown)
    markdown = re.sub(r'</?(p|sub|sup|strong|div|figure|img|br)\b[^>]*>', '', markdown)
    markdown = re.sub(r'^> For the complete documentation index.*$', '', markdown, flags=re.M)
    markdown = re.sub(r'^\[\*?Alternate download.*$', '', markdown, flags=re.M)
    markdown = re.sub(r'^\|.*\|$', '', markdown, flags=re.M)
    markdown = re.sub(r'^\*This page is licensed.*$', '', markdown, flags=re.M)
    markdown = re.sub(r'\s*<a href="#[^"]*" id="[^"]*"></a>', '', markdown)
    markdown = re.sub(r'^(Be notified of new MariaDB|MariaDB may already be included).*$', '', markdown, flags=re.M)
    markdown = re.sub(r'^\*{3}$', '', markdown, flags=re.M)
    return markdown


def absolute(text, url):
    """Text whose links relative to the site point to it."""
    base = re.match(r'https://[^/]+', url).group(0)
    return re.sub(r'\]\((/[^)]*)\)', lambda m: f']({base}{m.group(1)})', text)


def entries(lines):
    """Entries of a section: each list item with its continuation lines, or
    each paragraph; a list item starts with `- `."""
    found, current = [], []
    for line in lines:
        starts = re.match(r'^[*-] ', line)
        if (starts or not line.strip()) and current:
            found.append(current)
            current = []
        if line.strip():
            current.append(re.sub(r'^\* ', '- ', line.rstrip()))
    if current:
        found.append(current)
    return ['\n'.join(entry) for entry in found]


def sections(markdown):
    """Sections of the page, after its title: (heading path, lines); the
    path holds the level and the heading of the section and of the sections
    it lies in, each read without its links."""
    found, path, lines = [], [], []
    for line in markdown.split('\n'):
        heading = HEADING.match(line)
        if heading:
            if lines:
                found.append((path, lines))
            level = len(heading.group(1))
            name = LINK.sub(r'\1', heading.group(2)).strip()
            path = [(outer, other) for outer, other in path if outer < level] + [(level, name)]
            lines = []
        else:
            lines.append(line)
    if lines:
        found.append((path, lines))
    return found


def main():
    page, url, output = sys.argv[1:4]
    markdown = clean(open(page, encoding='utf-8').read())
    # The sections start at the first heading under the title, of level 2
    # or 3 (the preview releases have no level 2).
    first = re.search(r'^#{2,6} ', markdown, re.M)
    head, body = (markdown[: first.start()], markdown[first.start() :]) if first else (markdown, '')
    title = re.search(r'^# (.*)$', head, re.M)
    out = [f'# {title.group(1).strip() if title else url}', '', f'Source: {url}', '']
    lead = re.search(r'^MariaDB \d+\.\d+\.\d+ is an? .*$', head, re.M)
    released = re.search(r'^\*\*Release date:\*\*\s*(.*)$', head, re.M)
    if lead:
        out += [LINK.sub(r'\1', lead.group(0)).replace('*', '').strip(), '']
    elif released:
        out += [f'Release date: {released.group(1).strip()}', '']
    # The headings keep their structure, the first level of the page written
    # at level 2; a heading is written once, before the first entry kept
    # under it.
    top = len(first.group(0)) - 1 if first else 2
    written, count = set(), 0
    for path, lines in sections(body):
        if any(DROPPED_SECTIONS.search(name) for _, name in path):
            continue
        found = [absolute(entry, url) for entry in entries(lines) if not DROPPED_WORDS.search(entry)]
        if not found:
            continue
        for depth in range(len(path)):
            if tuple(path[: depth + 1]) not in written:
                written.add(tuple(path[: depth + 1]))
                level, name = path[depth]
                out += ['#' * (level - top + 2) + ' ' + name, '']
        for entry in found:
            out += [entry, '' if not entry.startswith('- ') else None]
        out.append('')
        count += len(found)
    if not count:
        out += ['No entry about SQL.', '']
    text = '\n'.join(line for line in out if line is not None)
    text = re.sub(r'\n{3,}', '\n\n', text).strip() + '\n'
    open(output, 'w', encoding='utf-8', newline='\n').write(text)
    print(f'{output}: {count} entries')


main()
