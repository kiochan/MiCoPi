"""Check virtual archive links and portable identities without executable builds."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PACKAGE = load('package_fixture', ROOT / 'scripts/package.py')
CI_PACKAGE = load('ci_package_fixture', ROOT / 'scripts/package-ci.py')
LICENSE_FIXTURES = load('license_fixtures', ROOT / 'scripts/test-collect-licenses.py')


class PackageTests(unittest.TestCase):
    def test_root_nested_images_and_directory_links_use_archive_paths(self):
        PACKAGE.validate_markdown_links({
            'README.md': b'[Guide](docs/nested/guide.md) [Docs](/docs/) ![Image](/assets/image.png)',
            'docs/nested/guide.md': b'[Home](../../README.md) [Parent](../) [Root](/)',
            'assets/image.png': b'\xffbinary',
        })

    def test_encoded_paths_fragments_queries_and_angle_paths_resolve(self):
        PACKAGE.validate_markdown_links({
            'README.md': b'[Encoded](docs/a%20b.md?download=1#section) [Angle](<docs/a b.md>) ![Unicode](assets/%E7%94%BB.png)',
            'docs/a b.md': b'[Fragment](#section) [Root](%2FREADME.md)',
            'assets/\u753b.png': b'image',
        })

    def test_external_urls_and_fenced_examples_do_not_need_archive_entries(self):
        PACKAGE.validate_markdown_links({'README.md': b'''[Web](https://example.org/missing.md)
[Network](//example.org/file) [Email](mailto:artist@example.org) [Editor](codex://chat/example)
```markdown
[Sample](missing-backtick.md)
```
~~~~ markdown
[Sample](missing-tilde.md)
~~~
[Still fenced](missing-after-short-close.md)
~~~~
````markdown
```
[Inner sample](missing-inner.md)
````
'''})

    def test_missing_file_and_nonexistent_directory_are_reported(self):
        for href in ['missing.md', 'doc/', 'docs/not/', 'docs/guide.md']:
            with self.subTest(href=href), self.assertRaisesRegex(ValueError, 'missing archived link target'):
                PACKAGE.validate_markdown_links({'README.md': f'[Target]({href})'.encode(), 'docs/nothing/file.md': b''})

    def test_plain_and_encoded_paths_cannot_escape_the_archive(self):
        for href in ['../outside.md', '%2e%2e/outside.md', '/../../outside.md', 'docs/../../outside.md']:
            with self.subTest(href=href), self.assertRaisesRegex(ValueError, 'escapes archive'):
                PACKAGE.validate_markdown_links({'README.md': f'[Target]({href})'.encode()})

    def test_invalid_virtual_entry_paths_are_rejected(self):
        for name in ['/outside.md', '../outside.md', 'docs/../README.md', 'docs\\guide.md']:
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'Invalid virtual archive path'):
                PACKAGE.validate_markdown_links({name: b''})

    def test_fences_do_not_hide_links_after_the_closing_line(self):
        with self.assertRaisesRegex(ValueError, 'visible-missing.md'):
            PACKAGE.validate_markdown_links({'README.md': b'```md\n[Hidden](hidden.md)\n```\n[Visible](visible-missing.md)\n'})

    def fixture(self):
        parent = ROOT / 'target/test-output'
        parent.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix='package-fixtures-', dir=parent)
        base = Path(temporary.name).resolve()
        self.assertTrue(base.is_relative_to(parent.resolve()))
        self.addCleanup(temporary.cleanup)
        fixture = LICENSE_FIXTURES.Fixture(base)
        fixture.stage()
        root, dist = fixture.root, fixture.output.parent
        (root / 'Cargo.toml').write_text('[package]\nversion="0.1.0"\n', encoding='utf-8')
        for name in ['README.md', 'README.zh.md', 'README.ja.md', 'README.ko.md', 'CONTRIBUTING.md', 'AGENTS.md', 'CHANGELOG.md', 'ROADMAP.md']:
            (root / name).write_text('# Fixture\n', encoding='utf-8')
        (root / 'docs').mkdir()
        (root / 'docs/guide.md').write_text('[Home](../README.md)\n', encoding='utf-8')
        (root / 'README.md').write_text('[Guide](docs/guide.md) [Plugins](plugins/)\n', encoding='utf-8')
        (root / 'examples/config').mkdir(parents=True)
        (root / 'examples/config/settings.json').write_text('{"example":true}\n', encoding='utf-8')
        (root / 'plugins/feature').mkdir(parents=True)
        (root / 'plugins/README.md').write_text('[Feature](feature/)\n', encoding='utf-8')
        (root / 'plugins/feature/README.md').write_text('[Guide](../../docs/guide.md)\n', encoding='utf-8')
        (root / 'plugins/feature/manifest.json').write_text('{"id":"feature","library":"feature.dll"}', encoding='utf-8')
        (dist / 'plugins/feature').mkdir(parents=True)
        (dist / 'plugins/feature/feature.dll').write_bytes(b'fixture plugin')
        (dist / 'plugins/feature/manifest.json').write_text('{"id":"personal-installed-override"}', encoding='utf-8')
        (dist / 'MiCoPi-0.1.0.exe').write_bytes(b'fixture desktop')
        (dist / 'MiCoPi-CLI-0.1.0.exe').write_bytes(b'fixture cli')
        (dist / 'settings.json').write_text('{"personal":true}', encoding='utf-8')
        (dist / 'workspace.egui.json').write_text('{"personal":true}', encoding='utf-8')
        scripts = root / 'scripts'
        (scripts / 'licenses').mkdir(parents=True)
        (scripts / 'collect-licenses.py').write_bytes((ROOT / 'scripts/collect-licenses.py').read_bytes())
        (scripts / 'licenses/supplements.json').write_bytes(fixture.supplements.read_bytes())
        package = load('isolated_packager', ROOT / 'scripts/package.py')
        package.ROOT = root
        return fixture, package, base / 'output/fixture.zip'

    def test_missing_source_only_link_preserves_an_existing_zip(self):
        fixture, package, output = self.fixture()
        output.parent.mkdir()
        with zipfile.ZipFile(output, 'w') as archive:
            archive.writestr('retained.txt', b'Previous valid archive')
        previous = output.read_bytes()
        (fixture.root / 'docs/guide.md').write_text('[Source tooling](../scripts/licenses/README.md)\n', encoding='utf-8')
        # A file present in the checkout still cannot satisfy a portable link.
        (fixture.root / 'scripts/licenses/README.md').write_text('# Source-only guide\n', encoding='utf-8')
        with self.assertRaisesRegex(ValueError, 'missing archived link target'):
            package.package(fixture.output.parent, output)
        self.assertEqual(output.read_bytes(), previous)

    def test_missing_link_does_not_create_output_parent(self):
        fixture, package, output = self.fixture()
        (fixture.root / 'README.md').write_text('[Missing](missing.md)\n', encoding='utf-8')
        with self.assertRaises(ValueError):
            package.package(fixture.output.parent, output)
        self.assertFalse(output.parent.exists())

    def test_portable_identity_defaults_licenses_and_bytes_are_deterministic(self):
        fixture, package, output = self.fixture()
        package.package(fixture.output.parent, output)
        first = output.read_bytes()
        package.package(fixture.output.parent, output)
        self.assertEqual(output.read_bytes(), first)
        with zipfile.ZipFile(output) as archive:
            names = set(archive.namelist())
            self.assertEqual(archive.read('MiCoPi.exe'), b'fixture desktop')
            self.assertEqual(archive.read('micopi-cli.exe'), b'fixture cli')
            self.assertEqual(archive.read('plugins/feature/feature.dll'), b'fixture plugin')
            self.assertEqual(archive.read('plugins/feature/manifest.json'), (fixture.root / 'plugins/feature/manifest.json').read_bytes())
            self.assertTrue({'README.md', 'README.zh.md', 'README.ja.md', 'README.ko.md', 'licenses/LICENSES.json', 'licenses/sample-1.2.3/LICENSE'} <= names)
            self.assertTrue('examples/config/settings.json' in names)
            self.assertFalse({'settings.json', 'workspace.egui.json'} & names)
            self.assertFalse(any(name.startswith(('target/', 'scripts/')) or name.startswith('/') or '..' in name.split('/') for name in names))
            self.assertTrue(all(info.date_time == (1980, 1, 1, 0, 0, 0) for info in archive.infolist()))
            self.assertIsNone(archive.testzip())

    def test_plugin_catalogs_and_explicit_runtime_files_survive_packaging(self):
        fixture, package, output = self.fixture()
        plugin = fixture.root/'plugins/feature'
        (plugin/'locales').mkdir()
        (plugin/'locales/ja.json').write_text('{"title":"ブラシ"}', encoding='utf-8')
        (plugin/'manifest.json').write_text(json.dumps(dict(
            id='feature', library='feature.dll', locales={'ja':'locales/ja.json'})), encoding='utf-8')
        runtime = fixture.output.parent/'vcruntime140.dll'
        runtime.write_bytes(b'redistributed runtime')
        package.package(fixture.output.parent, output, extra_files=[(runtime, runtime.name)])
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(json.loads(archive.read('plugins/feature/locales/ja.json'))['title'], 'ブラシ')
            self.assertEqual(archive.read(runtime.name), runtime.read_bytes())
            self.assertNotIn('settings.json', archive.namelist())

    def test_missing_and_escaping_plugin_catalogs_fail_before_publication(self):
        fixture, package, output = self.fixture()
        manifest = fixture.root/'plugins/feature/manifest.json'
        for path in ['locales/missing.json', '../private.json', 'C:/private.json', 'locales\\ja.json', '/ja.json']:
            manifest.write_text(json.dumps(dict(id='feature', locales={'ja':path})), encoding='utf-8')
            with self.subTest(path=path), self.assertRaises((ValueError, FileNotFoundError)):
                package.package(fixture.output.parent, output)
            self.assertFalse(output.exists())

    def test_crt_admission_rejects_incomplete_debug_and_wrong_architecture(self):
        fixture, _, _ = self.fixture()
        runtime = fixture.root/'redist/14.44.0/x64/Microsoft.VC143.CRT'
        runtime.mkdir(parents=True)
        with self.assertRaisesRegex(ValueError, 'Incomplete'):
            CI_PACKAGE.runtime_files(runtime)
        required = ['vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll']
        for name in required:
            (runtime/name).write_bytes(b'CRT fixture')
        self.assertEqual([p.name for p in CI_PACKAGE.runtime_files(runtime)], sorted(required))
        (runtime/'vcruntime140d.dll').write_bytes(b'debug CRT')
        with self.assertRaisesRegex(ValueError, 'Unexpected'):
            CI_PACKAGE.runtime_files(runtime)
        with self.assertRaisesRegex(ValueError, 'redistributable directory'):
            CI_PACKAGE.runtime_files(runtime.parent)

    def test_ci_stage_uses_build_outputs_and_records_archive_hashes(self):
        fixture, _, output = self.fixture()
        root, build = fixture.root, fixture.output.parent
        (root/'scripts/package.py').write_bytes((ROOT/'scripts/package.py').read_bytes())
        (build/'micopi.exe').write_bytes(b'tested desktop')
        (build/'micopi-cli.exe').write_bytes(b'tested cli')
        (build/'feature.dll').write_bytes(b'tested native build')
        runtime = root/'redist/14.44.0/x64/Microsoft.VC143.CRT'
        runtime.mkdir(parents=True)
        for name in ['msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll']:
            (runtime/name).write_bytes(b'CRT fixture')
        ci = load('isolated_ci_packager', ROOT/'scripts/package-ci.py')
        ci.ROOT = root
        commit = 'b'*40

        def cargo_metadata(command, **kwargs):
            self.assertEqual(command[:2], ['cargo', 'metadata'])
            self.assertIn('--locked', command)
            self.assertIn('--offline', command)
            kwargs['stdout'].write(json.dumps(fixture.metadata).encode())

        with mock.patch('sys.argv', ['package-ci.py', '--crt', str(runtime), '--build', str(build),
                                     '--output', str(output.parent)]), \
                mock.patch.dict(os.environ, {'GITHUB_SHA':commit}, clear=True), \
                mock.patch.object(ci.subprocess, 'check_output', return_value=commit+'\n'), \
                mock.patch.object(ci.subprocess, 'run', side_effect=cargo_metadata):
            ci.main()
        archive_path = output.parent/f'MiCoPi-0.1.0-{commit[:12]}-Windows-x64.zip'
        self.assertTrue(archive_path.with_suffix('.zip.sha256').read_text().startswith(ci.digest(archive_path)))
        with zipfile.ZipFile(archive_path) as archive:
            self.assertEqual(archive.read('MiCoPi.exe'), b'tested desktop')
            self.assertEqual(archive.read('plugins/feature/feature.dll'), b'tested native build')
            info = json.loads(archive.read('BUILD-INFO.json'))
            self.assertEqual(info['commit'], commit)
            self.assertEqual(info['runtime_version'], '14.44.0')
            self.assertEqual(info['sha256']['MiCoPi.exe'], ci.digest(build/'micopi.exe'))
            self.assertNotIn('metadata.json', archive.namelist())


if __name__ == '__main__':
    unittest.main()
