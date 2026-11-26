# Smoke test of the Windows build, for CI: daemon on its pipe, the command
# line reaches it, the hook program hands an event over and stays silent
# without a daemon. Uses its own data folder.
$ErrorActionPreference = 'Stop'
$bin = if ($args.Count -gt 0) { $args[0] } else { 'target\debug' }
$env:DECK_DATA_DIR = Join-Path $env:RUNNER_TEMP 'deck-smoke'
if (-not $env:RUNNER_TEMP) { $env:DECK_DATA_DIR = Join-Path $env:TEMP 'deck-smoke' }
Remove-Item -Recurse -Force $env:DECK_DATA_DIR -ErrorAction SilentlyContinue

function Fail($msg) { Write-Error $msg; exit 1 }

# No daemon: the hook program prints nothing and ends with 0.
$event = '{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"C:\\x","tool_name":"Bash","tool_input":{"command":"ls"}}'
$out = $event | & "$bin\deck-hook.exe"
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
    $out = $event | & "$bin\deck-hook.exe"
    if ($LASTEXITCODE -ne 0 -or $out) { Fail "hook PermissionRequest: exit $LASTEXITCODE, output '$out'" }
    Start-Sleep -Milliseconds 500
    $json = & "$bin\amiscara-deck.exe" list --json | ConvertFrom-Json
    if (@($json).Count -lt 1) { Fail "the agent did not arrive: $json" }
    Write-Output 'smoke test passed'
} finally {
    Stop-Process -Id $daemon.Id -ErrorAction SilentlyContinue
}
