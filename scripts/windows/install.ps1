# Installs Amiscara Deck for the current user: copies the programs to
# %LOCALAPPDATA%\Programs\Amiscara Deck, installs the hooks in Claude Code and
# starts the daemon now and at every login.
#
#   powershell -ExecutionPolicy Bypass -File install.ps1
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Uninstall
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'

$bin = Join-Path $env:LOCALAPPDATA 'Programs\Amiscara Deck'
$cli = Join-Path $bin 'amiscara-deck.exe'
$daemon = Join-Path $bin 'amiscara-deckd.exe'

if ($Uninstall) {
    Get-Process amiscara-deckd -ErrorAction SilentlyContinue | Stop-Process
    if (Test-Path $cli) {
        & $cli daemon uninstall
        & $cli hooks uninstall
    }
    Write-Output "Programs remain in $bin."
    exit 0
}

New-Item -ItemType Directory -Force -Path $bin | Out-Null
Get-Process amiscara-deckd -ErrorAction SilentlyContinue | Stop-Process
foreach ($p in 'amiscara-deckd.exe', 'deck-hook.exe', 'amiscara-deck.exe') {
    Copy-Item -Force (Join-Path $PSScriptRoot $p) (Join-Path $bin $p)
}
Write-Output "Programs in $bin"
& $cli hooks install
& $cli daemon install
if ($LASTEXITCODE -ne 0) {
    Start-Process -FilePath $daemon -WindowStyle Hidden
    Write-Output 'Start at login failed; the daemon runs until you log off.'
}
& $cli daemon status
