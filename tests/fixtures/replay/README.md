# Replay v1 compatibility fixture

[v1.mcreplay](v1.mcreplay) was exported by the original MiCoPi 0.20.0 CLI, with a relative, isolated plugin directory. It contains a 32×24 transparent two-layer document, brush parameter changes and a view change. It intentionally uses the original full-metadata/full-tile MCRPLY01 encoding. No DLL or private artwork is required to read it.

[v1.json](v1.json) records its producer, dimensions and original archive/PNG SHA-256 values. The compatibility test verifies decoding and promotion to MCRPLY02 without changing event hashes or pixels. This is an intentional source-controlled fixture, not generated user artwork.

Keep historical fixtures immutable. Add a separately named fixture and provenance record for a new format instead of regenerating v1 with the current encoder. Generated replay captures belong under ignored `target/test-output/`.
