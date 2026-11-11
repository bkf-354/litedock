; Installer LiteDock (NSIS 3, per-user: tanpa hak admin)
Unicode true
!include "MUI2.nsh"

!define APP "LiteDock"
!define VER "1.9.0"
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiteDock"

Name "${APP}"
OutFile "LiteDock-Setup-${VER}.exe"
InstallDir "$LOCALAPPDATA\Programs\LiteDock"
InstallDirRegKey HKCU "Software\LiteDock" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "LiteDock ${VER}"

VIProductVersion "${VER}.0"
VIAddVersionKey "ProductName" "LiteDock"
VIAddVersionKey "FileDescription" "LiteDock - dock ringan untuk Windows 11"
VIAddVersionKey "FileVersion" "${VER}"
VIAddVersionKey "LegalCopyright" "LiteDock"

!define MUI_ICON "..\res\icon.ico"
!define MUI_UNICON "..\res\icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\LiteDock.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Jalankan LiteDock sekarang"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Indonesian"
!insertmacro MUI_LANGUAGE "English"

Section "LiteDock (wajib)" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  ; bila sudah berjalan: tutup dengan rapi (taskbar dipulihkan dulu)
  IfFileExists "$INSTDIR\LiteDock.exe" 0 +2
    nsExec::Exec '"$INSTDIR\LiteDock.exe" --restore'
  File "LiteDock.exe"
  File "..\README.md"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\LiteDock" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNKEY}" "DisplayName" "LiteDock"
  WriteRegStr HKCU "${UNKEY}" "DisplayVersion" "${VER}"
  WriteRegStr HKCU "${UNKEY}" "Publisher" "LiteDock"
  WriteRegStr HKCU "${UNKEY}" "DisplayIcon" "$INSTDIR\LiteDock.exe"
  WriteRegStr HKCU "${UNKEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${UNKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNKEY}" "NoRepair" 1
  CreateDirectory "$SMPROGRAMS\LiteDock"
  CreateShortcut "$SMPROGRAMS\LiteDock\LiteDock.lnk" "$INSTDIR\LiteDock.exe"
  CreateShortcut "$SMPROGRAMS\LiteDock\Pulihkan Taskbar Windows.lnk" "$INSTDIR\LiteDock.exe" "--restore"
  CreateShortcut "$SMPROGRAMS\LiteDock\Uninstall LiteDock.lnk" "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Jalankan otomatis saat Windows mulai" SecAuto
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "LiteDock" '"$INSTDIR\LiteDock.exe"'
SectionEnd

LangString DESC_Main ${LANG_INDONESIAN} "Program LiteDock."
LangString DESC_Main ${LANG_ENGLISH} "LiteDock program."
LangString DESC_Auto ${LANG_INDONESIAN} "Dock langsung muncul setiap kali Anda masuk ke Windows."
LangString DESC_Auto ${LANG_ENGLISH} "Start the dock every time you sign in."
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} $(DESC_Main)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAuto} $(DESC_Auto)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  ; tutup LiteDock dan pulihkan taskbar Windows sebelum menghapus
  nsExec::Exec '"$INSTDIR\LiteDock.exe" --restore'
  Delete "$INSTDIR\LiteDock.exe"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\LiteDock\LiteDock.lnk"
  Delete "$SMPROGRAMS\LiteDock\Pulihkan Taskbar Windows.lnk"
  Delete "$SMPROGRAMS\LiteDock\Uninstall LiteDock.lnk"
  RMDir "$SMPROGRAMS\LiteDock"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "LiteDock"
  DeleteRegKey HKCU "${UNKEY}"
  DeleteRegKey HKCU "Software\LiteDock"
  MessageBox MB_YESNO|MB_ICONQUESTION "Hapus juga pengaturan LiteDock (dock, aplikasi tersemat, lokasi salat)?" IDNO +2
    RMDir /r "$APPDATA\LiteDock"
SectionEnd
