Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
Name "RustifiedYoutube"
OutFile "${OUTPUT_FILE}"
; Resolve defaults in .onInit so a caller's /D= always wins, even when it
; explicitly selects the normal new-install directory during legacy migration.
InstallDir ""
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey "ProductName" "RustifiedYoutube"
VIAddVersionKey "FileDescription" "RustifiedYoutube native desktop client installer"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "LegalCopyright" "RustifiedYoutube contributors"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\RustifiedYoutube.exe"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "RustifiedYoutube requires 64-bit Windows."
    Abort
  ${EndIf}
  ${If} $INSTDIR == ""
    ReadRegStr $INSTDIR HKCU "Software\RustifiedYoutube" "InstallDir"
    ${If} $INSTDIR == ""
      IfFileExists "$LOCALAPPDATA\RustifiedYoutube\rustified-youtube.exe" 0 new_install_directory
        StrCpy $INSTDIR "$LOCALAPPDATA\RustifiedYoutube"
        Goto install_directory_ready
      new_install_directory:
        StrCpy $INSTDIR "$LOCALAPPDATA\Programs\RustifiedYoutube"
      install_directory_ready:
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "RustifiedYoutube" SEC_APP
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${STAGE_DIR}\RustifiedYoutube.exe"
  ; Old shortcuts and Windows session restoration may still use this name.
  ; Both names launch the same native client; never retain the web wrapper.
  File /oname=rustified-youtube.exe "${STAGE_DIR}\RustifiedYoutube.exe"
  File "${STAGE_DIR}\rustified-auth.exe"
  File "${STAGE_DIR}\RUST-DEPENDENCIES.txt"
  File "${STAGE_DIR}\THIRD-PARTY-NOTICES.md"
  File "${STAGE_DIR}\README.md"
  File "${STAGE_DIR}\LICENSE"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\RustifiedYoutube.lnk" "$INSTDIR\RustifiedYoutube.exe"
  CreateShortcut "$DESKTOP\RustifiedYoutube.lnk" "$INSTDIR\RustifiedYoutube.exe"
  IfFileExists "$SMPROGRAMS\RustifiedYoutube\RustifiedYoutube.lnk" 0 legacy_shortcut_ready
    CreateShortcut "$SMPROGRAMS\RustifiedYoutube\RustifiedYoutube.lnk" "$INSTDIR\RustifiedYoutube.exe"
  legacy_shortcut_ready:
  WriteRegStr HKCU "Software\RustifiedYoutube" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "DisplayName" "RustifiedYoutube"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "MainBinaryName" "RustifiedYoutube.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "DisplayIcon" "$INSTDIR\RustifiedYoutube.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$SMPROGRAMS\RustifiedYoutube.lnk"
  Delete "$DESKTOP\RustifiedYoutube.lnk"
  Delete "$SMPROGRAMS\RustifiedYoutube\RustifiedYoutube.lnk"
  RMDir "$SMPROGRAMS\RustifiedYoutube"
  Delete "$INSTDIR\RustifiedYoutube.exe"
  Delete "$INSTDIR\rustified-youtube.exe"
  Delete "$INSTDIR\rustified-auth.exe"
  Delete "$INSTDIR\yt-dlp.exe"
  Delete "$INSTDIR\node.exe"
  Delete "$INSTDIR\ffmpeg.exe"
  Delete "$INSTDIR\FFMPEG-LICENSE.txt"
  Delete "$INSTDIR\NODE-LICENSE.txt"
  Delete "$INSTDIR\YT-DLP-LICENSE.txt"
  Delete "$INSTDIR\YT-DLP-THIRD-PARTY-LICENSES.txt"
  Delete "$INSTDIR\RUST-DEPENDENCIES.txt"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.md"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\RustifiedYoutube"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustifiedYoutube"
SectionEnd
