!macro NSIS_HOOK_POSTUNINSTALL
  MessageBox MB_YESNO|MB_ICONQUESTION "是否同时删除 BroSDK Dashboard 的本地数据、日志和受保护凭据？" IDNO keep_user_data
  RMDir /r "$LOCALAPPDATA\BroSDK Dashboard"
  keep_user_data:
!macroend
