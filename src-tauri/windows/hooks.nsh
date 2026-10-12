; CONest NSIS installer hooks
; Keep installer/uninstaller cleanup here so Windows uninstall leaves no stale
; startup entry behind.


!macro NSIS_HOOK_POSTINSTALL
  ; Register Native Messaging from the actual installation directory.
  ; The special mode exits before creating the Tauri window.
  ExecWait '"$INSTDIR\CONest.exe" --register-native-messaging-only'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; CONest autostart is registered per-user by the application.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "CONest"
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.nununuuuu.conest"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.nununuuuu.conest"
  Delete "$LOCALAPPDATA\CONest\NativeMessaging\com.nununuuuu.conest.json"
  RMDir "$LOCALAPPDATA\CONest\NativeMessaging"
!macroend
