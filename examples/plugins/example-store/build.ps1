# Builds the plugin for wasm32-wasip2 and puts the component next to plugin.json as provider.wasm.
# Needs the target once: rustup target add wasm32-wasip2
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    cargo build --release --target wasm32-wasip2
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot "target" }
    Copy-Item (Join-Path $targetDir "wasm32-wasip2/release/example_store.wasm") (Join-Path $PSScriptRoot "provider.wasm") -Force
    Write-Host "Built provider.wasm"
}
finally {
    Pop-Location
}
