"""Stage a clean, already-tested MSVC build and produce a traceable portable ZIP."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CRT = re.compile(r'(?:concrt140|vccorlib140|msvcp140(?:_[a-z0-9_]+)?|vcruntime140(?:_[a-z0-9_]+)?)\.dll', re.I)


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT/'scripts'/f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def runtime_files(directory):
    """Accept only release x64 CRT files from a Visual Studio redistributable tree."""
    directory = directory.resolve(strict=True)
    if directory.parent.name != 'x64' or not re.fullmatch(r'Microsoft\.VC\d+\.CRT', directory.name):
        raise ValueError('Expected an x64 Microsoft.VC*.CRT redistributable directory')
    files = sorted(directory.glob('*.dll'))
    if any(not CRT.fullmatch(p.name) or p.is_symlink() for p in files):
        raise ValueError('Unexpected file in CRT redistributable directory')
    if not {'vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll'} <= {p.name.lower() for p in files}:
        raise ValueError('Incomplete x64 MSVC runtime')
    return files


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--crt', type=Path, required=True)
    parser.add_argument('--build', type=Path, default=ROOT/'target/release')
    parser.add_argument('--output', type=Path, default=ROOT/'target/artifacts')
    args = parser.parse_args()
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if not re.fullmatch('[0-9a-f]{40}', commit) or os.environ.get('GITHUB_SHA', commit) != commit:
        raise ValueError('Checkout does not match the CI commit')
    version = tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    if not re.fullmatch(r'[0-9A-Za-z.+-]+', version):
        raise ValueError('Unsafe version in package filename')
    crt = runtime_files(args.crt)
    parent = ROOT/'target/test-output'
    parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='ci-package-', dir=parent) as temporary:
        stage = Path(temporary).resolve()
        shutil.copyfile(args.build/'micopi.exe', stage/f'MiCoPi-{version}.exe')
        shutil.copyfile(args.build/'micopi-cli.exe', stage/f'MiCoPi-CLI-{version}.exe')
        for manifest in sorted((ROOT/'plugins').glob('*/manifest.json')):
            library = json.loads(manifest.read_text(encoding='utf-8')).get('library')
            if library:
                if '/' in library or '\\' in library or ':' in library or not library.endswith('.dll'):
                    raise ValueError(f'Invalid library in {manifest}')
                destination = stage/'plugins'/manifest.parent.name/library
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(args.build/library, destination)
        metadata = stage/'metadata.json'
        with metadata.open('wb') as stream:
            subprocess.run(['cargo', 'metadata', '--format-version', '1', '--locked', '--offline',
                            '--filter-platform', 'x86_64-pc-windows-msvc'], cwd=ROOT, stdout=stream, check=True)
        load('collect-licenses').stage(ROOT, metadata, ROOT/'Cargo.lock',
                                      ROOT/'scripts/licenses/supplements.json', stage/'licenses')
        extras = []
        for source in crt:
            destination = stage/source.name
            shutil.copyfile(source, destination)
            extras.append((destination, source.name))
        info = dict(version=version, commit=commit, target='x86_64-pc-windows-msvc',
                    kind='development', runtime_directory=args.crt.name,
                    runtime_version=args.crt.parent.parent.name,
                    run_url=(f'https://github.com/{os.environ["GITHUB_REPOSITORY"]}/actions/runs/'
                             f'{os.environ["GITHUB_RUN_ID"]}') if 'GITHUB_RUN_ID' in os.environ else None,
                    sha256={p.relative_to(stage).as_posix(): digest(p)
                            for p in sorted(stage.rglob('*')) if p.suffix.lower() in ('.exe', '.dll')})
        # Record installed archive names, rather than the temporary staging aliases.
        hashes = info['sha256']
        hashes['MiCoPi.exe'] = hashes.pop(f'MiCoPi-{version}.exe')
        hashes['micopi-cli.exe'] = hashes.pop(f'MiCoPi-CLI-{version}.exe')
        build_info = stage/'BUILD-INFO.json'
        build_info.write_text(json.dumps(info, indent=2, sort_keys=True)+'\n', encoding='utf-8')
        extras.append((build_info, build_info.name))
        output = args.output.resolve()/f'MiCoPi-{version}-{commit[:12]}-Windows-x64.zip'
        load('package').package(stage, output, extra_files=extras)
        checksum = output.with_suffix('.zip.sha256')
        checksum.write_text(f'{digest(output)}  {output.name}\n', encoding='ascii')
        if os.environ.get('GITHUB_OUTPUT'):
            with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as stream:
                stream.write(f'zip_path={output.as_posix()}\nchecksum_path={checksum.as_posix()}\n')
        print(checksum)


if __name__ == '__main__':
    main()
