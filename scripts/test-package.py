"""Check virtual archive links and portable identities without executable builds."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PACKAGE = load('package_fixture', ROOT / 'scripts/package.py')
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


if __name__ == '__main__':
    unittest.main()
