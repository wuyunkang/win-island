//! 本机合成回归探针：模拟创建时未启用透明，检查原生初始化能否恢复。
#[cfg(windows)]
fn main()->eframe::Result<()> {
    use eframe::egui;
    use raw_window_handle::{HasWindowHandle,RawWindowHandle};
    use windows::Win32::{Foundation::*,Graphics::Gdi::*,UI::WindowsAndMessaging::*};
    struct Probe {native:island::native::Window,back:HWND,hwnd:HWND,start:std::time::Instant,checkpoint:std::time::Instant,checked:bool,reset:usize}
    impl Drop for Probe {fn drop(&mut self){unsafe{let _=DestroyWindow(self.back);}}}
    impl eframe::App for Probe {
        fn clear_color(&self,_:&egui::Visuals)->[f32;4]{[0.0;4]}
        fn update(&mut self,ctx:&egui::Context,_:&mut eframe::Frame){
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(60.0,30.0),egui::pos2(260.0,130.0)),25.0,egui::Color32::BLACK);
            });
            self.native.update((0.0,0.0,320.0,160.0),(60.0,30.0,200.0,100.0),None,None,None,ctx.pixels_per_point(),false,false);
            // 隐藏创建的窗口首次显示时可能被系统重新定位，白色测试背景需跟随实际位置和层级。
            unsafe {
                let mut rect=RECT::default();GetWindowRect(self.hwnd,&mut rect).unwrap();
                SetWindowPos(self.back,self.hwnd,rect.left,rect.top,rect.right-rect.left,rect.bottom-rect.top,SWP_NOACTIVATE).unwrap();
            }
            if !self.checked && self.checkpoint.elapsed().as_millis()>=800 {
                unsafe {
                    let mut origin=POINT::default();let _=ClientToScreen(self.hwnd,&mut origin);
                    let dc=GetDC(None);
                    let back_dc=GetDC(self.back);let background=GetPixel(back_dc,10,10).0;ReleaseDC(self.back,back_dc);
                    let outside=GetPixel(dc,origin.x+10,origin.y+10).0;
                    let inside=GetPixel(dc,origin.x+160,origin.y+80).0;
                    let mut feathered=0;
                    for y in 30..56{for x in 60..87{let pixel=GetPixel(dc,origin.x+x,origin.y+y).0;
                        let r=pixel&255;let g=(pixel>>8)&255;let b=(pixel>>16)&255;
                        if (1..255).contains(&r) && r.abs_diff(g)<=1 && r.abs_diff(b)<=1{feathered+=1;}
                    }}
                    ReleaseDC(None,dc);
                    println!("outside={outside:06x} inside={inside:06x} background={background:06x} feathered={feathered}");
                    assert_eq!(outside,0x00ffffff,"透明区域应透出背后的白色窗口");
                    assert_eq!(inside,0,"黑色岛体不能被当作透明色挖掉");
                    assert!(feathered>=10,"圆角应保留半透明抗锯齿像素，不能只有全黑与全白");
                }
                self.checked=true;
            }
            if self.checked && self.reset<4 && self.checkpoint.elapsed().as_millis()>=1100 {
                unsafe {
                    use windows::Win32::Graphics::Dwm::*;
                    DwmEnableBlurBehindWindow(self.hwnd,&DWM_BLURBEHIND{dwFlags:DWM_BB_ENABLE,..Default::default()}).unwrap();
                    if self.reset==3 {
                        let _=ShowWindow(self.hwnd,SW_HIDE);
                        let _=ShowWindow(self.hwnd,SW_SHOWNOACTIVATE);
                    }else{
                        SendMessageW(self.hwnd,[WM_DWMCOMPOSITIONCHANGED,WM_DISPLAYCHANGE,WM_DWMCOLORIZATIONCOLORCHANGED][self.reset],WPARAM(0),LPARAM(0));
                    }
                }
                self.reset+=1;self.checked=false;self.checkpoint=std::time::Instant::now();
            }
            if self.checked && self.reset==4 {ctx.send_viewport_cmd(egui::ViewportCommand::Close);return;}
            assert!(self.start.elapsed().as_secs()<60,"透明合成生命周期检查超时");
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }
    let hidden_start=std::env::args().any(|arg|arg=="--hidden-start");
    let mut opts=eframe::NativeOptions{viewport:egui::ViewportBuilder::default().with_title("灵动岛透明合成回归检查").with_inner_size([320.0,160.0]).with_position([100.0,100.0]).with_decorations(false).with_transparent(std::env::args().any(|arg|arg=="--transparent")).with_visible(!hidden_start).with_active(false),..Default::default()};
    if std::env::args().any(|arg|arg=="--wgpu"){
        opts.renderer=eframe::Renderer::Wgpu;
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup)=&mut opts.wgpu_options.wgpu_setup{setup.instance_descriptor.backends=eframe::wgpu::Backends::DX12;setup.power_preference=eframe::wgpu::PowerPreference::HighPerformance;}
    }
    eframe::run_native("透明合成回归检查",opts,Box::new(move |cc|{
        cc.egui_ctx.set_pixels_per_point(1.0);
        let RawWindowHandle::Win32(raw)=cc.window_handle().unwrap().as_raw() else {unreachable!()};
        let hwnd=HWND(raw.hwnd.get() as *mut _);
        let mut rect=RECT::default();
        let back=unsafe {
            GetWindowRect(hwnd,&mut rect).unwrap();
            let back=CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_LAYERED,windows::core::w!("STATIC"),windows::core::w!(""),WS_POPUP|WS_VISIBLE,rect.left,rect.top,rect.right-rect.left,rect.bottom-rect.top,None,None,None,None).unwrap();
            // 提供确定的白色合成表面，避免被上层 HWND 遮挡的 GDI 控件从未收到绘制。
            let width=rect.right-rect.left;let height=rect.bottom-rect.top;
            let info=BITMAPINFO{bmiHeader:BITMAPINFOHEADER{biSize:std::mem::size_of::<BITMAPINFOHEADER>()as u32,biWidth:width,biHeight:-height,biPlanes:1,biBitCount:32,biCompression:BI_RGB.0,..Default::default()},..Default::default()};
            let dc=CreateCompatibleDC(None);let mut bits=std::ptr::null_mut();
            let bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,None,0).unwrap();
            std::ptr::write_bytes(bits,255,(width*height*4)as usize);let old=SelectObject(dc,bitmap);
            UpdateLayeredWindow(back,None,Some(&POINT{x:rect.left,y:rect.top}),Some(&SIZE{cx:width,cy:height}),dc,Some(&POINT::default()),COLORREF(0),Some(&BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8,BlendFlags:0}),ULW_ALPHA).unwrap();
            SelectObject(dc,old);let _=DeleteObject(bitmap);let _=DeleteDC(dc);back
        };
        let native=island::native::Window::new(cc).unwrap();
        if hidden_start {unsafe{
            use windows::Win32::Graphics::Dwm::*;
            DwmEnableBlurBehindWindow(hwnd,&DWM_BLURBEHIND{dwFlags:DWM_BB_ENABLE,..Default::default()}).unwrap();
            let _=ShowWindow(hwnd,SW_SHOWNOACTIVATE);
        }}
        Ok(Box::new(Probe{native,back,hwnd,start:std::time::Instant::now(),checkpoint:std::time::Instant::now(),checked:false,reset:0}))
    }))
}
#[cfg(not(windows))]
fn main(){}
