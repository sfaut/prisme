<#
.SYNOPSIS
    Stops the running Prisme, compiles the interface and the backend, then starts Prisme again.

.DESCRIPTION
    Shortcut for checking a change in the compiled application (http://127.0.0.1:8484):
    stops every running prisme.exe (its executable is locked while it runs), runs
    `npm run build` in web/, `cargo build`, then starts the new executable in this
    terminal (Ctrl+C stops it). Stops at the first step that fails.

    -Verbose, the common parameter, also starts Prisme with --verbose: its log then
    shows each request to the API and each message of the query sessions.

.PARAMETER Release
    Builds and starts the release executable (target/release) instead of the debug one.

.PARAMETER PrismeArgs
    Options passed to Prisme, for example `-- --data-dir ./tmp`.

.EXAMPLE
    ./rebuild.ps1
    ./rebuild.ps1 -Release
    ./rebuild.ps1 -Verbose
    ./rebuild.ps1 -- --data-dir ./tmp
#>
param(
    [switch] $Release,
    [Parameter(ValueFromRemainingArguments)]
    [string[]] $PrismeArgs = @()
)

# No global $ErrorActionPreference = 'Stop': Windows PowerShell would turn the
# progress messages cargo writes to stderr into errors. Native commands are
# checked through their exit code instead.
Set-Location $PSScriptRoot -ErrorAction Stop

# Fails the script when the last native command (npm, cargo) failed.
function Assert-Success([string] $step) {
    if ($LASTEXITCODE -ne 0) {
        throw "$step failed (exit code $LASTEXITCODE)"
    }
}

$running = Get-Process -Name prisme -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "Stopping Prisme (PID $($running.Id -join ', '))"
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}

Write-Host 'Compiling the interface'
Push-Location web
try {
    npm run build
    Assert-Success 'npm run build'
} finally {
    Pop-Location
}

$profile_dir = if ($Release) { 'release' } else { 'debug' }
Write-Host "Compiling the backend ($profile_dir)"
if ($Release) { cargo build --release } else { cargo build }
Assert-Success 'cargo build'

# The [Parameter()] attribute makes this script an advanced one: -Verbose is its
# common parameter.
if ($PSBoundParameters['Verbose']) {
    $PrismeArgs = @('--verbose') + $PrismeArgs
}

Write-Host 'Starting Prisme'
& "target/$profile_dir/prisme.exe" @PrismeArgs
