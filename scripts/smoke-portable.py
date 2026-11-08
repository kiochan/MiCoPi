"""Verify and run an extracted CI package with its own plugins and isolated profile."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--zip', type=Path, required=True)
    args = parser.parse_args()
    archive_path = args.zip.resolve()
    with archive_path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    assert archive_path.with_suffix('.zip.sha256').read_text(encoding='ascii') == f'{digest}  {archive_path.name}\n'
    # Outside the checkout: plugin discovery must use the executable's directory.
    with tempfile.TemporaryDirectory(prefix='micopi-portable-') as temporary:
        base = Path(temporary).resolve()
        app, profile, work = base/'app', base/'profile', base/'work'
        work.mkdir()
        with zipfile.ZipFile(archive_path) as archive:
            names = archive.namelist()
            assert len(names) == len(set(names))
            assert all(not name.startswith('/') and '\\' not in name and ':' not in name
                       and '..' not in name.split('/') for name in names)
            assert not {'settings.json', 'workspace.egui.json'} & set(names)
            assert archive.testzip() is None
            archive.extractall(app)
        info = json.loads((app/'BUILD-INFO.json').read_text(encoding='utf-8'))
        expected_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        assert info['commit'] == expected_commit
        for name, expected in info['sha256'].items():
            with (app/name).open('rb') as stream:
                assert hashlib.file_digest(stream, 'sha256').hexdigest() == expected, name
        expected_native = set()
        for manifest in (app/'plugins').glob('*/manifest.json'):
            data = json.loads(manifest.read_text(encoding='utf-8'))
            if data.get('library'):
                expected_native.add(data['id'])
                assert (manifest.parent/data['library']).is_file()
            for locale in data.get('locales', {}).values():
                json.loads((manifest.parent/locale).read_text(encoding='utf-8'))
        for language in ['en', 'zh-CN', 'ja', 'ko']:
            json.loads((app/f'locales/{language}.json').read_text(encoding='utf-8'))
        commands = [dict(action='plugins.list', args={}),
                    dict(action='canvas.new', args=dict(width=16, height=16)),
                    dict(action='brush.set', args=dict(radius=2, stabilizer=0)),
                    dict(action='canvas.stroke', args=dict(points=[dict(x=2, y=2, pressure=.5, pen=True),
                                                                 dict(x=12, y=12, pressure=1, pen=True)])),
                    dict(action='native.invert', args={}),
                    dict(action='canvas.snapshot', args={}),
                    dict(action='replay.verify', args={})]
        script = work/'commands.jsonl'
        script.write_text(''.join(json.dumps(row)+'\n' for row in commands), encoding='utf-8')
        run = subprocess.run([str(app/'micopi-cli.exe'), 'run', str(script), '--config-dir', str(profile)],
                             cwd=work, capture_output=True, text=True, encoding='utf-8', timeout=60)
        assert run.returncode == 0, (run.stdout, run.stderr)
        rows = [json.loads(line) for line in run.stdout.splitlines()]
        assert len(rows) == len(commands) and all(row['ok'] for row in rows), rows
        inventory = rows[0]['data']['result']
        assert not inventory['diagnostics'], inventory['diagnostics']
        assert {p['id'] for p in inventory['native']} == expected_native
        assert rows[-2]['data']['result']['data']
        assert rows[-1]['data']['result']['valid']
    print('Portable ZIP: checksums, catalogs, default DLL discovery, painting and replay passed.')


if __name__ == '__main__':
    main()
