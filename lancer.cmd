@echo off
rem Lance Fuck Calorie sur ce PC (mode developpement).
rem
rem   lancer              ouvre l'application
rem   lancer installer    construit l'installateur Windows (.msi et .exe)
rem
rem La premiere fois, installe ce qui manque (plusieurs minutes).
setlocal
cd /d "%~dp0app" || goto :erreur

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust est introuvable. Installe-le depuis https://rustup.rs puis relance ce fichier.
  goto :erreur
)

rustup target list --installed | findstr /x "wasm32-unknown-unknown" >nul
if errorlevel 1 (
  echo Installation de la cible WebAssembly...
  rustup target add wasm32-unknown-unknown || goto :erreur
)

where trunk >nul 2>nul
if errorlevel 1 (
  echo Installation de Trunk, une seule fois, quelques minutes...
  cargo install trunk --locked || goto :erreur
)

cargo tauri --version >nul 2>nul
if errorlevel 1 (
  echo Installation de Tauri, une seule fois, quelques minutes...
  cargo install tauri-cli --version "^2" --locked || goto :erreur
)

if /i "%~1"=="installer" goto :installer

echo Lancement de Fuck Calorie. La premiere compilation prend plusieurs minutes...
cargo tauri dev || goto :erreur
exit /b 0

:installer
cargo tauri build || goto :erreur
echo.
echo Installateur pret dans %~dp0target\release\bundle
explorer "%~dp0target\release\bundle"
pause
exit /b 0

:erreur
echo.
echo Echec. Copie les dernieres lignes ci-dessus pour obtenir de l'aide.
pause
exit /b 1
