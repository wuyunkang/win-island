param([string]$DataDirectory)
$ErrorActionPreference = 'Stop'
if (-not $DataDirectory) {
    $DataDirectory = if ($env:WIN_ISLAND_DATA_DIR) { $env:WIN_ISLAND_DATA_DIR } else { Join-Path $env:APPDATA 'WinIslandRust' }
}
$taskDiscovery = Join-Path $DataDirectory 'live-api.json'
if (-not (Test-Path -LiteralPath $taskDiscovery)) { throw '请先启动 灵动岛，并在设置中启用实时活动接口，或使用 --activity-api 参数。' }
$taskApi = Get-Content -LiteralPath $taskDiscovery -Raw | ConvertFrom-Json
if ($taskApi.endpoint -notmatch '^http://127\.0\.0\.1:\d+$') { throw '接口地址必须是本机地址' }
$taskHeaders = @{ Authorization = "Bearer $($taskApi.token)" }
function Send-IslandEvent([string]$Route, [hashtable]$Payload) {
    $taskBody = [Text.Encoding]::UTF8.GetBytes(($Payload | ConvertTo-Json -Compress))
    Invoke-RestMethod -Uri "$($taskApi.endpoint)$Route" -Method Post -Headers $taskHeaders -ContentType 'application/json; charset=utf-8' -Body $taskBody | Out-Null
}
$taskId = "demo-$PID"
try {
    foreach ($taskProgress in @(0,0.25,0.5,0.75,1)) {
        Send-IslandEvent '/activity' @{id=$taskId;title='演示：正在构建';subtitle='来自 PowerShell 的实时进度';progress=$taskProgress;priority=50}
        Start-Sleep -Milliseconds 800
    }
} finally {
    Invoke-RestMethod -Uri "$($taskApi.endpoint)/activity/$taskId" -Method Delete -Headers $taskHeaders | Out-Null
}
Send-IslandEvent '/notify' @{title='构建完成';subtitle='这条提醒会在 5 秒后消失';duration=5;priority=70}
Write-Output '演示通知和进度已发送。'
