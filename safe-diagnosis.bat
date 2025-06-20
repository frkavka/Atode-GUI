@echo off
echo === Atode プロジェクト診断 ===
echo.

echo 1. Tauri CLI バージョン:
npx tauri --version
echo.

echo 2. Node.js バージョン:
node --version
echo.

echo 3. npm バージョン:
npm --version
echo.

echo 4. 現在のpackage.json:
if exist "package.json" (
    echo ✅ package.json 存在
    type package.json | findstr "tauri-apps"
) else (
    echo ❌ package.json なし
)
echo.

echo 5. 現在のtauri.conf.json:
if exist "src-tauri\tauri.conf.json" (
    echo ✅ tauri.conf.json 存在
    echo 最初の数行:
    type "src-tauri\tauri.conf.json" | more +1 | head -5
) else (
    echo ❌ tauri.conf.json なし
)
echo.

echo 6. 現在のCargo.toml:
if exist "src-tauri\Cargo.toml" (
    echo ✅ Cargo.toml 存在
    echo Tauriバージョン:
    type "src-tauri\Cargo.toml" | findstr "tauri"
) else (
    echo ❌ Cargo.toml なし
)
echo.

echo 7. ファイル構造:
echo プロジェクトルート:
dir /b | findstr -v "node_modules"
echo.
echo src-tauri:
dir src-tauri /b 2>nul
echo.

echo 診断完了
pause