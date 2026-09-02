!define FABDEV_VC_RUNTIME_URL "https://aka.ms/vc14/vc_redist.x64.exe"

LangString FabDevVcRuntimeRequired ${LANG_ENGLISH} "fabDev requires the Microsoft Visual C++ Redistributable (x64).$\r$\n$\r$\nDownload it from Microsoft now? fabDev Setup will close. Install the Microsoft package, then run this installer again."
LangString FabDevVcRuntimeRequired ${LANG_TRADCHINESE} "fabDev 需要 Microsoft Visual C++ 可轉散發套件（x64）。$\r$\n$\r$\n是否立即前往 Microsoft 官方下載？fabDev 安裝程序將會關閉。完成 Microsoft 套件安裝後，請重新執行本安裝檔。"
LangString FabDevVcRuntimeRequired ${LANG_SIMPCHINESE} "fabDev 需要 Microsoft Visual C++ 可再发行组件（x64）。$\r$\n$\r$\n是否立即前往 Microsoft 官方下载？fabDev 安装程序将会关闭。完成 Microsoft 组件安装后，请重新运行本安装程序。"
LangString FabDevVcRuntimeOpenFailed ${LANG_ENGLISH} "Unable to open the Microsoft download page. Download and install the x64 package from:$\r$\n${FABDEV_VC_RUNTIME_URL}"
LangString FabDevVcRuntimeOpenFailed ${LANG_TRADCHINESE} "無法開啟 Microsoft 下載頁面。請從下列網址下載並安裝 x64 套件：$\r$\n${FABDEV_VC_RUNTIME_URL}"
LangString FabDevVcRuntimeOpenFailed ${LANG_SIMPCHINESE} "无法打开 Microsoft 下载页面。请从以下网址下载并安装 x64 组件：$\r$\n${FABDEV_VC_RUNTIME_URL}"

!macro NSIS_HOOK_PREINSTALL
  Push $0
  ClearErrors
  SetRegView 32
  ReadRegDWORD $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
  ${If} $0 != 1
    ClearErrors
    SetRegView 64
    ReadRegDWORD $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
    SetRegView 32
  ${EndIf}

  ${If} $0 = 1
    IfFileExists "$WINDIR\Sysnative\VCRUNTIME140.dll" fabdev_vc_runtime_ready 0
    IfFileExists "$WINDIR\System32\VCRUNTIME140.dll" fabdev_vc_runtime_ready 0
  ${EndIf}

  ${If} ${Silent}
    DetailPrint "$(FabDevVcRuntimeRequired)"
    Pop $0
    Abort "$(FabDevVcRuntimeRequired)"
  ${EndIf}

  MessageBox MB_ICONEXCLAMATION|MB_YESNO|MB_DEFBUTTON1 "$(FabDevVcRuntimeRequired)" IDYES fabdev_vc_runtime_download IDNO fabdev_vc_runtime_abort

  fabdev_vc_runtime_download:
    ClearErrors
    ExecShell "open" "${FABDEV_VC_RUNTIME_URL}"
    IfErrors 0 fabdev_vc_runtime_abort
    MessageBox MB_ICONSTOP|MB_OK "$(FabDevVcRuntimeOpenFailed)"

  fabdev_vc_runtime_abort:
    Pop $0
    Abort

  fabdev_vc_runtime_ready:
    SetRegView 32
    Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  IfFileExists "$INSTDIR\fabdev-windows-helper.exe" 0 +2
  ExecWait '"$INSTDIR\fabdev-windows-helper.exe" sync-proxy-hosts'
  IfFileExists "$INSTDIR\fabdev-windows-helper.exe" 0 +2
  ExecWait '"$INSTDIR\fabdev-windows-helper.exe" sync-hosts'
  IfFileExists "$INSTDIR\fabdev-windows-helper.exe" 0 +3
  IfFileExists "$LOCALAPPDATA\FabDev\config\tls\ca.crt" 0 +2
  ExecWait '"$INSTDIR\fabdev-windows-helper.exe" untrust-ca --certificate "$LOCALAPPDATA\FabDev\config\tls\ca.crt"'
!macroend
