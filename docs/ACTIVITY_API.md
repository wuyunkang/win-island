# 本机实时活动接口

先在通知设置启用，或用 `--activity-api` 启动参数。默认地址 `http://127.0.0.1:17831`；端口修改后接口自动重启。关闭接口或提醒总开关会停止监听并清空活动。

## 发现与认证

接口在数据目录 `%APPDATA%\WinIslandRust\live-api.json` 写入 `endpoint` 和 `token`。所有请求必须带 `Authorization: Bearer <token>`；POST 使用 `Content-Type: application/json`。

令牌每次启动接口时重新随机生成，正常退出或关闭接口删除发现文件。强制结束可能留下旧文件，重启后会替换；旧令牌不能访问新接口。不要分享发现文件。

只监听回环地址，不提供 CORS，拒绝带 Origin 的浏览器请求。正文最多 32 KiB、请求头最多 8 KiB，同时最多保存 64 项。接口不执行命令、不提供任意文件读取，也不会返回聊天通知。

## 路由

| 方法 | 路径 | 行为 |
| --- | --- | --- |
| POST | `/notify` | 短通知，默认 5 秒后过期 |
| POST | `/activity` | 持续活动，默认持续到显式删除 |
| DELETE | `/activity/{id}` | 撤销通知或活动，不存在返回 404 |
| GET | `/status` | 当前活动，按优先级、更新时间排序 |

成功返回 200 和活动 ID。JSON、字段或 HTTP 格式错误返回 400；未认证 401、网页来源 403、正文过大 413。

## 字段

| 字段 | 含义 |
| --- | --- |
| id | 可选，1–64 个英文字母、数字、点、下划线或横线；省略时生成唯一 ID |
| title | 标题，与 progress 至少一个；过滤控制字符，最多显示 120 字符 |
| subtitle | 说明，过滤控制字符，最多显示 300 字符 |
| progress | 0–1，显示进度条和百分比；省略时不画进度条 |
| priority | 0–99，默认 50；较高者优先，同级最近更新者优先 |
| duration | 有效秒数，大于 0 且不超过 86400；更新时重新计时 |

同 ID 提交会**完整替换**旧活动，更新进度时仍应发送标题、说明和期望优先级。progress 为 1 不会自动删除持续活动，可先撤销，再发送完成通知。仅有进度时标题为「任务进度」。

消息提醒先展示，结束后恢复最高优先级活动。「关闭」只移除展示，不取消外部构建或下载。全屏隐藏与应用避让继续生效。

## PowerShell

```powershell
$api = Get-Content "$env:APPDATA\WinIslandRust\live-api.json" -Raw | ConvertFrom-Json
$headers = @{ Authorization = "Bearer $($api.token)" }
$body = [Text.Encoding]::UTF8.GetBytes('{"id":"build","title":"正在构建","progress":0.65}')
Invoke-RestMethod "$($api.endpoint)/activity" -Method Post -Headers $headers `
  -ContentType 'application/json; charset=utf-8' -Body $body
Invoke-RestMethod "$($api.endpoint)/status" -Headers $headers
Invoke-RestMethod "$($api.endpoint)/activity/build" -Method Delete -Headers $headers
```

完整演示见 `scripts/activity-demo.ps1`。
