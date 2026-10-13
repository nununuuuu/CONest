@echo off
setlocal EnableExtensions
chcp 65001 >nul
cd /d "%~dp0"

set "OUT=%~dp0dist-package"
set "PORTABLE=%OUT%\CONest-Portable"
set "TARGET=x86_64-pc-windows-msvc"
set "BIN=%~dp0src-tauri\binaries\windows"

for /f "usebackq delims=" %%V in (`node -p "require('./package.json').version"`) do set "APP_VERSION=%%V"

echo ============================================================
echo  CONest v%APP_VERSION% - Windows Builder
echo ============================================================
echo.

where node >nul 2>nul
if errorlevel 1 (
  echo ERROR: Node.js not found.
  goto :failed
)

where cargo >nul 2>nul
if errorlevel 1 (
  echo ERROR: Rust/Cargo not found.
  goto :failed
)

for %%F in (
  "aria2c-%TARGET%.exe"
  "ffmpeg-%TARGET%.exe"
  "DanmakuFactory-%TARGET%.exe"
) do (
  if not exist "%BIN%\%%~F" (
    echo ERROR: Missing sidecar: %BIN%\%%~F
    goto :failed
  )
)

if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%"
mkdir "%PORTABLE%"

echo [1/4] Preparing verified multi-platform engines...
node scripts\provider-tools.mjs
if errorlevel 1 goto :failed

echo [1/4] Installing dependencies...
call npx -y pnpm@10.25.0 install --frozen-lockfile
if errorlevel 1 goto :failed

echo.
echo [2/4] Building Native Messaging host...
cargo build --manifest-path "native-host\Cargo.toml" --release
if errorlevel 1 goto :failed
copy /y "native-host\target\release\conest-native-host.exe" "%BIN%\conest-native-host-%TARGET%.exe" >nul
if errorlevel 1 goto :failed
if not exist "src-tauri\target\release" mkdir "src-tauri\target\release"
copy /y "native-host\target\release\conest-native-host.exe" "src-tauri\target\release\conest-native-host.exe" >nul
if errorlevel 1 goto :failed

echo.
echo [2/4] Building CONest...

rem Ensure NSIS uses the CONest uninstall cleanup hook.
rem This edits only bundle.windows.nsis.installerHooks and preserves updater pubkey.
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$p='src-tauri/tauri.conf.json';" ^
  "$cfg=Get-Content -Raw -Encoding UTF8 $p | ConvertFrom-Json;" ^
  "if (-not $cfg.bundle.windows) { $cfg.bundle | Add-Member -NotePropertyName windows -NotePropertyValue ([pscustomobject]@{}) };" ^
  "if (-not $cfg.bundle.windows.nsis) { $cfg.bundle.windows | Add-Member -NotePropertyName nsis -NotePropertyValue ([pscustomobject]@{}) };" ^
  "if ($cfg.bundle.windows.nsis.PSObject.Properties['installerHooks']) { $cfg.bundle.windows.nsis.installerHooks='./windows/hooks.nsh' } else { $cfg.bundle.windows.nsis | Add-Member -NotePropertyName installerHooks -NotePropertyValue './windows/hooks.nsh' };" ^
  "$json=$cfg | ConvertTo-Json -Depth 100;" ^
  "$utf8NoBom=New-Object System.Text.UTF8Encoding($false);" ^
  "[System.IO.File]::WriteAllText((Resolve-Path $p),$json,$utf8NoBom)"
if errorlevel 1 goto :failed

call npx -y pnpm@10.25.0 tauri build
if errorlevel 1 goto :failed

echo.
echo [3/4] Creating portable package...
if not exist "src-tauri\target\release\conest.exe" (
  echo ERROR: src-tauri\target\release\conest.exe not found.
  goto :failed
)
copy /y "src-tauri\target\release\conest.exe" "%PORTABLE%\CONest.exe" >nul
copy /y "%BIN%\aria2c-%TARGET%.exe" "%PORTABLE%\aria2c.exe" >nul
copy /y "%BIN%\ffmpeg-%TARGET%.exe" "%PORTABLE%\ffmpeg.exe" >nul
copy /y "%BIN%\DanmakuFactory-%TARGET%.exe" "%PORTABLE%\DanmakuFactory.exe" >nul
copy /y "native-host\target\release\conest-native-host.exe" "%PORTABLE%\conest-native-host.exe" >nul

copy /y "src-tauri\resources\providers\yt-dlp.exe" "%PORTABLE%\yt-dlp.exe" >nul
if errorlevel 1 goto :failed
copy /y "src-tauri\resources\providers\deno.exe" "%PORTABLE%\deno.exe" >nul
if errorlevel 1 goto :failed
copy /y "src-tauri\resources\providers\versions.json" "%PORTABLE%\provider-versions.json" >nul

echo Registering Native Messaging for this local test build...
"%PORTABLE%\CONest.exe" --register-native-messaging-only
if errorlevel 1 (
  echo WARNING: CONest registration command failed. Trying direct HKCU registration...
)

rem Verify the Chrome registry entry exists. If not, create the manifest and
rem registry entries directly from the portable output path. This makes the
rem freshly built extension usable for local testing even before Setup install.
reg query "HKCU\Software\Google\Chrome\NativeMessagingHosts\com.nununuuuu.conest" /ve >nul 2>nul
if errorlevel 1 (
  powershell -NoProfile -ExecutionPolicy Bypass -Command ^
    "$dir=Join-Path $env:LOCALAPPDATA 'CONest\NativeMessaging'; New-Item -ItemType Directory -Force -Path $dir | Out-Null;" ^
    "$manifest=Join-Path $dir 'com.nununuuuu.conest.json';" ^
    "$host=(Resolve-Path '%PORTABLE%\conest-native-host.exe').Path;" ^
    "$m=[ordered]@{name='com.nununuuuu.conest';description='CONest Native Messaging Host';path=$host;type='stdio';allowed_origins=@('chrome-extension://hdcbkkfljbmldjcgnfdmdjjkchekaggp/','chrome-extension://aifecpakbelclcbccfaookeanfcmif/')};" ^
    "$json=$m | ConvertTo-Json -Depth 8; $utf8=New-Object System.Text.UTF8Encoding($false); [IO.File]::WriteAllText($manifest,$json,$utf8);" ^
    "$keys=@('HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.nununuuuu.conest','HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\com.nununuuuu.conest','HKCU:\Software\Chromium\NativeMessagingHosts\com.nununuuuu.conest','HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.nununuuuu.conest','HKCU:\Software\Vivaldi\NativeMessagingHosts\com.nununuuuu.conest');" ^
    "foreach($k in $keys){New-Item -Force -Path $k | Out-Null; Set-Item -Path $k -Value $manifest}"
)
reg query "HKCU\Software\Google\Chrome\NativeMessagingHosts\com.nununuuuu.conest" /ve >nul 2>nul
if errorlevel 1 (
  echo WARNING: Chrome Native Messaging registry verification failed.
) else (
  echo Native Messaging registration: OK
)

for /r "src-tauri\target\release\bundle\nsis" %%F in (*.exe) do (
  copy /y "%%F" "%OUT%\CONest-Setup.exe" >nul
  goto :setup_found
)
:setup_found

echo.
echo [4/4] Packaging Chrome / Edge extension...
if exist "extension" (
  powershell -NoProfile -ExecutionPolicy Bypass -Command ^
    "$p='extension/manifest.json';" ^
    "$m=Get-Content -Raw -Encoding UTF8 $p | ConvertFrom-Json;" ^
    "$m.version='%APP_VERSION%';" ^
    "$json=$m | ConvertTo-Json -Depth 100;" ^
    "$utf8NoBom=New-Object System.Text.UTF8Encoding($false);" ^
    "[System.IO.File]::WriteAllText((Resolve-Path $p),$json,$utf8NoBom)"
  if errorlevel 1 goto :failed
  powershell -NoProfile -Command "Compress-Archive -Path 'extension\*' -DestinationPath '%OUT%\CONest-Extension.zip' -Force"
)

(
echo CONest Portable
echo ==================
echo.
echo Keep these files together:
echo - CONest.exe
echo - aria2c.exe
echo - ffmpeg.exe
echo - DanmakuFactory.exe
echo - conest-native-host.exe
echo - yt-dlp.exe
echo - deno.exe
echo - provider-versions.json
echo.
echo Launch CONest.exe once after moving this portable folder so Native Messaging can refresh its current path.
echo.
) > "%PORTABLE%\README.txt"

echo.
echo ============================================================
echo BUILD COMPLETE
echo ============================================================
echo %OUT%\CONest-Setup.exe
echo %PORTABLE%\CONest.exe
echo %OUT%\CONest-Extension.zip
echo.
pause
exit /b 0

:failed
echo.
echo ============================================================
echo BUILD FAILED
echo ============================================================
pause
exit /b 1
