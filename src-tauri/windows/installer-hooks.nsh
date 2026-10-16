; Extra steps for the NSIS installer (tauri.conf.json: bundle.windows.nsis.installerHooks).

; "Delete the application data" in the uninstaller only removes Tauri's own
; folders (named after the bundle id). Transmute keeps its settings, presets
; and run state under its product name, so remove those too, but only when
; the box is ticked and this isn't an update.
;
; Only this version's files go (and their .v1 / .corrupt-* backups): the same
; folder may hold Transmute 1.x's config.json and Profiles, which stay. Each
; folder is removed only if that leaves it empty. The program folder may be
; %LOCALAPPDATA%\Transmute itself, so only its "state" subfolder goes.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    Delete "$APPDATA\${PRODUCTNAME}\settings*.json"
    Delete "$APPDATA\${PRODUCTNAME}\defaults*.json"
    Delete "$APPDATA\${PRODUCTNAME}\session*.json"
    RmDir /r "$APPDATA\${PRODUCTNAME}\presets"
    RmDir "$APPDATA\${PRODUCTNAME}"
    RmDir /r "$LOCALAPPDATA\${PRODUCTNAME}\state"
    RmDir "$LOCALAPPDATA\${PRODUCTNAME}"
  ${EndIf}
!macroend
