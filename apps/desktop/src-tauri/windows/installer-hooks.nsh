!macro NSIS_HOOK_PREUNINSTALL
  IfFileExists "$INSTDIR\fabdev-windows-helper.exe" 0 +2
  ExecWait '"$INSTDIR\fabdev-windows-helper.exe" sync-proxy-hosts'
  IfFileExists "$INSTDIR\fabdev-windows-helper.exe" 0 +2
  ExecWait '"$INSTDIR\fabdev-windows-helper.exe" sync-hosts'
  IfFileExists "$INSTDIR\fabdev-windows-helper.exe" 0 +3
  IfFileExists "$LOCALAPPDATA\FabDev\config\tls\ca.crt" 0 +2
  ExecWait '"$INSTDIR\fabdev-windows-helper.exe" untrust-ca --certificate "$LOCALAPPDATA\FabDev\config\tls\ca.crt"'
!macroend
