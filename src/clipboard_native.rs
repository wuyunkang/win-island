//! 用户点击或拖放后才执行的本机剪贴板操作。
use crate::clipboard::Clip;
use std::path::PathBuf;
use windows::Win32::{
    Foundation::HWND,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

fn mark() -> Result<(), String> {
    let format = clipboard_win::register_format("LingDongIsland.ClipboardHistoryCopy")
        .ok_or("无法注册剪贴板格式")?;
    clipboard_win::raw::set_without_clear(format.get(), &[1]).map_err(|e| e.to_string())
}
pub fn copy_clip(clip: &Clip, plain: bool) -> Result<(), String> {
    if !plain && !clip.files.is_empty() {
        return copy_files(&clip.files);
    }
    if let Some(picture) = &clip.picture {
        let image = image::load_from_memory(picture.png.as_slice())
            .map_err(|e| e.to_string())?
            .to_rgba8();
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard
            .set_image(arboard::ImageData {
                width: image.width() as usize,
                height: image.height() as usize,
                bytes: std::borrow::Cow::Owned(image.into_raw()),
            })
            .map_err(|e| e.to_string())?;
        drop(clipboard);
        let _clipboard = clipboard_win::Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
        return mark();
    }
    let _clipboard = clipboard_win::Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
    clipboard_win::raw::set_string(&clip.plain_text()).map_err(|e| e.to_string())?;
    if !plain {
        for (name, text) in [("HTML Format", &clip.html), ("Rich Text Format", &clip.rtf)] {
            if !text.is_empty() {
                if let Some(format) = clipboard_win::register_format(name) {
                    let mut bytes = text.as_bytes().to_vec();
                    bytes.push(0);
                    clipboard_win::raw::set_without_clear(format.get(), &bytes)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    mark()
}
pub fn copy_files(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() || paths.iter().any(|p| !p.exists()) {
        return Err("部分文件已不存在，未改动剪贴板；请在详情中检查路径".into());
    }
    let paths: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    if paths.is_empty() {
        return Err("文件已不存在，未改动剪贴板".into());
    }
    let _clipboard = clipboard_win::Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
    clipboard_win::raw::set_file_list(&paths).map_err(|e| e.to_string())?;
    mark()
}

#[derive(Default)]
pub struct Target {
    window: isize,
    pid: u32,
}
impl Target {
    pub fn remember(&mut self) {
        unsafe {
            let hwnd = GetForegroundWindow();
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != 0 && pid != std::process::id() {
                self.window = hwnd.0 as isize;
                self.pid = pid;
            }
        }
    }
    pub fn paste(&self) -> Result<(), String> {
        unsafe {
            let hwnd = HWND(self.window as *mut _);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if self.window == 0 || pid != self.pid || !IsWindow(hwnd).as_bool() {
                return Err("已复制，请先选择要粘贴的窗口".into());
            }
            if !SetForegroundWindow(hwnd).as_bool() {
                return Err("已复制，目标窗口暂时无法自动粘贴，可按 Ctrl+V".into());
            }
            let held = GetAsyncKeyState(VK_CONTROL.0 as i32) < 0;
            let mut inputs = vec![];
            let key = |code: VIRTUAL_KEY, up: bool| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: code,
                        dwFlags: if up {
                            KEYEVENTF_KEYUP
                        } else {
                            KEYBD_EVENT_FLAGS(0)
                        },
                        ..Default::default()
                    },
                },
            };
            if !held {
                inputs.push(key(VK_CONTROL, false));
            }
            inputs.push(key(VIRTUAL_KEY(b'V' as u16), false));
            inputs.push(key(VIRTUAL_KEY(b'V' as u16), true));
            if !held {
                inputs.push(key(VK_CONTROL, true));
            }
            if SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) != inputs.len() as u32 {
                return Err("已复制，系统阻止自动粘贴，请按 Ctrl+V".into());
            }
            Ok(())
        }
    }
}
pub fn drag_clip(clip: &Clip) -> Result<(), String> {
    copy_clip(clip, false)?;
    unsafe {
        use windows::Win32::System::Ole::*;
        let initialized = OleInitialize(None).is_ok();
        let result = OleGetClipboard().and_then(|data| {
            windows::Win32::UI::Shell::SHDoDragDrop(None, &data, None, DROPEFFECT_COPY).map(|_| ())
        });
        if initialized {
            OleUninitialize();
        }
        result.map_err(|e| e.to_string())
    }
}
