"""Check documentation links, example data and Git artifact boundaries without building."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
LINK = re.compile(r'!?\[[^\]]*\]\((<[^>]+>|[^\s)]+)(?:\s+"[^"]*")?\)')


def check(root=ROOT, check_git=True):
    errors = []
    excluded = {'.git', 'target', 'dist', 'artworks', '.codex', '.agents', '__pycache__'}
    documents = []
    for directory, children, names in os.walk(root):
        children[:] = sorted(name for name in children if name not in excluded)
        documents.extend(Path(directory)/name for name in sorted(names) if name.endswith('.md'))
    for document in documents:
        # Code samples are not Markdown links.
        text = re.sub(r'```[\s\S]*?```', '', document.read_text(encoding='utf-8'))
        for match in LINK.finditer(text):
            href = match[1].strip('<>')
            parsed = urlsplit(href)
            if parsed.scheme or parsed.netloc or not parsed.path:
                continue
            target = (root / unquote(parsed.path).lstrip('/') if parsed.path.startswith('/')
                      else document.parent / unquote(parsed.path))
            if not target.exists():
                errors.append(f'{document.relative_to(root)}: missing link target {href}')
    data_files = sorted((root/'examples').rglob('*.json'))
    data_files += sorted((root/'examples').rglob('*.jsonl'))
    for path in data_files:
        try:
            text = path.read_text(encoding='utf-8-sig')
            if path.suffix == '.jsonl':
                for number, line in enumerate(text.splitlines(), 1):
                    if line.strip():
                        record = json.loads(line)
                        if not isinstance(record, dict) or not isinstance(record.get('action'), str):
                            errors.append(f'{path.relative_to(root)}:{number}: expected an action envelope')
            else:
                json.loads(text)
        except (ValueError, OSError) as error:
            errors.append(f'{path.relative_to(root)}: {error}')
    if check_git and (root/'.git').exists():
        result = subprocess.run(['git', 'ls-files', '-z'], cwd=root, capture_output=True, check=True)
        for name in result.stdout.decode('utf-8').split('\0'):
            if not name:
                continue
            path = Path(name)
            if (path.parts[0] in {'target', 'dist', 'artworks', '.env', '.codex', '.agents'}
                    or 'target' in path.parts
                    or path.suffix.lower() in {'.exe', '.dll', '.pdb', '.dmp'}
                    or name in {'settings.json', 'session.json', 'workspace.egui.json'}):
                errors.append(f'Generated or personal file is tracked: {name}')
    return documents, data_files, errors


if __name__ == '__main__':
    documents, data_files, errors = check()
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        raise SystemExit(1)
    print(f'Repository checks passed: {len(documents)} Markdown files, '
          f'{len(data_files)} JSON/JSONL examples, and Git artifact boundaries.')
