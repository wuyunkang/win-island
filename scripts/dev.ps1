param(
    [ValidateSet('Build','Test','Run','Check')][string]$Action = 'Build',
    [switch]$Release,
    [switch]$ActivityApi
)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskCargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path -LiteralPath $taskCargo)) { throw '未找到 Rust。请先运行 scripts/bootstrap.ps1。' }
$taskLlvm = Join-Path $env:LOCALAPPDATA 'WinIslandRust\tools\llvm-mingw-20261104-ucrt-x86_64\bin'
$taskCargoArgs = @()
if (Test-Path -LiteralPath $taskLlvm) {
    $env:PATH = "$taskLlvm;$env:USERPROFILE\.cargo\bin;$env:PATH"
    $env:CC = 'clang'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = 'clang'
    $taskCargoArgs += '+stable-x86_64-pc-windows-gnullvm'
    $taskTarget = 'x86_64-pc-windows-gnullvm'
} else {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    $taskTarget = 'x86_64-pc-windows-msvc'
}
Push-Location -LiteralPath $taskRoot
try {
    $taskCargoArgs += $Action.ToLowerInvariant()
    $taskCargoArgs += @('--locked','--target',$taskTarget)
    if ($Action -eq 'Test') { $taskCargoArgs += @('--workspace','--all-targets','--features','record') }
    elseif ($Action -eq 'Check') { $taskCargoArgs += @('--workspace','--all-targets','--features','record') }
    else { $taskCargoArgs += @('--bin','win-island-rust') }
    if ($Release) { $taskCargoArgs += '--release' }
    if ($Action -eq 'Test') { $taskCargoArgs += @('--','--test-threads=1') }
    if ($Action -eq 'Run' -and $ActivityApi) { $taskCargoArgs += @('--','--activity-api') }
    & $taskCargo @taskCargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo 失败，退出码 $LASTEXITCODE" }
} finally { Pop-Location }
