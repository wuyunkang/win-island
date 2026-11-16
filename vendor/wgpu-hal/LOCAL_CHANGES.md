# 本地修改

基于 crates.io 的 wgpu-hal 24.0.4，保留原始许可证。

Direct3D 12 的合成 visual、composition surface handle 和 SwapChainPanel 支持 PreMultiplied；普通 HWND 仍只声明 Opaque。DXGI 转换将预乘／非预乘 alpha 映射到对应模式，避免一律忽略 alpha。其余后端保持上游实现。
