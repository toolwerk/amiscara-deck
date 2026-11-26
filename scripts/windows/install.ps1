# Installs Amiscara Deck for the current user: copies the programs to
# %LOCALAPPDATA%\Programs\Amiscara Deck, installs the hooks in Claude Code and
# starts the daemon now and at every login.
#
#   powershell -ExecutionPolicy Bypass -File install.ps1
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Wsl [-Distro Ubuntu]
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Uninstall [-Wsl]
#
# -Wsl also installs the hooks for Claude Code in WSL (the default
# distribution, or -Distro). They call deck-hook.exe of Windows through
# /mnt/c, which reaches the daemon on Windows.
param([switch]$Uninstall, [switch]$Wsl, [string]$Distro)
$ErrorActionPreference = 'Stop'

$bin = Join-Path $env:LOCALAPPDATA 'Programs\Amiscara Deck'
$cli = Join-Path $bin 'amiscara-deck.exe'
$daemon = Join-Path $bin 'amiscara-deckd.exe'

# The settings file of Claude Code in WSL as a Windows path, and the hook
# program of Windows as a path in WSL. `--exec` keeps a shell away from the
# backslashes.
function Wsl-Paths {
    $wslArgs = @()
    if ($Distro) { $wslArgs = @('-d', $Distro) }
    $settings = & wsl.exe @wslArgs --exec sh -c 'mkdir -p "$HOME/.claude" && wslpath -w "$HOME/.claude/settings.json"'
    if ($LASTEXITCODE -ne 0 -or -not $settings) { throw 'WSL not reachable (wsl.exe).' }
    $hook = & wsl.exe @wslArgs --exec wslpath -u (Join-Path $bin 'deck-hook.exe')
    if ($LASTEXITCODE -ne 0 -or -not $hook) { throw 'wslpath failed.' }
    return @(($settings | Select-Object -First 1).Trim(), ($hook | Select-Object -First 1).Trim())
}

if ($Uninstall) {
    if (Test-Path $cli) {
        if ($Wsl) {
            $settings, $hook = Wsl-Paths
            & $cli hooks uninstall --settings $settings --program $hook
        }
        & $cli daemon uninstall
        & $cli hooks uninstall
    }
    Get-Process amiscara-deckd -ErrorAction SilentlyContinue | Stop-Process
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
if ($Wsl) {
    $settings, $hook = Wsl-Paths
    # --force: the path is one of WSL, Windows cannot check it.
    & $cli hooks install --settings $settings --program $hook --force
}
& $cli daemon status
