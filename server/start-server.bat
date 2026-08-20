@echo off
chcp 65001 >nul
setlocal

REM ============================================================
REM  dcr 服务端启动脚本
REM  启动: 信令+STUN+管理后台 (dcr-signal) 与 中继 (dcr-relay)
REM  管理后台: http://localhost:21120  (首次密码见 dcr-signal 窗口日志)
REM  可用环境变量覆盖: DCR_RELAY_HINT / DCR_ADMIN_PASS
REM ============================================================

set "BIN_DIR=%~dp0target\release"
set "UI_DIR=%~dp0admin-ui\dist"

if not exist "%BIN_DIR%\dcr-signal.exe" (
    echo [错误] 未找到 dcr-signal.exe,请先执行: cargo build --release
    pause
    exit /b 1
)

if not exist "%UI_DIR%\index.html" (
    echo [错误] 未找到管理后台 dist,请先执行: cd admin-ui ^&^& npm run build
    pause
    exit /b 1
)

REM 中继地址(供信令下发给客户端),默认本机公网地址需自行修改
if "%DCR_RELAY_HINT%"=="" set "DCR_RELAY_HINT=120.78.77.248:21117"

REM 可选:指定初始管理员密码(缺省随机生成并打印到 dcr-signal 窗口日志)
if not "%DCR_ADMIN_PASS%"=="" set "DCR_ADMIN_PASS_ARG=--admin-pass %DCR_ADMIN_PASS%"

echo ============================================================
echo  dcr 服务端启动中...
echo  信令 TCP 21116 / STUN UDP 21115
echo  中继 TCP 21117 / UDP 21119
echo  管理后台 http://localhost:21120
echo ============================================================

REM 启动中继服务(独立窗口)
start "dcr-relay" cmd /k ""%BIN_DIR%\dcr-relay.exe""

REM 启动信令+管理后台(当前窗口)
"%BIN_DIR%\dcr-signal.exe" --admin-ui "%UI_DIR%" --relay-hint "%DCR_RELAY_HINT%" %DCR_ADMIN_PASS_ARG%

endlocal
