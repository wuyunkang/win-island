# DSH 只读会话桥接

适配灵动岛的 AI 页面，已在 Windows / DeepSeek Harness Desktop **0.2.2-rc.3** 上验证。复用 DSH 现有 Node 进程，不创建 Electron 窗口，不调用模型，也不发送消息或处理审批。

## 安装与使用

在本目录运行 `powershell -ExecutionPolicy Bypass -File .\install.ps1`，或使用 Node.js 22.19 及以上版本运行 `node install.mjs --profile desktop`。桌面 profile 的 Cordis patch 支持热加载，安装后可直接使用。

- 安装器只追加带标记的 `win-island-dsh-bridge` 区块，保留其他配置，并在原 profile 目录保存备份。代码安装到灵动岛数据目录的独立版本文件夹；更新使用新的版本条目，让 Cordis 卸载旧桥接后导入新模块，不需要重启 DSH。
- 在灵动岛「设置 → AI」启用 DSH 来源；「页面管理」分别控制 AI 胶囊与展开页。会话列表中的 DSH 与 Codex 可自动跟随或固定，原有额度账户保持独立。
- 胶囊显示状态与项目，展开查看最近一轮公开消息、最近工具、可用的计划进度和上下文指标；未知指标不推算。首次仅读取摘要时，历史任务的详细状态会在展开后补齐。
- 「返回 DSH」激活已有桌面窗口，具体会话仍在 DSH 内选择。审批、回复与停止任务在 DSH 内进行。

若使用 web profile，可运行 `node install.mjs --profile web`。同一发现文件只连接最后启动的桥接实例，多个 profile 同时使用时，请通过安装参数 `--data-dir` 与灵动岛的发现文件路径设置分别隔离。

## 数据与生命周期

桥接在 `127.0.0.1` 随机端口提供受临时 Bearer 令牌保护的只读快照接口，发现文件默认是 `%APPDATA%\WinIslandRust\dsh-bridge.json`。该文件含本机访问令牌，请勿分享。桥接不读取 DSH 凭据、不提供网页跨域访问，不接受非快照操作。

灵动岛有显示需求时读取摘要，展开后才读取公开正文；候选最多 32 个，正文最多 64 条 / 256 KiB。适配器仅折叠所选会话尾部最多 2048 个事件，排除系统、开发者、推理与工具原始参数／输出，超限显示截断标记。

灵动岛隐藏或最小化后停止读取并释放自己的线程和正文缓存；桥接连续 5 秒没有客户端请求后解除事件订阅并清理缓存。DSH 自己已有的会话和公共查询缓存仍由 DSH 管理。发现文件仅含接口位置、令牌和宿主进程号，不含会话正文。

## 卸载与测试

运行 `node install.mjs --profile desktop --uninstall` 仅移除本桥接区块。也可在 DSH 插件管理中停用该条目。

在源码目录运行 `node --test test/*.test.mjs` 验证只读适配、认证、消息过滤、容量限制、闲置释放以及安装／卸载对原配置的保留。

接口参考：[DSH Agent](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/core/agent)、[Session Query](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/core/session-query)。设计参考：[dsh-island](https://github.com/a961282799-crypto/dsh-island)、[dsh-codeisland-bridge](https://github.com/LeLiu/dsh-codeisland-bridge)。本桥接独立实现，使用本项目 MIT 许可。
