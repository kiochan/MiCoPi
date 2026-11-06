# Test Support

Most unit and platform regression tests live beside the source they exercise. This directory holds the shared test-only material:

- `reference/tools.rs`: the earlier tool implementation used to compare actual DLL pixels and history.
- [Replay compatibility fixtures](fixtures/replay/README.md): original versioned archives and expected pixel hashes.
- `fixtures/native-incompatible/`: a deliberately incompatible native plugin used to verify ABI rejection.
- `fixtures/native-layout.c`: compile-only C11/C++17 x64 SDK layouts and callback signatures, checked by `scripts/check-native-abi.py` with the existing portable compiler or explicit `--cc`.

`scripts/smoke-native-masks.py` exercises actual A8 tool and coverage compositor DLL tables through synthetic scoped loans and independent byte references. Native crate tests cover kernel arithmetic, malformed descriptors and prefix compatibility. These prerequisites are separate from the planned document mask feature and do not validate Session mask history or physical tablet hardware. CI runs the native crates and actual-DLL conformance alongside the host suites.

Fixture source and intentional historical replay archives are tracked; compiled DLLs and generated captures are not. Build the real plugin DLLs before host tests. `build.ps1 -Test` explicitly runs the normally ignored actual-DLL lifecycle test in addition to regular tests. Optional performance tests are local measurements, not fixed CI timing thresholds.

Read [Development](../docs/development.md) for commands and the separate end-to-end smoke checks.
