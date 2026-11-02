# Material Symbols Rounded

Copyright Google LLC. Licensed under the Apache License, Version 2.0; see [LICENSE](LICENSE).

Upstream: https://github.com/google/material-design-icons
Revision: 10ecc14790612ca8fd164e3f4ffe9c16fc50d5b7
Guide: https://developers.google.com/fonts/docs/material_symbols

MiCoPi modifies the upstream variable font by selecting 40 codepoints, removing
ligatures, fixing FILL=0, GRAD=0, opsz=24 and wght=400, and renaming its family to
MiCoPi Symbols. The 8,152-byte font is embedded and registered privately for this
process; it is not installed on the user's system. Original copyright metadata
is retained. Full source/subset hashes and selected glyphs are in manifest.json.

Maintainers can regenerate with `python scripts/prepare-icons.py --download`
(requires fonttools). The committed manifest pins the upstream revision. Normal
Rust builds use the committed subset and require neither Python nor downloads.
Distribute the Apache license and this notice alongside the application.
