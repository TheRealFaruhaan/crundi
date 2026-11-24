; The native client's Windows installer: per-user, one click, as Electron's
; (electron-builder NSIS, oneClick, perMachine false) so it installs over the
; Electron app in place and takes its uninstall entry (package/CONTRACT.md,
; "Identity"). build/installer.nsh is the reference for the custom steps: the
; DisconnectRDP task and the crundi skill.
;
;   makensis -DSTAGE=<stage.mjs output> -DVERSION=1.2.3 -DEDITION=full|client
;            -DAPP_GUID=<uuid> -DOUTFILE=<path> native-v2/package/windows/crundi.nsi
;
; APP_GUID is electron-builder's: UUID v5 of the appId (package/feed.mjs guid).

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "StrFunc.nsh"
${StrStr}

!if "${EDITION}" == "client"
  !define PRODUCT_NAME "Crundi Client"
  !define APP_ID "com.crundi.client"
  !define EXE "crundi-client.exe"
  !define OLD_EXE "Crundi Client.exe"
  !define DIR_NAME "crundi-client"
!else
  !define PRODUCT_NAME "Crundi"
  !define APP_ID "com.crundi.app"
  !define EXE "crundi.exe"
  !define OLD_EXE "Crundi.exe"
  !define DIR_NAME "crundi"
!endif
!define PUBLISHER "Obitech"
!define UNINSTALLER "Uninstall ${PRODUCT_NAME}.exe"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}"
!define INSTALL_KEY "Software\${APP_GUID}"

Name "${PRODUCT_NAME}"
OutFile "${OUTFILE}"
; Where electron-builder put it: Programs\<package name> (crundi,
; crundi-client; seen in the CI upgrade test), so an install moves over in place.
InstallDir "$LOCALAPPDATA\Programs\${DIR_NAME}"
Icon "${ICON}"
UninstallIcon "${ICON}"
BrandingText "${PRODUCT_NAME} ${VERSION}"
VIProductVersion "${VIVERSION}"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME}"
VIAddVersionKey "LegalCopyright" "Copyright © ${PUBLISHER}"

; One click: a progress window only, as electron-builder's oneClick installer.
AutoCloseWindow true
ShowInstDetails nevershow
ShowUninstDetails nevershow
Page instfiles
UninstPage instfiles

Var running

; Stop everything running from the install folder, by PID: the app, its
; browser host, the worker, the bundled Node's server, and (from an Electron
; install) the server Electron ran with the machine's Node from this folder.
!macro StopInstalled
  System::Call 'kernel32::GetCurrentProcessId() i .r9'
  nsExec::ExecToLog `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d='$INSTDIR\'; Get-CimInstance Win32_Process | Where-Object { $$_.ProcessId -ne $$PID -and $$_.ProcessId -ne $9 -and $$_.Name -notlike 'Au_*' -and $$_.Name -notlike 'Un_*' -and (($$_.ExecutablePath -and $$_.ExecutablePath.StartsWith($$d, 'OrdinalIgnoreCase')) -or ($$_.CommandLine -and $$_.CommandLine.IndexOf($$d, [StringComparison]::OrdinalIgnoreCase) -ge 0)) } | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force -ErrorAction SilentlyContinue }"`
  Pop $running
  Sleep 500
!macroend

Function .onInit
  ; An earlier install (Electron's or ours) wherever it went.
  ReadRegStr $0 HKCU "${INSTALL_KEY}" "InstallLocation"
  ${If} $0 != ""
  ${AndIf} ${FileExists} "$0\*.*"
    StrCpy $INSTDIR $0
  ${EndIf}
FunctionEnd

Section "Install"
  SetShellVarContext current
  !insertmacro StopInstalled

  ; Replace the old install entirely (Electron's files, or our previous
  ; version), only when the folder is one of ours. User data is elsewhere
  ; (%APPDATA%\${PRODUCT_NAME}) and is never touched.
  ${If} ${FileExists} "$INSTDIR\${UNINSTALLER}"
  ${OrIf} ${FileExists} "$INSTDIR\${EXE}"
  ${OrIf} ${FileExists} "$INSTDIR\resources\app.asar"
    RMDir /r /REBOOTOK "$INSTDIR"
  ${EndIf}

  SetOutPath "$INSTDIR"
  File /r "${STAGE}\*.*"
  WriteUninstaller "$INSTDIR\${UNINSTALLER}"

  ; The uninstall entry, under electron-builder's key so this one replaces Electron's.
  WriteRegStr HKCU "${INSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME} ${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\${UNINSTALLER}" /currentuser'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\${UNINSTALLER}" /currentuser /S'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${EXE},0"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" "$0"

  ; Desktop and Start menu shortcuts, as electron-builder makes them.
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\${EXE}" 0 "" "" "${PRODUCT_NAME}"
  CreateShortCut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXE}" "" "$INSTDIR\${EXE}" 0 "" "" "${PRODUCT_NAME}"

  ; Launch at login carries over: Electron's Run value names "...\${OLD_EXE}";
  ; point it at the new executable, keeping the value's name (one entry).
  StrCpy $1 0
  loop:
    EnumRegValue $2 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" $1
    StrCmp $2 "" done
    ReadRegStr $3 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" $2
    ${StrStr} $4 $3 "$INSTDIR\${OLD_EXE}"
    ${If} $4 != ""
      WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" $2 '"$INSTDIR\${EXE}"'
      DetailPrint "Launch at login: $2 now starts $INSTDIR\${EXE}"
    ${EndIf}
    IntOp $1 $1 + 1
    Goto loop
  done:

!if "${EDITION}" != "client"
  ; build/installer.nsh customInstall: the DisconnectRDP task (locked-screen
  ; capture), created if missing (fails harmlessly without elevation; the app's
  ; setup step offers it then).
  nsExec::ExecToStack 'schtasks /query /tn "DisconnectRDP"'
  Pop $0
  ${If} $0 != 0
    nsExec::ExecToLog 'schtasks /create /tn "DisconnectRDP" /tr "\"$INSTDIR\server\scripts\disconnect-rdp.bat\"" /sc once /sd 01/01/2099 /st 00:00 /rl highest /f'
  ${Else}
    ; A task an Electron install made runs the .bat from Electron's place; it
    ; needs elevation to change, so the .bat stays reachable there too.
    CreateDirectory "$INSTDIR\resources\app.asar.unpacked\scripts"
    CopyFiles /SILENT "$INSTDIR\server\scripts\disconnect-rdp.bat" "$INSTDIR\resources\app.asar.unpacked\scripts"
  ${EndIf}

  ; The crundi skill into the user's Claude config, by name only.
  IfFileExists "$INSTDIR\server\skills\crundi\*.*" 0 crundiSkillsDone
    RMDir /r "$PROFILE\.claude\skills\crundi"
    CreateDirectory "$PROFILE\.claude\skills\crundi"
    CopyFiles /SILENT "$INSTDIR\server\skills\crundi\*.*" "$PROFILE\.claude\skills\crundi"
    DetailPrint "Installed the crundi skill to $PROFILE\.claude\skills\crundi"
  crundiSkillsDone:
!endif
SectionEnd

; electron-builder's oneClick starts the app when it is done; a silent
; install (an update) only with --force-run, as electron-updater asks.
Function .onInstSuccess
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "--force-run" $1
  ${IfNot} ${Silent}
  ${OrIfNot} ${Errors}
    Exec '"$INSTDIR\${EXE}"'
  ${EndIf}
FunctionEnd

Function un.onInit
  SetShellVarContext current
FunctionEnd

Section "Uninstall"
  !insertmacro StopInstalled
!if "${EDITION}" != "client"
  ; build/installer.nsh customUnInstall
  nsExec::ExecToLog 'schtasks /delete /tn "DisconnectRDP" /f'
!endif
  Delete "$SMPROGRAMS\${PRODUCT_NAME}.lnk"
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "${INSTALL_KEY}"
  RMDir /r /REBOOTOK "$INSTDIR"

  ; Ask whether to keep user configuration. Default (silent / Enter) = keep.
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Keep your configuration?$\n$\nChoose Yes to preserve your bot token, registered projects and topics in AppData (recommended if you plan to reinstall).$\n$\nChoose No to remove all configuration." \
    /SD IDYES \
    IDYES keepConfig IDNO removeConfig
  removeConfig:
    RMDir /r "$APPDATA\${PRODUCT_NAME}"
!if "${EDITION}" != "client"
    RMDir /r "$PROFILE\.claude\skills\crundi"
!endif
  keepConfig:
SectionEnd
