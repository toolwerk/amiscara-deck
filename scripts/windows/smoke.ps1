# Smoke test of the Windows build, for CI: daemon on its pipe, the command
# line reaches it, the hook program hands an event over and stays silent
# without a daemon. Uses its own data folder.
$ErrorActionPreference = 'Stop'
$bin = if ($args.Count -gt 0) { $args[0] } else { 'target\debug' }
$tmp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$env:DECK_DATA_DIR = Join-Path $tmp 'deck-smoke'
Remove-Item -Recurse -Force $env:DECK_DATA_DIR -ErrorAction SilentlyContinue

function Fail($msg) { Write-Error $msg; exit 1 }

# No daemon: the hook program prints nothing and ends with 0.
$request = '{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"C:\\x","tool_name":"Bash","tool_input":{"command":"ls"}}'
$out = $request | & "$bin\deck-hook.exe"
if ($LASTEXITCODE -ne 0 -or $out) { Fail "hook without daemon: exit $LASTEXITCODE, output '$out'" }

$daemon = Start-Process -FilePath "$bin\amiscara-deckd.exe" -PassThru -NoNewWindow
Start-Sleep -Seconds 2
try {
    $list = & "$bin\amiscara-deck.exe" list
    if ($LASTEXITCODE -ne 0) { Fail "list: exit $LASTEXITCODE" }
    Write-Output "list: $list"

    # A second daemon on the same folder refuses to start.
    $second = Start-Process -FilePath "$bin\amiscara-deckd.exe" -PassThru -NoNewWindow -Wait
    if ($second.ExitCode -eq 0) { Fail 'a second daemon started' }

    $start = '{"session_id":"s1","hook_event_name":"SessionStart","cwd":"C:\\x","source":"startup"}'
    $out = $start | & "$bin\deck-hook.exe"
    if ($LASTEXITCODE -ne 0 -or $out) { Fail "hook SessionStart: exit $LASTEXITCODE, output '$out'" }
    # No UI is connected: a permission request is not held, no output.
    $out = $request | & "$bin\deck-hook.exe"
    if ($LASTEXITCODE -ne 0 -or $out) { Fail "hook PermissionRequest: exit $LASTEXITCODE, output '$out'" }
    Start-Sleep -Milliseconds 500
    $json = & "$bin\amiscara-deck.exe" list --json | ConvertFrom-Json
    if (@($json).Count -lt 1) { Fail "the agent did not arrive: $json" }
} finally {
    Stop-Process -Id $daemon.Id -ErrorAction SilentlyContinue
}

# Start at login: the Run key of the user, the daemon starts at once and
# ends again with uninstall.
Start-Sleep -Seconds 1
$daemonArgs = @('--program', (Resolve-Path "$bin\amiscara-deckd.exe").Path, '--data-dir', $env:DECK_DATA_DIR)
& "$bin\amiscara-deck.exe" daemon status @daemonArgs | Out-Null
if ($LASTEXITCODE -ne 3) { Fail "daemon status before install: exit $LASTEXITCODE" }
& "$bin\amiscara-deck.exe" daemon install @daemonArgs
if ($LASTEXITCODE -ne 0) { Fail "daemon install: exit $LASTEXITCODE" }
$value = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run').'Amiscara Deck'
if (-not $value -or -not $value.Contains('--data-dir')) { Fail "Run key: '$value'" }
& "$bin\amiscara-deck.exe" daemon status @daemonArgs
if ($LASTEXITCODE -ne 0) { Fail "daemon status after install: exit $LASTEXITCODE" }
& "$bin\amiscara-deck.exe" list | Out-Null
if ($LASTEXITCODE -ne 0) { Fail "the daemon from install does not answer" }
if (-not (Test-Path (Join-Path $env:DECK_DATA_DIR 'deckd.log'))) { Fail 'no deckd.log' }
& "$bin\amiscara-deck.exe" daemon uninstall @daemonArgs
if ($LASTEXITCODE -ne 0) { Fail "daemon uninstall: exit $LASTEXITCODE" }
Start-Sleep -Seconds 1
$value = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).'Amiscara Deck'
if ($value) { Fail "Run key still there: '$value'" }
if (Get-Process amiscara-deckd -ErrorAction SilentlyContinue) { Fail 'the daemon still runs after uninstall' }
Write-Output 'smoke test passed'

