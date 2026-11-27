; 卸载时清理便携数据。
;
; 应用把配置、项目和 FRP 客户端放在 exe 同级的 config / projects / versions
; 目录里（便携模式）。Tauri 默认卸载器只删除自己安装的程序文件，内置的
; 「Delete the application data」复选框也只清理 %APPDATA% / %LOCALAPPDATA%，
; 所以这些目录会残留。这里在卸载时把它们一并删除。
;
; 规则：
; - 自动更新（/UPDATE）不删除数据，避免升级时丢失配置。
; - 用户勾选了内置的「Delete the application data」时，直接清理，不再追问。
; - 静默卸载不弹窗、不删除数据（避免脚本/更新误删）。
; - 交互式卸载默认询问一次，默认按钮为「是」。
; - 只删除本应用自己的数据目录，不递归删除整个 $INSTDIR，因为用户可能把
;   程序放在包含其他文件的目录里。
; - 自定义到安装目录之外的数据目录不在此处处理（应用无从得知），会保留。

!macro NSIS_HOOK_PREUNINSTALL
  ; 更新模式：保留数据。
  ${If} $UpdateMode = 1
    Goto ftpm_keep_data
  ${EndIf}

  ; 用户已经勾选内置的「Delete the application data」，视为同意清理。
  ${If} $DeleteAppDataCheckboxState = 1
    Goto ftpm_remove_data
  ${EndIf}

  ; 静默/被动卸载（脚本、更新器）：保留数据，直接跳过清理。
  ${If} $PassiveMode = 1
    Goto ftpm_keep_data
  ${EndIf}
  IfSilent ftpm_keep_data

  MessageBox MB_YESNO|MB_ICONQUESTION \
    "是否同时删除程序目录中的配置、项目和已安装的 FRP 客户端？$\r$\n$\r$\n选择「否」将保留这些数据，便于以后重新安装时继续使用。" \
    IDNO ftpm_keep_data

  ftpm_remove_data:
  RMDir /r "$INSTDIR\config"
  RMDir /r "$INSTDIR\projects"
  RMDir /r "$INSTDIR\versions"

  ftpm_keep_data:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; 数据目录已删除时，安装目录通常已经为空；非递归删除，避免误删用户文件。
  RMDir "$INSTDIR"
!macroend
