"""Exercise license staging with isolated Cargo archive fixtures; no Rust build."""
import gzip
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('collector', ROOT / 'scripts/collect-licenses.py')
COLLECTOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COLLECTOR)
MIT = (ROOT / 'LICENSE').read_bytes()
APACHE = (ROOT / 'scripts/licenses/upstream/egui-44cdd653/LICENSE-APACHE').read_bytes()
SOURCE = 'registry+https://github.com/rust-lang/crates.io-index'
REVISION = 'a' * 40


class Fixture:
    def __init__(self, base, files=None, license='MIT'):
        self.base = base
        self.root = base / 'project'
        self.root.mkdir()
        (self.root / 'LICENSE').write_bytes(MIT)
        assets = self.root / 'assets/material-symbols'
        assets.mkdir(parents=True)
        (assets / 'LICENSE').write_bytes(APACHE)
        (assets / 'NOTICE.md').write_text('[License](LICENSE)\n', encoding='utf-8')
        self.directory = base / 'registry/src/index/sample-1.2.3'
        self.directory.mkdir(parents=True)
        self.archive = base / 'registry/cache/index/sample-1.2.3.crate'
        self.archive.parent.mkdir(parents=True)
        self.lock = self.root / 'Cargo.lock'
        self.supplements = base / 'supplements.json'
        self.supplements.write_text('{"version":1,"packages":{},"files":{}}', encoding='utf-8')
        self.output = base / 'staged/licenses'
        self.license = license
        self.files = dict(files if files is not None else {'LICENSE': MIT})
        self.write_archive()
        self.metadata = {'workspace_root': str(self.root), 'workspace_members': ['workspace'],
                         'packages': [{'id': 'workspace', 'name': 'example', 'version': '0.1.0', 'source': None, 'manifest_path': str(self.root / 'Cargo.toml'), 'license': 'MIT'},
                                      {'id': 'registry', 'name': 'sample', 'version': '1.2.3', 'source': SOURCE, 'manifest_path': str(self.directory / 'Cargo.toml'), 'license': license}],
                         'resolve': {'nodes': [{'id': 'workspace'}, {'id': 'registry'}]}}

    def write_archive(self, extra=()):
        entries = [('sample-1.2.3/Cargo.toml', f'[package]\nname="sample"\nversion="1.2.3"\nlicense="{self.license}"\n'.encode()),
                   ('sample-1.2.3/.cargo_vcs_info.json', json.dumps({'git': {'sha1': REVISION}}).encode())]
        entries += [('sample-1.2.3/' + path, data) for path, data in sorted(self.files.items())]
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            for name, data in entries + list(extra):
                info = tarfile.TarInfo(name)
                if data is None:
                    info.type, info.linkname = tarfile.SYMTYPE, '../../escape'
                    archive.addfile(info)
                else:
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))
        self.archive.write_bytes(gzip.compress(stream.getvalue(), mtime=0))
        self.checksum = COLLECTOR.digest(self.archive.read_bytes())
        self.lock.write_text(f'version=4\n[[package]]\nname="example"\nversion="0.1.0"\n[[package]]\nname="sample"\nversion="1.2.3"\nsource="{SOURCE}"\nchecksum="{self.checksum}"\n', encoding='utf-8')

    def supplement(self, **extra):
        package = {'name': 'sample', 'checksum': self.checksum, 'revision': REVISION, 'license': self.license, **extra}
        self.supplements.write_text(json.dumps({'version': 1, 'packages': {'sample-1.2.3': package}, 'files': {}}), encoding='utf-8')

    def collect(self):
        return COLLECTOR.collect(self.root, self.metadata, self.lock, self.supplements)

    def stage(self):
        return COLLECTOR.stage(self.root, self.metadata, self.lock, self.supplements, self.output)


class LicenseTests(unittest.TestCase):
    def setUp(self):
        parent = ROOT / 'target/test-output'
        parent.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix='license-fixtures-', dir=parent)
        self.base = Path(self.temporary.name).resolve()
        self.assertTrue(self.base.is_relative_to(parent.resolve()))
        self.addCleanup(self.temporary.cleanup)

    def fixture(self, **arguments):
        return Fixture(self.base, **arguments)

    def test_nested_notices_and_workspace_are_retained_from_verified_archive(self):
        fixture = self.fixture(files={'fonts/LICENSE-MIT': MIT, 'fonts/NOTICE.txt': b'Bundled font attribution\n'})
        (fixture.directory / 'LICENSE').write_bytes(b'Untrusted expanded cache text')
        output, manifest = fixture.collect()
        self.assertEqual(output['sample-1.2.3/fonts/LICENSE-MIT'], MIT)
        self.assertEqual(output['sample-1.2.3/fonts/NOTICE.txt'], b'Bundled font attribution\n')
        self.assertEqual(output['example-0.1.0/LICENSE'], MIT)
        self.assertEqual(manifest['package_count'], 2)
        self.assertNotIn(str(self.base).encode(), output['LICENSES.json'])

    def test_spdx_all_branches_are_parsed_and_and_terms_required(self):
        self.assertTrue(COLLECTOR.covered('(MIT OR Apache-2.0) AND OFL-1.1', {'MIT', 'OFL-1.1'}))
        self.assertFalse(COLLECTOR.covered('MIT AND Apache-2.0', {'MIT'}))
        self.assertTrue(COLLECTOR.covered('MIT/Apache-2.0', {'Apache-2.0'}))
        for expression in ['MIT OR (', 'MIT AND', 'MIT WITH exception', 'MIT !!!']:
            with self.subTest(expression=expression), self.assertRaises(ValueError):
                COLLECTOR.covered(expression, {'MIT'})

    def test_four_bundled_font_notices_are_mandatory_even_with_main_license(self):
        names = ['fonts/OFL.txt', 'fonts/UFL.txt', 'fonts/Hack-Regular.txt', 'fonts/emoji-icon-font-mit-license.txt']
        fixture = self.fixture(files={'LICENSE': MIT, **{n: b'Font attribution\n' for n in names}})
        fixture.supplement(required_files=names)
        output, _ = fixture.collect()
        self.assertTrue(all('sample-1.2.3/' + n in output for n in names))
        fixture.files.pop(names[2])
        fixture.write_archive()
        fixture.supplement(required_files=names)
        with self.assertRaisesRegex(ValueError, 'Missing required bundled notices'):
            fixture.collect()

    def test_missing_and_fake_grants_fail_coverage(self):
        fixture = self.fixture(files={'LICENSE': b'This is not the license grant.\n'})
        with self.assertRaisesRegex(ValueError, 'Unresolved license coverage'):
            fixture.collect()

    def test_explicit_source_copyright_headers_are_preserved_without_code(self):
        fixture = self.fixture(files={'LICENSE': MIT, 'src/lib.rs': b'// Copyright Example Authors\n// License terms are in LICENSE.\n\nfn implementation() {}\n'})
        fixture.supplement(source_headers=[{'path': 'src/lib.rs', 'lines': 2}])
        output, _ = fixture.collect()
        self.assertEqual(output['sample-1.2.3/source-attributions/src/lib.rs.txt'], b'// Copyright Example Authors\n// License terms are in LICENSE.\n')
        fixture.supplement(source_headers=[{'path': 'src/lib.rs', 'lines': 4}])
        with self.assertRaisesRegex(ValueError, 'not a complete comment header'):
            fixture.collect()

    def test_both_required_licenses_must_have_full_texts(self):
        fixture = self.fixture(license='Apache-2.0 AND MIT', files={'LICENSE-APACHE': APACHE})
        with self.assertRaisesRegex(ValueError, 'Unresolved license coverage'):
            fixture.collect()
        fixture.files['nested/LICENSE-MIT'] = MIT
        fixture.write_archive()
        fixture.collect()

    def test_bad_archive_checksum_does_not_replace_previous_output(self):
        fixture = self.fixture()
        fixture.stage()
        before = (fixture.output / 'LICENSES.json').read_bytes()
        fixture.archive.write_bytes(fixture.archive.read_bytes() + b'changed')
        with self.assertRaisesRegex(ValueError, 'archive checksum mismatch'):
            fixture.stage()
        self.assertEqual((fixture.output / 'LICENSES.json').read_bytes(), before)

    def test_locked_metadata_declaration_and_graph_are_checked(self):
        fixture = self.fixture()
        fixture.metadata['packages'][1]['license'] = 'Apache-2.0'
        with self.assertRaisesRegex(ValueError, 'differs from locked crate declaration'):
            fixture.collect()
        fixture.metadata['packages'][1]['license'] = 'MIT'
        fixture.metadata['resolve']['nodes'].pop()
        with self.assertRaisesRegex(ValueError, 'resolved packages'):
            fixture.collect()

    def test_supplement_pins_do_not_cover_another_crate_or_commit(self):
        fixture = self.fixture()
        for key, value in [('checksum', '0' * 64), ('revision', 'b' * 40), ('license', 'Apache-2.0')]:
            fixture.supplement(**{key: value})
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'supplemental provenance mismatch'):
                fixture.collect()

    def test_a_new_version_of_a_reviewed_dependency_requires_review(self):
        fixture = self.fixture()
        fixture.supplement()
        document = json.loads(fixture.supplements.read_text())
        document['packages']['sample-1.2.2'] = document['packages'].pop('sample-1.2.3')
        fixture.supplements.write_text(json.dumps(document))
        with self.assertRaisesRegex(ValueError, 'upgrade requires supplemental license review'):
            fixture.collect()

    def test_vendored_supplement_bytes_and_upstream_blob_are_verified(self):
        fixture = self.fixture(files={})
        data = APACHE
        (self.base / 'upstream.txt').write_bytes(data)
        descriptor = {'path': 'upstream.txt', 'sha256': COLLECTOR.digest(data), 'git_blob_sha': COLLECTOR.hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest(), 'url': 'https://github.com/example/upstream/blob/' + REVISION + '/LICENSE', 'revision': REVISION}
        fixture.license = 'Apache-2.0'
        fixture.write_archive()
        fixture.metadata['packages'][1]['license'] = fixture.license
        fixture.supplement(files=[{'source': 'apache', 'destination': 'LICENSE-APACHE'}])
        document = json.loads(fixture.supplements.read_text())
        document['files']['apache'] = descriptor
        fixture.supplements.write_text(json.dumps(document))
        fixture.collect()
        (self.base / 'upstream.txt').write_bytes(data + b'changed')
        with self.assertRaisesRegex(ValueError, 'upstream notice fails checksum'):
            fixture.collect()

    def test_archive_traversal_duplicate_and_links_are_rejected(self):
        fixture = self.fixture()
        cases = [('sample-1.2.3/../escape', b'bad'), ('sample-1.2.3/LICENSE', MIT), ('sample-1.2.3/link', None)]
        for entry in cases:
            fixture.write_archive(extra=[entry])
            with self.subTest(path=entry[0]), self.assertRaises(ValueError):
                fixture.collect()
        self.assertFalse((self.base / 'escape').exists())

    def test_windows_case_colliding_notices_are_rejected(self):
        fixture = self.fixture(files={'LICENSE': MIT, 'license': MIT})
        with self.assertRaisesRegex(ValueError, 'Duplicate staged license path'):
            fixture.collect()

    def test_oversized_notice_is_rejected_before_staging(self):
        fixture = self.fixture(files={'LICENSE': b'x' * (COLLECTOR.MAX_TEXT + 1)})
        with self.assertRaisesRegex(ValueError, 'Oversized crate notice'):
            fixture.stage()
        self.assertFalse(fixture.output.exists())

    def test_repeated_staging_is_deterministic_and_prunes_stale_files(self):
        fixture = self.fixture()
        first = fixture.stage()
        before = {p.relative_to(fixture.output).as_posix(): p.read_bytes() for p in fixture.output.rglob('*') if p.is_file()}
        (fixture.output / 'stale.txt').write_bytes(b'old')
        second = fixture.stage()
        after = {p.relative_to(fixture.output).as_posix(): p.read_bytes() for p in fixture.output.rglob('*') if p.is_file()}
        self.assertEqual(first, second)
        self.assertEqual(before, after)

    def test_packaging_verifier_rejects_edited_missing_and_stale_notices(self):
        fixture = self.fixture()
        fixture.stage()
        COLLECTOR.verify_staged(fixture.output, fixture.lock, fixture.supplements)
        (fixture.output / 'extra.txt').write_bytes(b'extra')
        with self.assertRaisesRegex(ValueError, 'Stale or missing'):
            COLLECTOR.verify_staged(fixture.output, fixture.lock, fixture.supplements)
        (fixture.output / 'extra.txt').unlink()
        (fixture.output / 'sample-1.2.3/LICENSE').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            COLLECTOR.verify_staged(fixture.output, fixture.lock, fixture.supplements)

    def test_packaging_verifier_rejects_lock_changes_and_lost_workspace(self):
        fixture = self.fixture()
        fixture.stage()
        original = fixture.lock.read_bytes()
        fixture.lock.write_bytes(original + b'\n')
        with self.assertRaisesRegex(ValueError, 'does not match Cargo.lock'):
            COLLECTOR.verify_staged(fixture.output, fixture.lock, fixture.supplements)
        fixture.lock.write_bytes(original)
        path = fixture.output / 'LICENSES.json'
        document = json.loads(path.read_text())
        document['packages'] = [p for p in document['packages'] if p['source']]
        document['package_count'] = len(document['packages'])
        path.write_text(json.dumps(document))
        with self.assertRaisesRegex(ValueError, 'Missing workspace'):
            COLLECTOR.verify_staged(fixture.output, fixture.lock, fixture.supplements)

    def test_packaging_verifier_rejects_old_collector_policy(self):
        fixture = self.fixture()
        fixture.stage()
        path = fixture.output / 'LICENSES.json'
        document = json.loads(path.read_text())
        document['collector_sha256'] = '0' * 64
        path.write_text(json.dumps(document))
        with self.assertRaisesRegex(ValueError, 'current collector policy'):
            COLLECTOR.verify_staged(fixture.output, fixture.lock, fixture.supplements)

    def test_output_cannot_replace_source_or_arbitrary_directory(self):
        fixture = self.fixture()
        for output in [fixture.root, self.base, self.base / 'not-licenses']:
            with self.subTest(output=output), self.assertRaises(ValueError):
                COLLECTOR.stage(fixture.root, fixture.metadata, fixture.lock, fixture.supplements, output)

    def test_packager_checks_notices_before_accepting_build_artifacts(self):
        fixture = self.fixture()
        fixture.stage()
        scripts = fixture.root / 'scripts'
        scripts.mkdir()
        (scripts / 'collect-licenses.py').write_bytes((ROOT / 'scripts/collect-licenses.py').read_bytes())
        (scripts / 'licenses').mkdir()
        (scripts / 'licenses/supplements.json').write_bytes(fixture.supplements.read_bytes())
        (fixture.root / 'Cargo.toml').write_text('[package]\nversion="0.1.0"\n', encoding='utf-8')
        spec = importlib.util.spec_from_file_location('packager_fixture', ROOT / 'scripts/package.py')
        packager = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(packager)
        packager.ROOT = fixture.root
        output = self.base / 'fixture.zip'
        with self.assertRaisesRegex(FileNotFoundError, 'No desktop build'):
            packager.package(fixture.output.parent, output)
        (fixture.output / 'sample-1.2.3/LICENSE').write_bytes(b'edited notice')
        with self.assertRaises(ValueError):
            packager.package(fixture.output.parent, output)
        self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
