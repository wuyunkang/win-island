//! Windows 默认使用 Direct3D 12 平稳提交帧；图形初始化失败时回退另一后端。
fn configured_renderer() -> eframe::Renderer {
    let value = std::env::var("ISLAND_RENDERER")
        .or_else(|_| std::env::var("DAOZAI_RENDERER"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    match value.trim() {
        "glow" | "opengl" => eframe::Renderer::Glow,
        "wgpu" | "dx12" => eframe::Renderer::Wgpu,
        _ => {
            if cfg!(windows) {
                eframe::Renderer::Wgpu
            } else {
                eframe::Renderer::Glow
            }
        }
    }
}
fn memory_efficient_device(
    mut descriptor: eframe::wgpu::DeviceDescriptor<'static>,
) -> eframe::wgpu::DeviceDescriptor<'static> {
    descriptor.memory_hints = eframe::wgpu::MemoryHints::MemoryUsage;
    descriptor
}
fn options_for(
    mut options: eframe::NativeOptions,
    renderer: eframe::Renderer,
) -> eframe::NativeOptions {
    options.renderer = renderer;
    if renderer == eframe::Renderer::Wgpu {
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup
        {
            setup.instance_descriptor.backends = if cfg!(windows) {
                eframe::wgpu::Backends::DX12
            } else {
                eframe::wgpu::Backends::VULKAN
            };
            setup.power_preference = eframe::wgpu::PowerPreference::HighPerformance;
            let descriptor = setup.device_descriptor.clone();
            setup.device_descriptor =
                std::sync::Arc::new(move |adapter| memory_efficient_device(descriptor(adapter)));
        }
    }
    options
}
pub fn run(
    options: eframe::NativeOptions,
    mut launch: impl FnMut(eframe::NativeOptions) -> eframe::Result<()>,
) -> eframe::Result<()> {
    let renderer = configured_renderer();
    let alternative = if renderer == eframe::Renderer::Wgpu {
        eframe::Renderer::Glow
    } else {
        eframe::Renderer::Wgpu
    };
    let fallback = options_for(options.clone(), alternative);
    let error = match launch(options_for(options, renderer)) {
        Ok(()) => return Ok(()),
        Err(
            error @ (eframe::Error::Wgpu(_)
            | eframe::Error::OpenGL(_)
            | eframe::Error::Glutin(_)
            | eframe::Error::NoGlutinConfigs(..)),
        ) => error,
        Err(error) => return Err(error),
    };
    launch(fallback).map_err(|fallback|eframe::Error::AppCreation(format!("首选图形后端初始化失败：{error}\n备用图形后端也无法启动：{fallback}\n请安装电脑厂商提供的显卡驱动后重试。").into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_efficient_device_keeps_features_and_rendering_limits() {
        let mut descriptor = eframe::wgpu::DeviceDescriptor::default();
        descriptor.label = Some("memory-test");
        descriptor.required_limits.max_texture_dimension_2d = 8192;
        let descriptor = memory_efficient_device(descriptor);
        assert!(matches!(descriptor.memory_hints,eframe::wgpu::MemoryHints::MemoryUsage));
        assert_eq!(descriptor.required_limits.max_texture_dimension_2d, 8192);
        assert_eq!(descriptor.label, Some("memory-test"));
    }
    #[test]
    #[cfg(windows)]
    fn review_windows_prefers_direct3d_for_even_frame_delivery() {
        run(eframe::NativeOptions::default(), |options| {
            assert_eq!(options.renderer, eframe::Renderer::Wgpu);
            let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = options.wgpu_options.wgpu_setup
            else {
                panic!("缺少图形设置")
            };
            assert_eq!(
                setup.instance_descriptor.backends,
                eframe::wgpu::Backends::DX12
            );
            assert_eq!(
                setup.power_preference,
                eframe::wgpu::PowerPreference::HighPerformance
            );
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn direct3d_failure_retries_opengl() {
        let mut attempts = 0;
        run(eframe::NativeOptions::default(), |options| {
            attempts += 1;
            if attempts == 1 {
                return Err(eframe::Error::Wgpu(
                    eframe::egui_wgpu::WgpuError::NoSuitableAdapterFound("测试图形设备".into()),
                ));
            }
            assert_eq!(options.renderer, eframe::Renderer::Glow);
            Ok(())
        })
        .unwrap();
        assert_eq!(attempts, 2);
    }
    #[test]
    fn opengl_can_fall_back_to_a_backend_without_gl() {
        let options = options_for(Default::default(), eframe::Renderer::Wgpu);
        let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = options.wgpu_options.wgpu_setup else {
            panic!("缺少图形设置")
        };
        assert!(!setup
            .instance_descriptor
            .backends
            .contains(eframe::wgpu::Backends::GL));
    }
    #[test]
    fn application_errors_do_not_restart_the_application() {
        let mut attempts = 0;
        assert!(run(eframe::NativeOptions::default(), |_| {
            attempts += 1;
            Err(eframe::Error::AppCreation("应用初始化失败".into()))
        })
        .is_err());
        assert_eq!(attempts, 1);
    }
    #[test]
    fn successful_run_is_not_restarted() {
        let mut attempts = 0;
        run(eframe::NativeOptions::default(), |_| {
            attempts += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(attempts, 1);
    }
    #[test]
    fn both_failures_are_preserved_in_the_report() {
        let mut attempts = 0;
        let error = run(eframe::NativeOptions::default(), |_| {
            attempts += 1;
            if attempts == 1 {
                Err(eframe::Error::Wgpu(
                    eframe::egui_wgpu::WgpuError::NoSuitableAdapterFound("Direct3D 12".into()),
                ))
            } else {
                Err(eframe::Error::OpenGL("OpenGL 2.0".to_owned().into()))
            }
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("Direct3D 12") && error.contains("OpenGL 2.0"));
        assert_eq!(attempts, 2);
    }
}
