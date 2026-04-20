!macro NSIS_HOOK_PREUNINSTALL
  ; Ensure the bundled proxy sidecar is not locking files under $INSTDIR\resources.
  !if "${INSTALLMODE}" == "currentUser"
    nsis_tauri_utils::FindProcessCurrentUser "cli-proxy-api-plus.exe"
  !else
    nsis_tauri_utils::FindProcess "cli-proxy-api-plus.exe"
  !endif
  Pop $R0

  ${If} $R0 = 0
    !if "${INSTALLMODE}" == "currentUser"
      nsis_tauri_utils::KillProcessCurrentUser "cli-proxy-api-plus.exe"
    !else
      nsis_tauri_utils::KillProcess "cli-proxy-api-plus.exe"
    !endif
    Pop $R0
    Sleep 500
  ${EndIf}
!macroend
