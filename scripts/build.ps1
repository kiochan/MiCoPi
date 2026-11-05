param([switch]$Portable, [switch]$Test, [switch]$Benchmark, [switch]$Headless, [switch]$PluginsOnly, [string]$BuildRoot = 'target/build')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
$savedEnvironment = @{}
foreach ($key in @('RUSTC','RUSTDOC','CARGO_HOME','CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER','PATH','TEMP','TMP','MICOPI_TEST_BAD_DLL')) { $savedEnvironment[$key] = [Environment]::GetEnvironmentVariable($key) }
try {
    $buildTemp = Join-Path $projectRoot 'target/build-temp'
    New-Item -ItemType Directory -Path $buildTemp -Force | Out-Null
    $env:TEMP = $buildTemp
    $env:TMP = $buildTemp
    $cargo = 'cargo'
    $cargoArgs = @('--offline','--locked','--target-dir',$BuildRoot)
    $output = Join-Path $BuildRoot 'release/micopi.exe'
    if ($Portable) {
        $tools = Join-Path $projectRoot 'target/tools'
        $rustBin = Join-Path $tools 'rust-1.90.0/bin'
        $cargo = Join-Path $rustBin 'cargo.exe'
        $linker = Join-Path $tools 'llvm-mingw-20240619-ucrt-x86_64/bin/x86_64-w64-mingw32-clang.exe'
        if (!(Test-Path -LiteralPath $cargo)) { throw 'Run python scripts/bootstrap-egui.py first (Python 3.12+).' }
        if (!(Test-Path -LiteralPath $linker)) { throw 'Run python scripts/bootstrap-portable.py to prepare the project-local linker.' }
        $env:RUSTC = Join-Path $rustBin 'rustc.exe'
        $env:RUSTDOC = Join-Path $rustBin 'rustdoc.exe'
        $env:CARGO_HOME = Join-Path $projectRoot 'target/cargo-home'
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $linker
        $linkerBin = Split-Path -Parent $linker
        $env:PATH = "$rustBin;$linkerBin;$env:PATH"
        $cargoArgs += @('--target','x86_64-pc-windows-gnu')
        $output = Join-Path $BuildRoot 'x86_64-pc-windows-gnu/release/micopi.exe'
    }
    if ($Headless) { $cargoArgs += @('--no-default-features'); $output = $output.Replace('micopi.exe','micopi-cli.exe') }
    # The native plugin is built independently; the host never links its code.
    & $cargo build --release -p micopi-compositing @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Compositing DLL build failed' }
    $compositorOutput = $output.Replace('micopi.exe','micopi_compositing.dll').Replace('micopi-cli.exe','micopi_compositing.dll')
    Copy-Item -LiteralPath $compositorOutput -Destination 'plugins/compositing/micopi_compositing.dll' -Force
    & $cargo build --release -p micopi-painting-tools @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Painting tool DLL build failed' }
    $paintingOutput = $output.Replace('micopi.exe','micopi_painting_tools.dll').Replace('micopi-cli.exe','micopi_painting_tools.dll')
    Copy-Item -LiteralPath $paintingOutput -Destination 'plugins/painting/micopi_painting_tools.dll' -Force
    & $cargo build --release -p micopi-native-paint @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Native plugin build failed' }
    $nativeOutput = $output.Replace('micopi.exe','micopi_native_paint.dll').Replace('micopi-cli.exe','micopi_native_paint.dll')
    Copy-Item -LiteralPath $nativeOutput -Destination 'plugins/native-paint/micopi_native_paint.dll' -Force
    & $cargo build --release -p micopi-ai-assistant @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'AI assistant plugin build failed' }
    $assistantOutput = $output.Replace('micopi.exe','micopi_ai_assistant.dll').Replace('micopi-cli.exe','micopi_ai_assistant.dll')
    Copy-Item -LiteralPath $assistantOutput -Destination 'plugins/ai-assistant/micopi_ai_assistant.dll' -Force
    if ($PluginsOnly) { Get-Item 'plugins/compositing/micopi_compositing.dll','plugins/native-paint/micopi_native_paint.dll','plugins/painting/micopi_painting_tools.dll','plugins/ai-assistant/micopi_ai_assistant.dll' | Select-Object FullName,Length; return }
    if ($Test) {
        & $cargo build --release -p micopi-native-incompatible @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw 'ABI test fixture build failed' }
        $env:MICOPI_TEST_BAD_DLL = (Resolve-Path -LiteralPath ($nativeOutput.Replace('micopi_native_paint.dll','micopi_native_incompatible.dll'))).Path
        & $cargo test @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
        & $cargo test @cargoArgs native_plugin_dll_lifecycle -- --ignored
        if ($LASTEXITCODE -ne 0) { throw 'Native DLL integration test failed' }
    }
    if ($Benchmark) {
        & $cargo test --release @cargoArgs brush_benchmark -- --ignored --nocapture
        if ($LASTEXITCODE -ne 0) { throw 'Benchmark failed' }
    }
    & $cargo build --release @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    $packageVersion = ((Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "([^"]+)"').Matches.Groups[1].Value)
    $suffix = if ($Headless) { '-headless' } else { '' }
    $versionedOutput = "dist/MiCoPi-$packageVersion$suffix.exe"
    try { Copy-Item -LiteralPath $output -Destination $versionedOutput -Force }
    catch {
        $buildNumber = 1
        do { $versionedOutput = "dist/MiCoPi-$packageVersion$suffix-build$buildNumber.exe"; $buildNumber++ } while (Test-Path -LiteralPath $versionedOutput)
        Copy-Item -LiteralPath $output -Destination $versionedOutput
        Write-Warning "Saved $versionedOutput because the usual file is unavailable."
    }
    if (!$Headless) {
        try { Copy-Item -LiteralPath $output -Destination 'dist/MiCoPi.exe' -Force }
        catch { Write-Warning "MiCoPi.exe may be running. New build: $versionedOutput" }
    }
    $cliOutput = if ($Headless) { $output } else { $output.Replace('micopi.exe','micopi-cli.exe') }
    Copy-Item -LiteralPath $cliOutput -Destination 'dist/micopi-cli.exe' -Force
    New-Item -ItemType Directory -Path 'dist/locales' -Force | Out-Null
    foreach ($locale in @('zh-CN','ja','ko')) {
        $catalogPath = Join-Path 'dist/locales' "$locale.json"
        if (!(Test-Path -LiteralPath $catalogPath)) {
            Copy-Item -LiteralPath (Join-Path 'locales' "$locale.json") -Destination $catalogPath
        }
    }
    Copy-Item -LiteralPath $cliOutput -Destination "dist/MiCoPi-CLI-$packageVersion.exe" -Force
    if (!(Test-Path -LiteralPath 'dist/plugins')) { Copy-Item -LiteralPath 'plugins' -Destination 'dist/plugins' -Recurse }
    foreach ($feature in @('canvas','toolbox','color','brush','navigator','layers')) {
        $featureDirectory = Join-Path 'dist/plugins' $feature
        New-Item -ItemType Directory -Path $featureDirectory -Force | Out-Null
        $featureManifest = Join-Path $featureDirectory 'manifest.json'
        if (!(Test-Path -LiteralPath $featureManifest)) {
            $featureConfig = Get-Content -LiteralPath (Join-Path (Join-Path 'plugins' $feature) 'manifest.json') -Raw | ConvertFrom-Json
            if (Test-Path -LiteralPath 'dist/plugins/workbench/manifest.json') {
                $legacyWorkbench = Get-Content -LiteralPath 'dist/plugins/workbench/manifest.json' -Raw | ConvertFrom-Json
                $legacyPanels = @($legacyWorkbench.panels | Where-Object { $_.id -eq $feature -and $_.builtin -eq $feature })
                if ($legacyPanels.Count -gt 0) { $featureConfig.panels = $legacyPanels }
            }
            $featureConfig | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $featureManifest -Encoding utf8
        }
    }
    New-Item -ItemType Directory -Path 'dist/plugins/compositing' -Force | Out-Null
    if (!(Test-Path 'dist/plugins/compositing/manifest.json')) { Copy-Item 'plugins/compositing/manifest.json' 'dist/plugins/compositing/manifest.json' }
    Copy-Item -LiteralPath $compositorOutput -Destination 'dist/plugins/compositing/micopi_compositing.dll' -Force
    $compositorPath='dist/plugins/compositing/manifest.json'
    $compositorConfig=Get-Content $compositorPath -Raw | ConvertFrom-Json
    if (!$compositorConfig.nodes) {
        $defaultConfig=Get-Content 'plugins/compositing/manifest.json' -Raw | ConvertFrom-Json
        $compositorConfig | Add-Member -NotePropertyName nodes -NotePropertyValue $defaultConfig.nodes -Force
        $compositorConfig | ConvertTo-Json -Depth 64 | Set-Content $compositorPath -Encoding utf8
    }
    New-Item -ItemType Directory -Path 'dist/plugins/workflows' -Force | Out-Null
    New-Item -ItemType Directory -Path 'dist/plugins/replay' -Force | Out-Null
    if (!(Test-Path 'dist/plugins/replay/manifest.json')) {Copy-Item 'plugins/replay/manifest.json' 'dist/plugins/replay/manifest.json'}
    if (!(Test-Path 'dist/plugins/workflows/manifest.json')) {Copy-Item 'plugins/workflows/manifest.json' 'dist/plugins/workflows/manifest.json'}
    # Append new system entrypoints while preserving existing custom definitions.
    $corePath = 'dist/plugins/core/manifest.json'
    if (!(Test-Path -LiteralPath $corePath)) {
        New-Item -ItemType Directory -Path 'dist/plugins/core' -Force | Out-Null
        Copy-Item 'plugins/core/manifest.json' $corePath
    }
    $coreConfig = Get-Content -LiteralPath $corePath -Raw | ConvertFrom-Json
    $currentCore = Get-Content 'plugins/core/manifest.json' -Raw | ConvertFrom-Json
    foreach ($action in $currentCore.actions | Where-Object { $_.output_schema }) {
        $existing = $coreConfig.actions | Where-Object { $_.id -eq $action.id }
        if ($existing -and !$existing.output_schema) { $existing | Add-Member -NotePropertyName output_schema -NotePropertyValue $action.output_schema }
        elseif ($existing -and $existing.output_schema.type -eq 'object' -and $action.output_schema.type -eq 'object') {
            # Add new public state ports without replacing customized schemas.
            if (!$existing.output_schema.PSObject.Properties['properties']) {
                $existing.output_schema | Add-Member -NotePropertyName properties -NotePropertyValue ([pscustomobject]@{})
            }
            foreach ($property in $action.output_schema.properties.PSObject.Properties) {
                if (!$existing.output_schema.properties.PSObject.Properties[$property.Name]) {
                    $existing.output_schema.properties | Add-Member -NotePropertyName $property.Name -NotePropertyValue $property.Value
                }
            }
        }
    }
    foreach ($action in $currentCore.actions) {
        if (!($coreConfig.actions | Where-Object { $_.id -eq $action.id })) { $coreConfig.actions = @($coreConfig.actions) + @($action) }
    }
    $coreConfig | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $corePath -Encoding utf8
    foreach ($systemPackage in @('core','workbench','workflows','toolbox')) {
        $installedManifest = Join-Path (Join-Path 'dist/plugins' $systemPackage) 'manifest.json'
        $systemConfig = Get-Content -LiteralPath $installedManifest -Raw | ConvertFrom-Json
        $currentSystem = Get-Content -LiteralPath (Join-Path (Join-Path 'plugins' $systemPackage) 'manifest.json') -Raw | ConvertFrom-Json
        foreach ($field in @('actions','states','panels')) {
            foreach ($contribution in $currentSystem.$field) {
                if (!($systemConfig.$field | Where-Object { $_.id -eq $contribution.id })) {
                    if (!$systemConfig.PSObject.Properties[$field]) { $systemConfig | Add-Member -NotePropertyName $field -NotePropertyValue @() }
                    $systemConfig.$field = @($systemConfig.$field) + @($contribution)
                }
            }
        }
        $systemConfig | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $installedManifest -Encoding utf8
    }
    New-Item -ItemType Directory -Path 'dist/plugins/painting' -Force | Out-Null
    $paintingManifest = 'dist/plugins/painting/manifest.json'
    if (!(Test-Path -LiteralPath $paintingManifest)) { Copy-Item 'plugins/painting/manifest.json' $paintingManifest }
    $paintingConfig = Get-Content -LiteralPath $paintingManifest -Raw | ConvertFrom-Json
    if (!$paintingConfig.library -and $paintingConfig.id -eq 'painting') {
        # Migrate old presets without replacing the user's brush parameters.
        $paintingConfig | Add-Member -NotePropertyName library -NotePropertyValue 'micopi_painting_tools.dll' -Force
        $toolHandlers = @{brush=1;eraser=2;picker=3;hand=4;zoom=5}
        foreach ($tool in $paintingConfig.tools) {
            if (!$tool.native -and $toolHandlers.ContainsKey($tool.kind)) { $tool | Add-Member -NotePropertyName native -NotePropertyValue $toolHandlers[$tool.kind] -Force }
        }
        $paintingConfig | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $paintingManifest -Encoding utf8
    }
    Copy-Item -LiteralPath $paintingOutput -Destination 'dist/plugins/painting/micopi_painting_tools.dll' -Force
    New-Item -ItemType Directory -Path 'dist/plugins/native-paint' -Force | Out-Null
    if (!(Test-Path -LiteralPath 'dist/plugins/native-paint/manifest.json')) { Copy-Item -LiteralPath 'plugins/native-paint/manifest.json' -Destination 'dist/plugins/native-paint/manifest.json' }
    Copy-Item -LiteralPath $nativeOutput -Destination 'dist/plugins/native-paint/micopi_native_paint.dll' -Force
    New-Item -ItemType Directory -Path 'dist/plugins/ai-assistant' -Force | Out-Null
    if (!(Test-Path 'dist/plugins/ai-assistant/manifest.json')) { Copy-Item 'plugins/ai-assistant/manifest.json' 'dist/plugins/ai-assistant/manifest.json' }
    Copy-Item -LiteralPath $assistantOutput -Destination 'dist/plugins/ai-assistant/micopi_ai_assistant.dll' -Force
    New-Item -ItemType Directory -Path 'dist/licenses' -Force | Out-Null
    Copy-Item -LiteralPath 'assets/material-symbols/LICENSE' -Destination 'dist/licenses/MaterialSymbols-LICENSE.txt' -Force
    (Get-Content -LiteralPath 'assets/material-symbols/NOTICE.md' -Raw).Replace('](LICENSE)','](MaterialSymbols-LICENSE.txt)') | Set-Content -LiteralPath 'dist/licenses/MaterialSymbols-NOTICE.md' -Encoding utf8
    Copy-Item -LiteralPath 'LICENSE' -Destination 'dist/licenses/MiCoPi-LICENSE.txt' -Force
    $metadata = & $cargo metadata --format-version 1 --offline --locked --filter-platform x86_64-pc-windows-gnu | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Dependency notice generation failed' }
    $notices = @('# MiCoPi dependency licenses', '', 'Generated from Cargo.lock. Each subdirectory contains the upstream license files.', '')
    foreach ($package in $metadata.packages | Where-Object { $_.name -ne 'micopi' }) {
        $notices += "- $($package.name) $($package.version): $($package.license) - $($package.repository)"
        $source = Split-Path -Parent $package.manifest_path
        $licenseFiles = Get-ChildItem -LiteralPath $source -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)' }
        if ($licenseFiles) {
            $dest = Join-Path 'dist/licenses' "$($package.name)-$($package.version)"
            New-Item -ItemType Directory -Path $dest -Force | Out-Null
            foreach ($file in $licenseFiles) { Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $dest $file.Name) -Force }
        }
    }
    $notices | Set-Content -LiteralPath 'dist/licenses/DEPENDENCIES.md' -Encoding utf8
    Get-Item $versionedOutput | Select-Object FullName,Length
} finally {
    foreach ($key in $savedEnvironment.Keys) { [Environment]::SetEnvironmentVariable($key, $savedEnvironment[$key]) }
    Pop-Location
}
