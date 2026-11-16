# 用户目录内安装 Rust 与便携 LLVM/MinGW；不修改系统 PATH，不需要系统级 C++ 安装。
$ErrorActionPreference = 'Stop'
$taskTools = Join-Path $env:LOCALAPPDATA 'WinIslandRust\tools'
$taskCargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
New-Item -ItemType Directory -Path $taskTools -Force | Out-Null
if (-not (Test-Path -LiteralPath (Join-Path $taskCargoBin 'rustup.exe'))) {
    $taskInstaller = Join-Path $taskTools 'rustup-init.exe'
    Invoke-WebRequest -Uri 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $taskInstaller
    $env:RUSTUP_INIT_SKIP_PREREQ_CHECK = '1'
    & $taskInstaller -y --profile minimal --default-host x86_64-pc-windows-gnullvm --no-modify-path
    if ($LASTEXITCODE -ne 0) { throw 'Rust 安装失败' }
}
$taskArchive = 'llvm-mingw-20261104-ucrt-x86_64'
if (-not (Test-Path -LiteralPath (Join-Path $taskTools "$taskArchive\bin\clang.exe"))) {
    $taskZip = Join-Path $taskTools "$taskArchive.zip"
    Invoke-WebRequest -Uri "https://github.com/mstorsjo/llvm-mingw/releases/download/20261104/$taskArchive.zip" -OutFile $taskZip
    Expand-Archive -LiteralPath $taskZip -DestinationPath $taskTools -Force
}
& (Join-Path $taskCargoBin 'rustup.exe') toolchain install stable-x86_64-pc-windows-gnullvm --profile minimal
if ($LASTEXITCODE -ne 0) { throw 'Rust LLVM 工具链安装失败' }
Write-Output '环境已就绪。运行 ./scripts/dev.ps1 -Action Run；测试用 -Action Test。'
