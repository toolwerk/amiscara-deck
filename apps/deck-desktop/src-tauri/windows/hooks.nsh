; The daemon runs from the install folder: it stops before its program is
; replaced or removed. The interface sets it up again at its next start.
!macro NSIS_HOOK_PREINSTALL
  ${If} ${FileExists} "$INSTDIR\amiscara-deck.exe"
    nsExec::Exec '"$INSTDIR\amiscara-deck.exe" daemon uninstall'
  ${EndIf}
!macroend

; Removing for good: Claude Code no longer calls the hook program. An update
; keeps the hooks, the path stays the same.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$INSTDIR\amiscara-deck.exe" daemon uninstall'
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\amiscara-deck.exe" hooks uninstall'
  ${EndIf}
!macroend
