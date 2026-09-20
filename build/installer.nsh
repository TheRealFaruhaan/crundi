; Custom NSIS hooks for electron-builder.
; Extends the generated installer/uninstaller with our own behaviour.

!macro customInstall
  ; Create the DisconnectRDP scheduled task so the bot can keep the GUI session
  ; alive when the physical screen is locked (RPA trick via tscon.exe). The task
  ; runs disconnect-rdp.bat with highest privileges; the bot triggers it at
  ; runtime via "schtasks /run" without needing UAC.
  ;
  ; Skip if it already exists (e.g. a reinstall where the user set it up before).
  ; If the installer isn't elevated this create may fail — that's fine: Crundi
  ; offers a one-click "Set up" step in the app's setup wizard and shows a
  ; dismissible prompt on launch whenever the task is still missing.
  nsExec::ExecToStack 'schtasks /query /tn "DisconnectRDP"'
  Pop $0 ; exit code: 0 = task exists
  ${If} $0 != 0
    nsExec::ExecToLog 'schtasks /create /tn "DisconnectRDP" /tr "\"$INSTDIR\resources\app.asar.unpacked\scripts\disconnect-rdp.bat\"" /sc once /sd 01/01/2099 /st 00:00 /rl highest /f'
  ${EndIf}

  ; Crundi ships a skill telling Claude how to drive this machine: the MCP
  ; tools, and how to run, expose, test and hand over work on a Crundi host. It
  ; belongs to the USER's Claude config, not to the install directory, so every
  ; project on the box picks it up. Mirrors what scripts/install.sh does on Linux.
  ;
  ; Overwritten on every install on purpose: the repo copy is canonical, so an
  ; upgrade must be able to correct it. Anything hand-edited in place is lost,
  ; which is why the skill itself says to edit the repo copy.
  ;
  ; Guarded: the client-only build ships no skills, and a missing directory here
  ; must not fail the install. Add a RMDir line per new skill directory shipped,
  ; so a rename cannot leave the old copy behind.
  IfFileExists "$INSTDIR\resources\skills\*.*" 0 crundiSkillsDone
    RMDir /r "$PROFILE\.claude\skills\crundi"
    CreateDirectory "$PROFILE\.claude\skills"
    CopyFiles /SILENT "$INSTDIR\resources\skills\*.*" "$PROFILE\.claude\skills"
    DetailPrint "Installed Crundi skills to $PROFILE\.claude\skills"
  crundiSkillsDone:
!macroend

!macro customUnInstall
  ; Remove the DisconnectRDP scheduled task
  nsExec::ExecToLog 'schtasks /delete /tn "DisconnectRDP" /f'

  ; Ask whether to keep user configuration. Default (silent / Enter) = keep.
  ; The config lives in %APPDATA%\${PRODUCT_NAME}\ and contains the bot token,
  ; registered projects, topic state, etc.
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Keep your configuration?$\n$\nChoose Yes to preserve your bot token, registered projects and topics in AppData (recommended if you plan to reinstall).$\n$\nChoose No to remove all configuration." \
    /SD IDYES \
    IDYES keepConfig IDNO removeConfig

  removeConfig:
    RMDir /r "$APPDATA\${PRODUCT_NAME}"
    RMDir /r "$PROFILE\.claude\skills\crundi"
    Goto doneConfig
  keepConfig:
  doneConfig:
!macroend
