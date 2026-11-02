# Test Support

Most unit and platform regression tests live beside the source they exercise. This directory holds the shared test-only material:

- `reference/tools.rs`: the earlier tool implementation used to compare actual DLL pixels and history.
- [Replay compatibility fixtures](fixtures/replay/README.md): original versioned archives and expected pixel hashes.
- `fixtures/native-incompatible/`: a deliberately incompatible native plugin used to verify ABI rejection.

Fixture source and intentional historical replay archives are tracked; compiled DLLs and generated captures are not. Build the real plugin DLLs before host tests. `build.ps1 -Test` explicitly runs the normally ignored actual-DLL lifecycle test in addition to regular tests. Optional performance tests are local measurements, not fixed CI timing thresholds.

Read [Development](../docs/development.md) for commands and the separate end-to-end smoke checks.
