# Test Support

Most unit and platform regression tests live beside the source they exercise. This directory holds the shared test-only material:

- `reference/tools.rs`: the earlier tool implementation used to compare actual DLL pixels and history.
- `fixtures/native-incompatible/`: a deliberately incompatible native plugin used to verify ABI rejection.

Fixture source is tracked; compiled DLLs and generated images are not. Build the real plugin DLLs before host tests. `build.ps1 -Test` explicitly runs the normally ignored actual-DLL lifecycle test in addition to regular tests. Optional performance tests are local measurements, not fixed CI timing thresholds.

Read [Development](../docs/development.md) for commands and the separate end-to-end smoke checks.
