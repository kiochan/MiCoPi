# Pinned dependency license supplements

[collect-licenses.py](../collect-licenses.py) stages notices offline from the exact
Cargo registry archives selected by locked Cargo metadata. It checks each archive
against `Cargo.lock`, retains nested license/copyright/notice texts and evaluates
the declared SPDX AND/OR expression against recognized complete license grants.
It rejects missing coverage, changed supplement versions/checksums, invalid paths,
duplicate archive entries and oversized inputs before replacing the output tree.
It also includes the workspace MIT license for every workspace package and the
Material Symbols notices. The resulting `LICENSES.json` records file hashes,
package checksums, upstream URLs and the source of shared license texts.

Some crates omit repository-level licenses from their published archives.
[supplements.json](supplements.json) binds those additions to the exact crate
checksum and `.cargo_vcs_info.json` commit. Files under `upstream/` were obtained
from those official repository commits through the GitHub file API. Their bytes
are unchanged, including missing final newlines; the collector verifies both
SHA-256 and the upstream Git blob hash. The gl-rs repository moved from
`brendanzab/gl-rs` to `rust-windowing/gl-rs`; its pinned commits are preserved.

`egui-scale 0.3.0` is the explicit shared-license exception. Its pinned official
`Cargo.toml` declares `MIT OR Apache-2.0`, and its repository tree contains no
separate license or notice. The package includes that original declaration and
selects Apache-2.0, with the complete standard license text shared from the pinned
egui upstream file. This does not invent an author copyright statement.

AccessKit's root `LICENSE.chromium` and the original attribution comment headers
from its `src/lib.rs` and `src/geometry.rs` retain the AccessKit, Chromium and
kurbo notices. These headers are read from the locked archive; their bounded
line counts are explicit, and the collector rejects non-comment header content.

The four embedded `epaint_default_fonts 0.33.3` families require their original
notices: `fonts/OFL.txt` (Noto Emoji), `fonts/UFL.txt` (Ubuntu),
`fonts/Hack-Regular.txt` (Hack/Bitstream/DejaVu), and
`fonts/emoji-icon-font-mit-license.txt`. All four paths are mandatory in the
supplement; the collector reads their original bytes from the checksum-verified
crate, alongside the egui source license terms.

After a dependency update, review the actual upstream license and bundled assets,
update pins and required notices, and run `python scripts/test-collect-licenses.py`.
A package upgrade is not automatically covered by an older supplement. Builds do
not fetch legal texts from the network. `scripts/package.py` checks the staged
manifest, lock digest and every notice hash before creating an archive.
