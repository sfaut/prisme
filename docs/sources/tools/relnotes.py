"""Extracts from a page of release notes of MySQL (dev.mysql.com) the entries
that matter for the dialects of Prisme, as Markdown.

Usage: python -I relnotes.py <page.html> <url> <output.md>

Kept: the sections about SQL (syntax, optimizer, functions, types, character
sets, JSON, deprecations, functionality, bugs fixed), without the entries about
storage engines, replication, packaging, plugins and internal failures
(assertions, crashes, sanitizers), which change no result of a query.
"""
import html
import re
import sys

KEPT_SECTIONS = {
    'Character Set Support', 'Data Type Notes', 'Deprecation and Removal Notes',
    'Functionality Added or Changed', 'Bugs Fixed', 'JSON Notes', 'Optimizer Notes',
    'SQL Syntax Notes', 'SQL Function and Operator Notes', 'Spatial Data Support',
    'Important Changes', 'Error Handling',
}
DROPPED_PREFIXES = (
    'InnoDB', 'Replication', 'Group Replication', 'NDB', 'Packaging', 'Binary packages',
    'Windows', 'macOS', 'Solaris', 'Linux', 'Performance Schema', 'Thread Pool', 'X Plugin',
    'MySQL Enterprise', 'Audit', 'Keyring', 'Compilation', 'Microsoft Windows', 'Enterprise',
    'Docker', 'Router', 'MySQL Router', 'Pluggable Authentication', 'Clone', 'Backup',
)
DROPPED_WORDS = re.compile(
    r'assertion|assert\b|crash|server exited|debug build|valgrind|asan|ubsan|sanitizer'
    r'|memory leak|compiler|compil(e|ing|ation)|cmake|unit test|mtr\b|test suite'
    r'|keyring|audit_log|\bplugin\b',
    re.I,
)


def strip_tags(fragment):
    """Text of an HTML fragment without its tags."""
    return html.unescape(re.sub(r'<[^>]+>', '', fragment))


def text(fragment):
    """Text of an HTML fragment as Markdown: its code between backticks, its
    examples in blocks."""
    blocks = []

    def block(match):
        blocks.append(strip_tags(match.group(1)).strip())
        return f'\x00{len(blocks) - 1}\x00'

    fragment = re.sub(r'<pre[^>]*>(.*?)</pre>', block, fragment, flags=re.S)
    fragment = re.sub(r'<code[^>]*>(.*?)</code>', lambda m: f'`{m.group(1)}`', fragment, flags=re.S)
    prose = ' '.join(strip_tags(fragment).split())
    return re.sub(
        r' ?\x00(\d+)\x00 ?',
        lambda m: f'\n\n  ```sql\n  {blocks[int(m.group(1))].replace(chr(10), chr(10) + "  ")}\n  ```\n\n  ',
        prose,
    ).strip()


def entries(section_html):
    """Top-level list items of a section."""
    items, depth, start = [], 0, None
    for m in re.finditer(r'<li\b[^>]*>|</li>', section_html):
        if m.group(0).startswith('<li'):
            if depth == 0:
                start = m.end()
            depth += 1
        else:
            depth -= 1
            if depth == 0 and start is not None:
                items.append(section_html[start:m.start()])
    return items


def main(page, url, output):
    s = open(page, encoding='utf-8').read()
    title = re.search(r'<h2 class="title">(.*?)</h2>', s, re.S)
    heading = ' '.join(strip_tags(title.group(1)).split()) if title else url
    lines = [f'# {heading.replace("Changes in ", "")}', '', f'Source: {url}', '']
    kept_total = dropped_total = 0
    for part in re.split(r'<h3 class="title">', s)[1:]:
        name, body = part.split('</h3>', 1)
        name = ' '.join(strip_tags(name).split())
        if name not in KEPT_SECTIONS:
            continue
        kept = []
        for item in entries(body):
            entry = text(item)
            if entry.startswith(DROPPED_PREFIXES) or DROPPED_WORDS.search(entry):
                dropped_total += 1
                continue
            kept.append(entry)
        if kept:
            lines += [f'## {name}', '']
            lines += [f'- {entry}' for entry in kept]
            lines.append('')
            kept_total += len(kept)
    lines.append(f'<!-- {kept_total} entries kept, {dropped_total} dropped in the kept sections -->')
    open(output, 'w', encoding='utf-8', newline='\n').write('\n'.join(lines) + '\n')
    print(output, kept_total, 'kept,', dropped_total, 'dropped')


if __name__ == '__main__':
    main(*sys.argv[1:4])
