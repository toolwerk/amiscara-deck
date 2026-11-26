# Smoke test for Claude Code in WSL, for CI: install.ps1 -Wsl puts the hooks
# into the settings of the WSL distribution; the command written there, run
# in WSL, hands an event to the daemon on Windows.
#   wsl-smoke.ps1 <folder with the three programs>
$ErrorActionPreference = 'Stop'
$bin = $args[0]
function Fail($msg) { Write-Error $msg; exit 1 }

$package = Join-Path $env:RUNNER_TEMP 'deck-package'
New-Item -ItemType Directory -Force $package | Out-Null
Copy-Item "$bin\amiscara-deckd.exe", "$bin\deck-hook.exe", "$bin\amiscara-deck.exe", 'scripts\windows\install.ps1' $package
& pwsh -File (Join-Path $package 'install.ps1') -Wsl
if ($LASTEXITCODE -ne 0) { Fail "install.ps1 -Wsl: exit $LASTEXITCODE" }

$cli = Join-Path $env:LOCALAPPDATA 'Programs\Amiscara Deck\amiscara-deck.exe'
$whoami = (& wsl.exe --exec sh -c 'wslpath -w "$HOME/.claude/settings.json"' | Select-Object -First 1).Trim()
Write-Output "WSL settings: $whoami"
$settings = Get-Content -Raw $whoami | ConvertFrom-Json
$command = $settings.hooks.SessionStart[0].hooks[0].command
Write-Output "command in WSL: $command"
if (-not $command.Contains('/mnt/')) { Fail "the hook does not point to /mnt: $command" }

# Run the command as Claude Code in WSL would, with the event on stdin.
$request = '{"session_id":"wsl1","hook_event_name":"SessionStart","cwd":"/home/x/project","source":"startup"}'
$out = $request | & wsl.exe --exec bash -c $command
if ($LASTEXITCODE -ne 0 -or $out) { Fail "hook in WSL: exit $LASTEXITCODE, output '$out'" }
Start-Sleep -Seconds 1
$agents = & $cli list --json | ConvertFrom-Json
if (-not (@($agents) | Where-Object { $_.id -eq 'wsl1' })) { Fail "the agent from WSL did not arrive: $(& $cli list --json)" }
Write-Output 'WSL smoke test passed'

& pwsh -File (Join-Path $package 'install.ps1') -Uninstall -Wsl
if ($LASTEXITCODE -ne 0) { Fail "uninstall: exit $LASTEXITCODE" }
$left = Get-Content -Raw $whoami
if ($left.Contains('deck-hook')) { Fail 'hooks left in WSL settings' }
