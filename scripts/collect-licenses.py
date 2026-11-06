"""Stage bounded, locked dependency notices without network access."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import tempfile
import tomllib

MAX_JSON = 32 * 1024 * 1024
MAX_TEXT = 4 * 1024 * 1024
MAX_TOTAL = 64 * 1024 * 1024
MAX_PACKAGES = 4096
MAX_ENTRIES = 100000
NAME = re.compile(r'^[A-Za-z0-9][A-Za-z0-9_.+-]*$')
NOTICE = re.compile(r'(licen[cs]e|copyright)|^(copying|notice|ofl|ufl)([._-]|$)', re.I)
ROOT = Path(__file__).resolve().parents[1]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read(path, maximum=MAX_TEXT):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size > maximum:
        raise ValueError(f'Invalid, symlinked or oversized license input: {path}')
    data = path.read_bytes()
    if len(data) > maximum:
        raise ValueError(f'Oversized license input: {path}')
    return data


def read_json(path):
    return json.loads(read(path, MAX_JSON).decode('utf-8-sig'))


def relative(value):
    path = PurePosixPath(value.replace('\\', '/'))
    if not value or path.is_absolute() or ':' in value or any(p in ('..', '.') for p in path.parts):
        raise ValueError(f'Unsafe license path: {value}')
    return path


def within(root, value):
    path = root.joinpath(*relative(value).parts)
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError(f'License path escapes source: {value}')
    return path


def recognized(data):
    """Recognize complete standard grants, not a filename claiming a license."""
    text = ' '.join(data.decode('utf-8-sig').lower().split())
    result = set()
    if 'permission is hereby granted, free of charge' in text and 'substantial portions' in text and 'software is provided' in text:
        result.add('MIT')
    if 'apache license' in text and 'version 2.0' in text and 'end of terms and conditions' in text:
        result.add('Apache-2.0')
    if 'boost software license' in text and 'version 1.0' in text and 'machine-executable object code' in text:
        result.add('BSL-1.0')
    if 'sil open font license' in text and 'version 1.1' in text and 'termination' in text:
        result.add('OFL-1.1')
    if 'ubuntu font licence version 1.0' in text and 'termination' in text:
        result.add('Ubuntu-font-1.0')
    if 'unicode license v3' in text and 'permission is hereby granted' in text and 'without restriction' in text:
        result.add('Unicode-3.0')
    if 'permission to use, copy, modify, and/or distribute' in text and 'for any purpose with or without fee' in text and 'provided that' in text:
        result.add('ISC')
    if 'this software is provided' in text and 'the origin of this software must not be misrepresented' in text and 'altered source versions' in text:
        result.add('Zlib')
    if 'this is free and unencumbered software released into the public domain' in text and 'for more information' in text:
        result.add('Unlicense')
    if 'redistribution and use in source and binary forms' in text and 'this software is provided' in text:
        if 'neither the name' in text:
            result.add('BSD-3-Clause')
        else:
            result.add('BSD-2-Clause')
    return result


def covered(expression, licenses):
    """Evaluate the supported SPDX AND/OR grammar, including legacy MIT/Apache."""
    tokens = re.findall(r'\(|\)|AND|OR|[A-Za-z0-9.+-]+', expression.replace('/', ' OR '))
    if ''.join(tokens) != re.sub(r'\s+', '', expression.replace('/', ' OR ')):
        raise ValueError(f'Unsupported license expression: {expression}')
    position = 0

    def term():
        nonlocal position
        if position >= len(tokens):
            raise ValueError(f'Incomplete license expression: {expression}')
        token = tokens[position]
        position += 1
        if token == '(':
            value = either()
            if position >= len(tokens) or tokens[position] != ')':
                raise ValueError(f'Unbalanced license expression: {expression}')
            position += 1
            return value
        if token in ('AND', 'OR', ')'):
            raise ValueError(f'Invalid license expression: {expression}')
        return token in licenses

    def both():
        nonlocal position
        value = term()
        while position < len(tokens) and tokens[position] == 'AND':
            position += 1
            other = term()
            value = value and other
        return value

    def either():
        nonlocal position
        value = both()
        while position < len(tokens) and tokens[position] == 'OR':
            position += 1
            other = both()
            value = value or other
        return value

    value = either()
    if position != len(tokens):
        raise ValueError(f'Invalid license expression: {expression}')
    return value


def registry_notices(directory, key, checksum, explicit, source_headers):
    """Read original notices directly from the checksum-verified Cargo archive."""
    archive_path = directory.parents[2] / 'cache' / directory.parent.name / f'{key}.crate'
    archive_bytes = read(archive_path, MAX_JSON)
    if digest(archive_bytes) != checksum:
        raise ValueError(f'Cargo.lock archive checksum mismatch: {key}')
    required = {relative(p).as_posix() for p in explicit}
    headers = {}
    for header in source_headers:
        name = relative(header['path']).as_posix()
        if type(header['lines']) is not int or not 1 <= header['lines'] <= 64 or name in headers:
            raise ValueError(f'Invalid source attribution header: {key}/{name}')
        headers[name] = header['lines']
    notices, evidence, attributions, seen = {}, {}, {}, set()
    uncompressed = 0
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode='r:gz') as archive:
        for entry in archive:
            if len(seen) >= MAX_ENTRIES:
                raise ValueError(f'Crate exceeds entry scan bound: {key}')
            path = relative(entry.name)
            if path.parts[0] != key or len(path.parts) < 2:
                raise ValueError(f'Crate path does not match package: {entry.name}')
            path = PurePosixPath(*path.parts[1:])
            name = path.as_posix()
            if name in seen or not (entry.isfile() or entry.isdir()):
                raise ValueError(f'Duplicate or non-regular crate entry: {key}/{name}')
            seen.add(name)
            uncompressed += entry.size
            if uncompressed > 256 * 1024 * 1024:
                raise ValueError(f'Crate exceeds decompression bound: {key}')
            if not entry.isfile():
                continue
            selected = name in required or NOTICE.search(path.name) or any(p.lower() in ('licenses', 'licences', 'license', 'licence') for p in path.parts[:-1])
            if selected or name in headers or name in ('Cargo.toml', '.cargo_vcs_info.json'):
                if entry.size > MAX_TEXT:
                    raise ValueError(f'Oversized crate notice: {key}/{name}')
                data = archive.extractfile(entry).read(MAX_TEXT + 1)
                if selected:
                    notices[name] = data
                if name in ('Cargo.toml', '.cargo_vcs_info.json'):
                    evidence[name] = data
                if name in headers:
                    lines = data.splitlines(keepends=True)[:headers[name]]
                    if len(lines) != headers[name] or any(line.strip() and not line.lstrip().startswith(b'//') for line in lines):
                        raise ValueError(f'Source attribution is not a complete comment header: {key}/{name}')
                    attributions[name] = b''.join(lines)
    if required - notices.keys():
        raise ValueError(f'Missing required bundled notices: {key}: {sorted(required - notices.keys())}')
    if headers.keys() - attributions.keys():
        raise ValueError(f'Missing required source attributions: {key}')
    return notices, evidence, attributions


def collect(root, metadata, lock_path, supplements_path):
    root = Path(root).resolve()
    metadata = read_json(metadata) if isinstance(metadata, (str, Path)) else metadata
    lock_bytes = read(lock_path, MAX_JSON)
    lock = tomllib.loads(lock_bytes.decode('utf-8'))
    locked = {(p['name'], p['version'], p.get('source')): p for p in lock['package']}
    supplements_path = Path(supplements_path)
    supplements = read_json(supplements_path)
    if supplements.get('version') != 1:
        raise ValueError('Unsupported supplemental license format')
    reviewed_names = {item['name'] for item in supplements.get('packages', {}).values()}
    packages = metadata.get('packages', [])
    if not 0 < len(packages) <= MAX_PACKAGES:
        raise ValueError('Invalid package count in Cargo metadata')
    members = set(metadata.get('workspace_members', []))
    if len({p['id'] for p in packages}) != len(packages) or {p['id'] for p in packages} != {n['id'] for n in metadata.get('resolve', {}).get('nodes', [])}:
        raise ValueError('Cargo metadata has incomplete or duplicate resolved packages')
    if Path(metadata.get('workspace_root', '')).resolve() != root:
        raise ValueError('Cargo metadata belongs to a different workspace')
    outputs = {}
    output_names = set()
    records = []
    total = 0

    def add(path, data, provenance):
        nonlocal total
        path = relative(path).as_posix()
        if path.casefold() in output_names:
            raise ValueError(f'Duplicate staged license path: {path}')
        if not data or len(data) > MAX_TEXT or b'\0' in data:
            raise ValueError(f'Invalid license text: {path}')
        data.decode('utf-8-sig')
        total += len(data)
        if total > MAX_TOTAL or len(outputs) >= MAX_PACKAGES * 8:
            raise ValueError('License output exceeds its bounded budget')
        outputs[path] = data
        output_names.add(path.casefold())
        return {'path': path, 'sha256': digest(data), 'bytes': len(data), **provenance}

    for package in sorted(packages, key=lambda p: (p['name'], p['version'], p.get('source') or '')):
        name, version, source = package['name'], package['version'], package.get('source')
        key = f'{name}-{version}'
        if not NAME.fullmatch(name) or not NAME.fullmatch(version):
            raise ValueError(f'Invalid package identity: {key}')
        locked_package = locked.get((name, version, source))
        if locked_package is None:
            raise ValueError(f'Metadata package is absent from Cargo.lock: {key}')
        directory = Path(package['manifest_path']).resolve().parent
        files = []
        licenses = set()
        expression = package.get('license')
        if not expression:
            raise ValueError(f'Missing SPDX license declaration: {key}')
        if source is None:
            if package['id'] not in members or not directory.is_relative_to(root) or expression != 'MIT':
                raise ValueError(f'Unrecognized workspace license ownership: {key}')
            data = read(root / 'LICENSE')
            licenses |= recognized(data)
            files.append(add(f'{key}/LICENSE', data, {'kind': 'workspace', 'source_path': 'LICENSE'}))
            checksum = None
        else:
            if not source.startswith('registry+'):
                raise ValueError(f'Unsupported dependency source requires explicit review: {key}: {source}')
            checksum = locked_package.get('checksum')
            supplement = supplements.get('packages', {}).get(key, {})
            if name in reviewed_names and not supplement:
                raise ValueError(f'Dependency upgrade requires supplemental license review: {key}')
            explicit = list(supplement.get('required_files', []))
            if package.get('license_file'):
                license_file = Path(package['license_file'])
                explicit.append(license_file.relative_to(directory).as_posix() if license_file.is_absolute() else str(license_file))
            notices, evidence, headers = registry_notices(directory, key, checksum, explicit, supplement.get('source_headers', []))
            declaration = tomllib.loads(evidence['Cargo.toml'].decode('utf-8'))['package']
            if (declaration['name'], declaration['version'], declaration.get('license')) != (name, version, expression):
                raise ValueError(f'Metadata differs from locked crate declaration: {key}')
            if supplement:
                vcs = json.loads(evidence['.cargo_vcs_info.json'])
                if supplement['checksum'] != checksum or supplement['revision'] != vcs.get('git', {}).get('sha1') or supplement['license'] != expression:
                    raise ValueError(f'Pinned supplemental provenance mismatch: {key}')
            for path, data in sorted(notices.items()):
                licenses |= recognized(data)
                files.append(add(f'{key}/{path}', data, {'kind': 'cargo-registry', 'source_path': path, 'crate_checksum': checksum}))
            for path, data in sorted(headers.items()):
                files.append(add(f'{key}/source-attributions/{path}.txt', data, {'kind': 'cargo-registry-copyright-header', 'source_path': path, 'lines': next(h['lines'] for h in supplement['source_headers'] if h['path'] == path), 'crate_checksum': checksum}))
            for extra in supplement.get('files', []):
                descriptor = supplements['files'][extra['source']]
                data = read(within(supplements_path.parent, descriptor['path']))
                blob = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
                if digest(data) != descriptor['sha256'] or blob != descriptor['git_blob_sha']:
                    raise ValueError(f'Vendored upstream notice fails checksum: {extra["source"]}')
                licenses |= recognized(data)
                files.append(add(f'{key}/{extra["destination"]}', data, {'kind': extra.get('kind', 'upstream'), 'url': descriptor['url'], 'revision': descriptor['revision'], 'git_blob_sha': blob}))
        if not files or not covered(expression, licenses):
            raise ValueError(f'Unresolved license coverage: {key}: {expression}; found {sorted(licenses)}')
        record = {'name': name, 'version': version, 'source': source, 'checksum': checksum, 'license': expression, 'covered_licenses': sorted(licenses), 'repository': package.get('repository'), 'files': files}
        if source and supplement.get('selection'):
            record['license_selection'] = supplement['selection']
        records.append(record)

    assets = []
    for source, destination in [('LICENSE', 'MiCoPi-LICENSE.txt'), ('assets/material-symbols/LICENSE', 'MaterialSymbols-LICENSE.txt'), ('assets/material-symbols/NOTICE.md', 'MaterialSymbols-NOTICE.md')]:
        data = read(root / source)
        if destination.endswith('NOTICE.md'):
            data = data.replace(b'](LICENSE)', b'](MaterialSymbols-LICENSE.txt)')
        assets.append(add(destination, data, {'kind': 'workspace-asset', 'source_path': source}))
    manifest = {'version': 1, 'cargo_lock_sha256': digest(lock_bytes), 'collector_sha256': digest(read(__file__)), 'supplements_sha256': digest(read(supplements_path, MAX_JSON)), 'package_count': len(records), 'packages': records, 'assets': assets}
    outputs['LICENSES.json'] = (json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode('utf-8')
    lines = ['# MiCoPi dependency licenses', '', 'Generated offline from locked Cargo metadata. LICENSES.json records coverage, SHA-256 hashes and source provenance. Nested notices and bundled font licenses are retained.', '']
    lines += [f'- {p["name"]} {p["version"]}: {p["license"]} — [{len(p["files"])} notice files]({p["name"]}-{p["version"]}/)' for p in records]
    outputs['DEPENDENCIES.md'] = ('\n'.join(lines) + '\n').encode('utf-8')
    return outputs, manifest


def verify_staged(output, lock_path, supplements=None):
    output = Path(output)
    manifest = read_json(output / 'LICENSES.json')
    if manifest.get('version') != 1 or manifest.get('cargo_lock_sha256') != digest(read(lock_path, MAX_JSON)):
        raise ValueError('Staged license manifest does not match Cargo.lock')
    supplements = supplements or ROOT / 'scripts/licenses/supplements.json'
    if manifest.get('collector_sha256') != digest(read(__file__)) or manifest.get('supplements_sha256') != digest(read(supplements, MAX_JSON)):
        raise ValueError('Staged licenses require regeneration for the current collector policy')
    packages = manifest.get('packages', [])
    locked = {(p['name'], p['version'], p.get('source')): p for p in tomllib.loads(read(lock_path, MAX_JSON).decode('utf-8'))['package']}
    if not packages or manifest.get('package_count') != len(packages) or len(packages) > MAX_PACKAGES:
        raise ValueError('Invalid staged license package coverage')
    expected = {'LICENSES.json', 'DEPENDENCIES.md'}
    seen = set()
    for package in packages:
        identity = (package['name'], package['version'], package['source'])
        if identity in seen or identity not in locked or locked[identity].get('checksum') != package['checksum']:
            raise ValueError(f'Staged package differs from Cargo.lock: {package.get("name")}')
        seen.add(identity)
        if not package.get('files') or not covered(package['license'], set(package['covered_licenses'])):
            raise ValueError(f'Incomplete staged license coverage: {package.get("name")}')
        actual_licenses = set()
        for item in package['files']:
            data = read(within(output, item['path']))
            if digest(data) != item['sha256'] or len(data) != item['bytes']:
                raise ValueError(f'Staged license checksum mismatch: {item["path"]}')
            actual_licenses |= recognized(data)
        if not covered(package['license'], actual_licenses):
            raise ValueError(f'Staged notice text does not cover package: {package.get("name")}')
    if not {p for p in locked if p[2] is None} <= seen:
        raise ValueError('Missing workspace package license provenance')
    for item in [*manifest.get('assets', []), *(f for p in packages for f in p['files'])]:
        path = relative(item['path']).as_posix()
        if path in expected:
            raise ValueError(f'Duplicate staged notice: {path}')
        data = read(within(output, path))
        if digest(data) != item['sha256'] or len(data) != item['bytes']:
            raise ValueError(f'Staged license checksum mismatch: {path}')
        expected.add(path)
    actual = set()
    for directory, children, files in os.walk(output, followlinks=False):
        if any((Path(directory) / name).is_symlink() for name in children):
            raise ValueError('Symlink in staged license directory')
        actual.update((Path(directory) / name).relative_to(output).as_posix() for name in files)
    if expected != actual:
        raise ValueError(f'Stale or missing staged licenses: {sorted(expected ^ actual)}')
    return manifest


def stage(root, metadata, lock_path, supplements, output):
    if Path(output).is_symlink():
        raise ValueError('License output must not be a symlink')
    root, output = Path(root).resolve(), Path(output).resolve()
    if output.name != 'licenses' or root.is_relative_to(output):
        raise ValueError('License output must be a dedicated licenses directory outside source ancestors')
    outputs, manifest = collect(root, metadata, lock_path, supplements)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.licenses-stage-', dir=output.parent) as temporary:
        temporary = Path(temporary)
        staged = temporary / 'licenses'
        staged.mkdir()
        for name, data in sorted(outputs.items()):
            path = within(staged, name)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        verify_staged(staged, lock_path, supplements)
        old = temporary / 'previous'
        if output.exists():
            if output.is_symlink() or not output.is_dir():
                raise ValueError('License output is not a regular directory')
            output.rename(old)
        try:
            staged.rename(output)
        except OSError:
            if old.exists():
                old.rename(output)
            raise
        # TemporaryDirectory removes only this checked, dedicated sibling tree.
        if temporary.parent != output.parent or not temporary.resolve().is_relative_to(output.parent):
            raise ValueError('Unexpected license temporary path')
    return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--metadata', type=Path, required=True)
    parser.add_argument('--lock', type=Path)
    parser.add_argument('--supplements', type=Path, default=ROOT / 'scripts/licenses/supplements.json')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = stage(args.root, args.metadata, args.lock or args.root / 'Cargo.lock', args.supplements, args.output)
    except (OSError, ValueError, KeyError, tarfile.TarError) as error:
        parser.exit(1, f'License staging failed: {error}\n')
    print(f'Staged verified licenses for {result["package_count"]} packages in {args.output}')
