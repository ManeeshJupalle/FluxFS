; FluxFS Windows installer (NSIS)
; Build from repo root after: cargo build --release --bins
;   makensis packaging/windows/installer.nsi

!include "MUI2.nsh"

!define PRODUCT_NAME "FluxFS"
!define PRODUCT_VERSION "0.2.0"
!define PRODUCT_PUBLISHER "Maneesh Jupalle"

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "..\..\dist\FluxFS-${PRODUCT_VERSION}-windows-x86_64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\FluxFS"
RequestExecutionLevel user
Unicode true

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath $INSTDIR
  File "..\..\target\release\flux.exe"
  File "..\..\target\release\fluxfs-tray.exe"
  File "..\..\target\release\fluxfs-settings.exe"
  CopyFiles /SILENT "$INSTDIR\flux.exe" "$INSTDIR\fluxfs.exe"
  File "add-to-path.ps1"
  File "remove-from-path.ps1"

  ; Add install directory to user PATH
  ExecWait '"powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\add-to-path.ps1" -InstallDir "$INSTDIR"'

  ; Post-install: scan Downloads + register auto-start
  ExecWait '"$INSTDIR\flux.exe" setup --quiet' $2

  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
  ExecWait '"$INSTDIR\flux.exe" uninstall-service' $0

  ExecWait '"powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\remove-from-path.ps1" -InstallDir "$INSTDIR"'

  Delete "$INSTDIR\add-to-path.ps1"
  Delete "$INSTDIR\remove-from-path.ps1"
  Delete "$INSTDIR\flux.exe"
  Delete "$INSTDIR\fluxfs.exe"
  Delete "$INSTDIR\fluxfs-tray.exe"
  Delete "$INSTDIR\fluxfs-settings.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
