//! 图文与文件剪贴板监听和系统交互。
pub use crate::clipboard_data::*;
use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
// ---------------------------------------------------------------- Windows 监听

#[cfg(windows)]
pub fn watch(shared: Shared, repaint: impl Fn() + Send + 'static) {
    std::thread::Builder::new().name("island-clipboard".into()).spawn(move || {
        // 启动失败（极少见）时剪贴板页只是没有新记录，不影响其他功能。
        let _ = win::run(shared, Box::new(repaint));
    }).ok();
}

#[cfg(not(windows))]
pub fn watch(_shared: Shared, _repaint: impl Fn() + Send + 'static) {}

#[cfg(windows)]
const HISTORY_COPY_FORMAT: &str = "LingDongIsland.ClipboardHistoryCopy";

#[cfg(windows)]
pub fn copy_history(text: &str) -> bool {
    use clipboard_win::{Clipboard, raw, register_format};
    let Some(format) = register_format(HISTORY_COPY_FORMAT) else { return false };
    let Ok(_clipboard) = Clipboard::new_attempts(10) else { return false };
    raw::set_string(text).is_ok() && raw::set_without_clear(format.get(), &[1]).is_ok()
}

#[cfg(windows)]
mod win {
    use super::Shared;
    use std::cell::RefCell;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::UI::WindowsAndMessaging::*;

    const WM_CLIPBOARDUPDATE: u32 = 0x031D;

    struct State { shared: Shared, repaint: Box<dyn Fn() + Send> }
    thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_CLIPBOARDUPDATE {
            STATE.with(|state| {
                if let Some(state) = state.borrow().as_ref() {
                    if state.shared.is_enabled() {
                        if read_content(hwnd,&state.shared) { (state.repaint)(); }
                    }
                }
            });
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }

    /// 密码管理器、远程桌面等会给敏感内容加上这些格式，表示不要进入剪贴板历史。
    unsafe fn excluded() -> bool {
        let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        let ignore = RegisterClipboardFormatW(w!("Clipboard Viewer Ignore"));
        if (exclude != 0 && IsClipboardFormatAvailable(exclude).is_ok()) || (ignore != 0 && IsClipboardFormatAvailable(ignore).is_ok()) {
            return true;
        }
        // 值为 DWORD 0 表示不允许进入历史。
        let history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
        if history == 0 || IsClipboardFormatAvailable(history).is_err() { return false; }
        let Ok(handle) = GetClipboardData(history) else { return true; };
        let memory = HGLOBAL(handle.0);
        let ptr = GlobalLock(memory) as *const u32;
        if ptr.is_null() { return true; }
        let allowed = GlobalSize(memory) >= 4 && *ptr != 0;
        let _ = GlobalUnlock(memory);
        !allowed
    }

    unsafe fn read_text(hwnd: HWND) -> Option<(String, bool)> {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() { return None; }
        // 其他程序可能正占用剪贴板，稍等重试几次。
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(hwnd).is_ok() { opened = true; break; }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !opened { return None; }
        let text = (|| {
            if excluded() { return None; }
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let memory = HGLOBAL(handle.0);
            let ptr = GlobalLock(memory) as *const u16;
            if ptr.is_null() { return None; }
            let max = (GlobalSize(memory) / 2).min(super::MAX_CHARS * 2 + 1);
            let mut len = 0;
            while len < max && *ptr.add(len) != 0 { len += 1; }
            let text = (len <= super::MAX_CHARS * 2).then(|| String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)));
            let _ = GlobalUnlock(memory);
            let from_history = clipboard_win::register_format(super::HISTORY_COPY_FORMAT)
                .is_some_and(|format| IsClipboardFormatAvailable(format.get()).is_ok());
            text.map(|text| (text, from_history))
        })();
        let _ = CloseClipboard();
        text
    }

    unsafe fn source_app()->String {
        use windows::Win32::System::Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION,QueryFullProcessImageNameW,PROCESS_NAME_WIN32};
        use windows::Win32::Foundation::CloseHandle;
        let Ok(owner)=GetClipboardOwner()else{return String::new();};let mut pid=0;GetWindowThreadProcessId(owner,Some(&mut pid));
        let Ok(process)=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,pid)else{return String::new();};
        let mut path=[0u16;32768];let mut len=path.len() as u32;let result=QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,windows::core::PWSTR(path.as_mut_ptr()),&mut len);let _=CloseHandle(process);
        if result.is_err(){return String::new();}std::path::Path::new(&String::from_utf16_lossy(&path[..len as usize])).file_name().unwrap_or_default().to_string_lossy().to_string()
    }
    unsafe fn registered_text(name:&str)->String{
        let wide:Vec<u16>=name.encode_utf16().chain(Some(0)).collect();let format=RegisterClipboardFormatW(PCWSTR(wide.as_ptr()));
        let Ok(handle)=GetClipboardData(format)else{return String::new();};let memory=HGLOBAL(handle.0);let pointer=GlobalLock(memory) as *const u8;
        if pointer.is_null(){return String::new();}let size=GlobalSize(memory).min(2*1024*1024);let bytes=std::slice::from_raw_parts(pointer,size);let len=bytes.iter().position(|b|*b==0).unwrap_or(size);let text=String::from_utf8_lossy(&bytes[..len]).to_string();let _=GlobalUnlock(memory);text
    }
    pub(super) unsafe fn read_content(hwnd:HWND,shared:&Shared)->bool{
        let source=source_app();if !shared.allows_source(&source){return false;}
        if OpenClipboard(hwnd).is_err(){return false;}
        let own=clipboard_win::register_format(super::HISTORY_COPY_FORMAT).is_some_and(|f|IsClipboardFormatAvailable(f.get()).is_ok());
        if own||excluded(){let _=CloseClipboard();return false;}
        // 资源管理器同时提供文件和路径文字时，优先保留真实文件格式。
        if IsClipboardFormatAvailable(windows::Win32::System::Ole::CF_HDROP.0 as u32).is_ok(){
            let mut paths=Vec::new();let read=clipboard_win::raw::get_file_list_path(&mut paths);
            let _=CloseClipboard();return read.is_ok()&&shared.record_files(paths,&source);
        }
        let image=IsClipboardFormatAvailable(windows::Win32::System::Ole::CF_DIB.0 as u32).is_ok()||IsClipboardFormatAvailable(windows::Win32::System::Ole::CF_DIBV5.0 as u32).is_ok();
        let html=registered_text("HTML Format");let rtf=registered_text("Rich Text Format");let _=CloseClipboard();
        if image&&shared.options.lock().is_ok_and(|o|o.images){
            if let Ok(mut clipboard)=arboard::Clipboard::new(){if let Ok(data)=clipboard.get_image(){
                if data.width.checked_mul(data.height).is_some_and(|pixels|pixels<=16_000_000){
                    if let Some(buffer)=image::RgbaImage::from_raw(data.width as u32,data.height as u32,data.bytes.into_owned()){
                        let mut png=std::io::Cursor::new(Vec::new());if image::DynamicImage::ImageRgba8(buffer).write_to(&mut png,image::ImageFormat::Png).is_ok(){return shared.record_picture(super::Picture{width:data.width as u32,height:data.height as u32,png:std::sync::Arc::new(png.into_inner())},&source);}
                    }
                }
            }}
        }
        read_text(hwnd).is_some_and(|(text,from_history)|!from_history&&shared.record_source(&text,&source,&html,&rtf))
    }

    pub fn run(shared: Shared, repaint: Box<dyn Fn() + Send>) -> windows::core::Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("LingDongIslandClipboardWatcher");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, PCWSTR::null(), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, instance, None)?;
            STATE.with(|state| *state.borrow_mut() = Some(State { shared, repaint }));
            AddClipboardFormatListener(hwnd)?;
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = RemoveClipboardFormatListener(hwnd);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    #[ignore="本机剪贴板往返测试，完成后恢复原有数据对象"]
    fn native_file_clipboard_round_trip_and_watcher_priority(){
        use windows::Win32::{Foundation::HWND,System::Ole::*};
        struct Restore(Option<windows::Win32::System::Com::IDataObject>);
        impl Drop for Restore{fn drop(&mut self){unsafe{let _=OleSetClipboard(self.0.as_ref());let _=OleFlushClipboard();OleUninitialize();}}}
        unsafe{OleInitialize(None).unwrap();}
        let _restore=Restore(unsafe{OleGetClipboard().ok()});
        let directory=std::env::temp_dir().join(format!("island-copied-files-{}",std::process::id()));std::fs::create_dir_all(&directory).unwrap();
        let paths=vec![directory.join("中文文件.txt"),directory.join("第二项.txt")];for path in &paths{std::fs::write(path,"fixture").unwrap();}
        let shared=Shared{history:Default::default(),enabled:Arc::new(AtomicBool::new(true)),options:Default::default()};
        {let _open=clipboard_win::Clipboard::new_attempts(10).unwrap();clipboard_win::raw::set_file_list(&paths.iter().map(|p|p.to_string_lossy().to_string()).collect::<Vec<_>>()).unwrap();
         clipboard_win::raw::set_without_clear(windows::Win32::System::Ole::CF_UNICODETEXT.0 as u32,&[b'x',0,0,0]).unwrap();}
        assert!(unsafe{win::read_content(HWND::default(),&shared)});let clip=shared.lock().items[0].clone();assert_eq!(clip.files,paths);assert_eq!(clip.kind,crate::transfer::Kind::Files);
        crate::clipboard_native::copy_clip(&clip,false).unwrap();
        {let _open=clipboard_win::Clipboard::new_attempts(10).unwrap();let mut files=Vec::new();clipboard_win::raw::get_file_list_path(&mut files).unwrap();assert_eq!(files,paths);}
        assert!(!unsafe{win::read_content(HWND::default(),&shared)},"岛内重新复制不能重复写入历史");
        crate::clipboard_native::copy_clip(&clip,true).unwrap();
        {let _open=clipboard_win::Clipboard::new_attempts(10).unwrap();let mut text=Vec::new();clipboard_win::raw::get_string(&mut text).unwrap();assert_eq!(String::from_utf8(text).unwrap(),clip.plain_text());}
        std::fs::remove_file(&paths[0]).unwrap();assert!(crate::clipboard_native::copy_clip(&clip,false).is_err());
        {let _open=clipboard_win::Clipboard::new_attempts(10).unwrap();let mut text=Vec::new();clipboard_win::raw::get_string(&mut text).unwrap();assert_eq!(String::from_utf8(text).unwrap(),clip.plain_text(),"缺失文件不能导致只粘贴部分文件或清空剪贴板");}
        std::fs::remove_file(&paths[1]).unwrap();std::fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn history_copy_preserves_order_but_external_copy_promotes() {
        let shared = Shared { history: Default::default(), enabled: Arc::new(AtomicBool::new(true)), options:Default::default() };
        shared.lock().items.clear();
        shared.record("较早的一条");
        shared.record("最近的一条");
        let before = shared.lock().items.clone();
        assert!(!shared.record_update("较早的一条", true));
        assert_eq!(shared.lock().items, before);
        assert!(shared.record_update("较早的一条", false));
        assert_eq!(shared.lock().items[0].text, "较早的一条");
    }
    #[test]
    fn history_dedupes_caps_and_keeps_pins() {
        let mut h = History::default();
        assert!(!h.push("   \n", "09:00"), "空白不记录");
        assert!(h.push("a", "09:00") && h.push("b", "09:01"));
        assert!(!h.push("b", "09:02"), "与最新一条相同不重复记录");
        assert!(h.push("a", "09:03"));
        assert_eq!(h.items.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(h.items[0].at, "09:03");
        h.items[1].pinned = true; // b
        for i in 0..(MAX_RECENT + 5) { h.push(&format!("n{i}"), "10:00"); }
        assert_eq!(h.items.iter().filter(|c| !c.pinned).count(), MAX_RECENT);
        assert!(h.items.iter().any(|c| c.text == "b" && c.pinned), "固定的不会被挤掉");
        h.clear_recent();
        assert_eq!(h.items.len(), 1);
        assert!(!h.push(&"x".repeat(MAX_CHARS + 1), "10:00"), "超长内容不记录");
    }

    #[test]
    fn reopening_restores_history_and_respects_manual_cleanup() {
        let path = std::env::temp_dir().join(format!("island-clip-{}.json", std::process::id()));
        std::fs::write(&path, br#"[{"text":"old","at":"08:00"}]"#).unwrap();
        let mut history = History::load_from(path.clone());
        assert!(history.items[0].pinned);
        history.push("保留", "09:00");
        history.push("删除", "09:01");
        history.remove(0);
        let mut reopened = History::load_from(path.clone());
        assert_eq!(reopened.items, history.items);
        reopened.clear_recent();
        let final_history = History::load_from(path.clone());
        assert_eq!(final_history.items.len(), 1);
        assert_eq!(final_history.items[0].text, "old");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn pinned_round_trip_and_preview() {
        let mut h = History::default();
        h.push("第一行\n第二行  末尾", "08:00");
        h.push("临时", "08:01");
        h.items[1].pinned = true;
        let back = History::from_json(&h.to_json());
        assert_eq!(back.items, h.items);
        assert_eq!(preview(&back.items[1].text), "第一行 第二行 末尾");
        assert!(History::from_json(b"not json").items.is_empty());
    }
}
