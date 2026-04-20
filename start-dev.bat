@echo off
echo TOAPIPROXY 开发服务器
echo.

:: 设置 Rust 环境变量
set PATH=%PATH%;%USERPROFILE%\.cargo\bin

echo 正在启动应用...
cargo tauri dev
pause
