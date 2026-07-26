LangString BROSDK_DELETE_USER_DATA ${LANG_SIMPCHINESE} "是否同时删除 BroSDK Dashboard 的本地数据、日志和受保护凭据？"
LangString BROSDK_DELETE_USER_DATA ${LANG_ENGLISH} "Also delete BroSDK Dashboard local data, logs, and protected credentials?"

!macro NSIS_HOOK_POSTUNINSTALL
  IfSilent keep_user_data
  MessageBox MB_YESNO|MB_ICONQUESTION "$(BROSDK_DELETE_USER_DATA)" IDNO keep_user_data
  RMDir /r "$LOCALAPPDATA\BroSDK Dashboard"
  keep_user_data:
!macroend
