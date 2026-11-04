"""Package an existing Windows build with the current default plugins and documentation."""
import argparse
from pathlib import Path
import json
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def package(dist: Path, output: Path):
    version = tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    entries = {}

    def add(source, destination):
        source = Path(source)
        if not source.is_file():
            raise FileNotFoundError(f'Missing {source}; build with scripts/build.ps1 first')
        if destination in entries:
            raise ValueError(f'Duplicate archive path: {destination}')
        entries[destination] = source

    gui_candidates = [p for p in [dist/f'MiCoPi-{version}.exe', *dist.glob(f'MiCoPi-{version}-build*.exe')]
                      if p.is_file()]
    if not gui_candidates:
        raise FileNotFoundError(f'No desktop build for {version} in {dist}')
    add(max(gui_candidates, key=lambda p: p.stat().st_mtime_ns), 'MiCoPi.exe')
    add(dist/f'MiCoPi-CLI-{version}.exe', 'micopi-cli.exe')
    for name in ['README.md', 'README.zh.md', 'README.ja.md', 'README.ko.md',
                 'LICENSE', 'CONTRIBUTING.md', 'AGENTS.md', 'CHANGELOG.md', 'ROADMAP.md']:
        add(ROOT/name, name)
    for directory in ['docs', 'examples', 'locales', 'assets/material-symbols']:
        for path in sorted((ROOT/directory).rglob('*')):
            if not path.is_file() or '__pycache__' in path.parts:
                continue
            relative = path.relative_to(ROOT)
            if 'archive' in relative.parts and path.suffix.lower() != '.md':
                continue
            add(path, relative.as_posix())
    # Use source-controlled default manifests, never a developer's installed settings.
    for manifest in sorted((ROOT/'plugins').glob('*/manifest.json')):
        add(manifest, manifest.relative_to(ROOT).as_posix())
        readme = manifest.parent/'README.md'
        if readme.exists():
            add(readme, readme.relative_to(ROOT).as_posix())
        library = json.loads(manifest.read_text(encoding='utf-8')).get('library')
        if library:
            if Path(library).name != library or not library.lower().endswith('.dll'):
                raise ValueError(f'Invalid native library path in {manifest}')
            relative = manifest.parent.relative_to(ROOT) / library
            add(dist/relative, relative.as_posix())
    add(ROOT/'plugins/README.md', 'plugins/README.md')
    licenses = sorted(p for p in (dist/'licenses').rglob('*') if p.is_file())
    if not licenses:
        raise FileNotFoundError('Missing staged licenses; run scripts/build.ps1 first')
    for path in licenses:
        add(path, path.relative_to(dist).as_posix())
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, source in sorted(entries.items()):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            data = source.read_bytes()
            if name == 'licenses/MaterialSymbols-NOTICE.md':
                data = data.replace(b'](LICENSE)', b'](MaterialSymbols-LICENSE.txt)')
            archive.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    with zipfile.ZipFile(output) as archive:
        bad_file = archive.testzip()
        if bad_file:
            raise ValueError(f'Archive verification failed: {bad_file}')
    print(f'Packaged MiCoPi {version}: {output} ({output.stat().st_size:,} bytes, {len(entries)} files)')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--dist', type=Path, default=ROOT/'dist')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    version = tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    package(args.dist.resolve(), (args.output or args.dist/f'MiCoPi-{version}.zip').resolve())
