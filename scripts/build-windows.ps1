# Build MD Reader for Windows (.msi + .exe/NSIS).
# Run this ON a Windows machine (PowerShell) — Tauri can't be cross-compiled from macOS.
#
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
#
# Output: src-tauri\target\release\bundle\{msi,nsis}\
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "MD Reader - Windows build" -ForegroundColor Cyan

# 1. Prerequisites (check + hint; install manually if missing):
#    - Microsoft Visual Studio C++ Build Tools (MSVC) + Windows SDK
#    - WebView2 Runtime (preinstalled on Windows 11; else from Microsoft)
#    - Rust (https://rustup.rs)  - Node.js 18+
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Host "Rust not found. Install from https://rustup.rs then re-run." -ForegroundColor Yellow
  exit 1
}
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  Write-Host "Node.js not found. Install Node 18+ then re-run." -ForegroundColor Yellow
  exit 1
}

# 2. Node deps + build.
Write-Host "npm install..." -ForegroundColor Cyan
npm install
Write-Host "Building (vite + tauri)..." -ForegroundColor Cyan
npm run build:desktop

Write-Host ""
Write-Host "Done. Installers under:" -ForegroundColor Green
Write-Host "   src-tauri\target\release\bundle\msi\*.msi"
Write-Host "   src-tauri\target\release\bundle\nsis\*-setup.exe"
