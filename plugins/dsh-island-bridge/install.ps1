param([ValidateSet('desktop','web')][string]$Profile='desktop',[switch]$Uninstall)
$ErrorActionPreference='Stop'
$taskNode=Get-Command node -ErrorAction SilentlyContinue
$taskNodePath=if($taskNode){$taskNode.Source}else{$null}
if(-not $taskNodePath){
    $taskBundledRoot=Join-Path $env:USERPROFILE '.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node'
    if(Test-Path -LiteralPath $taskBundledRoot){$taskNodePath=Get-ChildItem -LiteralPath $taskBundledRoot -Filter node.exe -File -Recurse|Select-Object -First 1 -ExpandProperty FullName}
}
if(-not $taskNodePath){throw '未找到 Node.js。请使用 DSH 自带 Node 或 Node.js 22.19 及以上版本运行 install.mjs。'}
$taskArgs=@((Join-Path $PSScriptRoot 'install.mjs'),'--profile',$Profile)
if($Uninstall){$taskArgs+='--uninstall'}
& $taskNodePath @taskArgs
if($LASTEXITCODE -ne 0){throw 'DSH 桥接安装未完成，请查看上方错误。'}
