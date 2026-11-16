param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$taskExecutable = (Resolve-Path -LiteralPath $Executable).Path
$taskData = Join-Path ([IO.Path]::GetTempPath()) ('win-island-smoke-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskData | Out-Null
$taskListener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
$taskListener.Start(); $taskPort = $taskListener.LocalEndpoint.Port; $taskListener.Stop()
@("live_api=1","live_api_port=$taskPort",'visible_pages=00010000','pet=0','system_hud=0','bluetooth_notifications=0','clipboard_history=0','mail_enabled=0','notifications=1') | Set-Content -LiteralPath (Join-Path $taskData 'settings.txt') -Encoding utf8
'{"enabled":false,"compact":false,"selected":"","accounts":[],"colors":{}}' | Set-Content -LiteralPath (Join-Path $taskData 'ai-quota.json') -Encoding utf8
$taskPrevious = $env:WIN_ISLAND_DATA_DIR
$taskProcess = $null
try {
    $env:WIN_ISLAND_DATA_DIR = $taskData
    $taskProcess = Start-Process -FilePath $taskExecutable -WindowStyle Hidden -PassThru
    $taskDiscovery = Join-Path $taskData 'live-api.json'
    $taskDeadline = [DateTime]::UtcNow.AddSeconds(25)
    while (-not (Test-Path -LiteralPath $taskDiscovery)) {
        if ($taskProcess.HasExited) { throw "程序提前退出：$($taskProcess.ExitCode)" }
        if ([DateTime]::UtcNow -gt $taskDeadline) { throw "接口启动超时，诊断在 $taskData" }
        Start-Sleep -Milliseconds 100
    }
    $taskApi = Get-Content -LiteralPath $taskDiscovery -Raw | ConvertFrom-Json
    $taskHeaders = @{ Authorization = "Bearer $($taskApi.token)" }
    $taskBody = [Text.Encoding]::UTF8.GetBytes('{"id":"smoke","title":"本机启动检查","progress":0.42}')
    Invoke-RestMethod "$($taskApi.endpoint)/activity" -Method Post -Headers $taskHeaders -ContentType 'application/json' -Body $taskBody | Out-Null
    $taskState = Invoke-RestMethod "$($taskApi.endpoint)/status" -Headers $taskHeaders
    if ($taskState.activities.Count -ne 1 -or [Math]::Abs($taskState.activities[0].progress - 0.42) -gt 0.001) { throw '实际进程的活动状态不正确' }
    $taskUnauthorized = $false
    try { Invoke-RestMethod "$($taskApi.endpoint)/status" | Out-Null } catch { $taskUnauthorized = [int]$_.Exception.Response.StatusCode -eq 401 }
    if (-not $taskUnauthorized) { throw '未认证请求没有被拒绝' }
    Invoke-RestMethod "$($taskApi.endpoint)/activity/smoke" -Method Delete -Headers $taskHeaders | Out-Null
    & (Join-Path $PSScriptRoot 'activity-demo.ps1') -DataDirectory $taskData
    if (-not ('WinIslandSmokeWindow' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class WinIslandSmokeWindow {
    private delegate bool Visitor(IntPtr window, IntPtr value);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Visitor visitor, IntPtr value);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    public static void Close(uint process) {
        EnumWindows((window, value) => { uint id; GetWindowThreadProcessId(window, out id);
            if (id == process) PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero); return true; }, IntPtr.Zero);
    }
}
'@
    }
    [WinIslandSmokeWindow]::Close([uint32]$taskProcess.Id)
    if (-not $taskProcess.WaitForExit(10000)) { throw '程序未能正常关闭' }
    if ($taskProcess.ExitCode -ne 0) { throw "退出码异常：$($taskProcess.ExitCode)" }
    if (Test-Path -LiteralPath $taskDiscovery) { throw '正常退出后仍残留接口凭据' }
    Write-Output '进程启动、活动收发、访问认证、演示脚本与正常退出检查通过。'
    Write-Output "诊断目录：$taskData"
} finally {
    if ($taskProcess -and -not $taskProcess.HasExited) { Stop-Process -Id $taskProcess.Id -Force }
    if ($null -eq $taskPrevious) { Remove-Item Env:WIN_ISLAND_DATA_DIR -ErrorAction SilentlyContinue } else { $env:WIN_ISLAND_DATA_DIR = $taskPrevious }
}
