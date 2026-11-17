$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'dev.ps1') -Action Build -Release
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskTarget = if (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'WinIslandRust\tools\llvm-mingw-20261104-ucrt-x86_64\bin')) { 'x86_64-pc-windows-gnullvm' } else { 'x86_64-pc-windows-msvc' }
$taskName = 'LingDongIsland-0.2.4-windows-x64'
$taskDestination = Join-Path $taskRoot "dist\$taskName"
New-Item -ItemType Directory -Path $taskDestination -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $taskRoot "target\$taskTarget\release\win-island-rust.exe") -Destination (Join-Path $taskDestination '灵动岛.exe') -Force
$taskVersion = (Get-Item -LiteralPath (Join-Path $taskDestination '灵动岛.exe')).VersionInfo
if ($taskVersion.ProductName -ne '灵动岛' -or $taskVersion.ProductVersion -ne '0.2.4') { throw '发布程序的中文产品名称或版本不正确，停止打包。' }
foreach ($taskFile in @('LICENSE','THIRD_PARTY_NOTICES.txt','README.md')) { Copy-Item -LiteralPath (Join-Path $taskRoot $taskFile) -Destination $taskDestination -Force }
New-Item -ItemType Directory -Path (Join-Path $taskDestination 'docs') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $taskRoot 'docs\ACTIVITY_API.md') -Destination (Join-Path $taskDestination 'docs') -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'docs\README.upstream.md') -Destination (Join-Path $taskDestination 'docs') -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'docs\VALIDATION.md') -Destination (Join-Path $taskDestination 'docs') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'activity-demo.ps1') -Destination $taskDestination -Force
$taskBridgeDestination=Join-Path $taskDestination 'dsh-bridge'
New-Item -ItemType Directory -Path $taskBridgeDestination -Force | Out-Null
foreach($taskBridgeFile in @('index.mjs','adapter.mjs','bridge.mjs','client.js','install.mjs','install.ps1','package.json','README.md')){Copy-Item -LiteralPath (Join-Path $taskRoot "plugins\dsh-island-bridge\$taskBridgeFile") -Destination $taskBridgeDestination -Force}
Copy-Item -LiteralPath (Join-Path $taskRoot 'LICENSE') -Destination $taskBridgeDestination -Force
$taskPortableReadme=Join-Path $taskDestination 'README.md'
$taskReadmeText=[IO.File]::ReadAllText($taskPortableReadme).Replace('(plugins/dsh-island-bridge/README.md)','(dsh-bridge/README.md)')
[IO.File]::WriteAllText($taskPortableReadme,$taskReadmeText,[Text.UTF8Encoding]::new($false))
$taskZip = Join-Path $taskRoot "dist\$taskName.zip"
Compress-Archive -Path (Join-Path $taskDestination '*') -DestinationPath $taskZip -Force
$taskHash = (Get-FileHash -LiteralPath $taskZip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$taskZip.sha256" -Encoding ascii -Value "$taskHash  $taskName.zip"
Write-Output "免安装程序：$taskDestination\灵动岛.exe"
Write-Output "压缩包：$taskZip"
