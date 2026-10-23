; Coordinate only the existing installation. Never delete business data or use
; process-name-wide taskkill: different worktrees use different executable paths.
!macro CousashPrepareInstallation
  ${If} ${FileExists} "$INSTDIR\codex-usage-dashboard.exe"
    nsExec::ExecToStack '"$INSTDIR\codex-usage-dashboard.exe" prepare-install --json'
    Pop $R0
    Pop $R1
    ${If} $R0 != 0
      Abort "Could not stop this installation safely. Quit Codex Usage Dashboard and retry."
    ${EndIf}
    !insertmacro CheckIfAppIsRunning "$INSTDIR\codex-usage-dashboard.exe" "${PRODUCTNAME}"
  ${EndIf}
!macroend
!macro NSIS_HOOK_PREINSTALL
  !insertmacro CousashPrepareInstallation
!macroend
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CousashPrepareInstallation
!macroend
