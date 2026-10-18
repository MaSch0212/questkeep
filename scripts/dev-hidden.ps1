# Runs `tauri dev` without a visible window, for driving the app through the WebView2 DevTools
# protocol (http://127.0.0.1:9222/json) while someone else uses the machine.
#  - window stays hidden and off the taskbar (QUESTKEEP_HIDDEN=1, debug builds only)
#  - background throttling disabled so the hidden page keeps rendering
#  - this shell and everything it starts (cargo, rustc, node, the app) run at below-normal priority
#    with a limited number of parallel build jobs
param(
  [int]$Port = 9222,
  [int]$Jobs = [Math]::Max(1, [Math]::Floor([Environment]::ProcessorCount / 3)),
  [switch]$Portable
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

(Get-Process -Id $PID).PriorityClass = "BelowNormal"
$env:CARGO_BUILD_JOBS = "$Jobs"
$env:QUESTKEEP_HIDDEN = "1"
# Own WebView2 profile: a running installed Questkeep uses the default one with different browser
# arguments, and WebView2 refuses to share a profile across differing arguments (0x8007139F).
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $root "target\debug\webview2-hidden"
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = @(
  "--remote-debugging-port=$Port",
  "--disable-background-timer-throttling",
  "--disable-renderer-backgrounding",
  "--disable-backgrounding-occluded-windows"
) -join " "

# Optional throwaway data dir under target/debug/data instead of the real library.
$marker = Join-Path $root "target\debug\portable.txt"
if ($Portable) {
  New-Item -ItemType Directory -Force (Split-Path $marker) | Out-Null
  New-Item -ItemType File -Force $marker | Out-Null
} elseif (Test-Path $marker) {
  Remove-Item $marker
}

Push-Location $root
try {
  pnpm tauri dev
} finally {
  Pop-Location
}
