; Chatters NSIS 安装钩子：自动放行局域网端口（UDP 28100 / TCP 28101）。
; 说明：安装程序以管理员权限运行，netsh 规则仅对「域/专用」网络生效，避免公网暴露。
; 端口若在应用内修改，需要用户自行放行新端口（设置页有提示）。

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Configuring Windows Firewall rules for Chatters..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Chatters UDP 28100"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Chatters TCP 28101"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Chatters UDP 28100" dir=in action=allow protocol=UDP localport=28100 profile=domain,private'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Chatters TCP 28101" dir=in action=allow protocol=TCP localport=28101 profile=domain,private'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DetailPrint "Removing Chatters firewall rules..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Chatters UDP 28100"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Chatters TCP 28101"'
!macroend
