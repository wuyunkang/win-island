# 本地修改

基于 crates.io 的 egui-wgpu 0.31.1，保留原始许可证。

Windows 的透明、无重定向位图窗口使用 DirectComposition visual 创建 wgpu surface。按视口保存设备、目标、visual 与窗口的生命周期；普通设置窗口继续使用 HWND surface，关闭视口时同步释放合成对象。

需要配合 vendor/eframe 的透明窗口创建和 vendor/wgpu-hal 的预乘 alpha 支持。环境变量 ISLAND_ALPHA_DIAGNOSTICS 可输出实际 alpha 模式和图形后端用于本机检查。
