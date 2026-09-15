"""Source-only pre-publication check. Does not inspect user configs or print matching values."""
from pathlib import Path
import re, sys
root = Path(__file__).resolve().parent.parent
skip = {'node_modules', 'target', 'dist', '.git', 'gen', 'private', 'captures', 'work'}
allowed_examples = {'192.168.1.100', '192.168.1.2'}
issues = []
for p in root.rglob('*'):
    if not p.is_file() or any(part in skip for part in p.relative_to(root).parts):
        continue
    if p.name == 'printer.json' or p.name.startswith('.env') or p.suffix in {'.log', '.pcap', '.pcapng'}:
        issues.append((p, 'private configuration or capture'))
    if p.suffix not in {'.rs', '.ts', '.json', '.md', '.toml', '.html', '.css', '.yml', '.yaml', '.py', '.plist'}:
        continue
    text = p.read_text(errors='replace')
    for match in re.findall(r'\b(?:192\.168\.\d{1,3}\.\d{1,3}|10\.\d{1,3}\.\d{1,3}\.\d{1,3})\b', text):
        if match not in allowed_examples: issues.append((p, 'non-example private address'))
    if re.search(r'(?:/Users/|C:\\Users\\)[A-Za-z][^/\\\s"\']+', text): issues.append((p, 'personal filesystem path'))
    if re.search(r'rtsps://bblp:(?!\{\}|TEST1234)[A-Za-z0-9]{8}@', text): issues.append((p, 'embedded camera credential'))
for path, reason in issues: print(f'{path.relative_to(root)}: {reason}')
if issues: sys.exit(1)
print('Source privacy checks passed. Before release, also review Git history, author identity, assets, release binaries, and screenshots.')
