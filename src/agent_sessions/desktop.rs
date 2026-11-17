//! 只返回已有 Agent 桌面窗口；不发送消息或执行会话指令。
use super::Source;
#[cfg(windows)]
fn window(source: Source) -> Option<windows::Win32::Foundation::HWND> {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, BOOL, HWND, LPARAM},
            System::Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
            UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible},
        },
    };
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> BOOL {
        let (source, result) = &mut *((param.0) as *mut (Source, Option<HWND>));
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return BOOL(1);
        };
        let mut bytes = [0u16; 4096];
        let mut len = bytes.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(bytes.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if ok {
            let path = String::from_utf16_lossy(&bytes[..len as usize]).to_lowercase();
            let matches = match source {
                Source::Codex => {
                    path.ends_with("\\codex.exe")
                        || (path.ends_with("\\chatgpt.exe") && path.contains("openai.codex_"))
                }
                Source::Dsh => [
                    "\\deepseek harness.exe",
                    "\\dsh-desktop.exe",
                    "\\dsh_desktop.exe",
                ]
                .iter()
                .any(|name| path.ends_with(name)),
            };
            if matches {
                *result = Some(hwnd);
                return BOOL(0);
            }
        }
        BOOL(1)
    }
    let mut result = (source, None);
    unsafe {
        let _ = EnumWindows(
            Some(visit),
            LPARAM((&mut result as *mut (Source, Option<HWND>)) as isize),
        );
    }
    result.1
}
#[cfg(windows)]
pub fn available() -> bool {
    available_for(Source::Codex)
}
#[cfg(windows)]
pub fn available_for(source: Source) -> bool {
    window(source).is_some()
}
#[cfg(windows)]
pub fn activate() -> Result<(), String> {
    activate_for(Source::Codex)
}
#[cfg(windows)]
pub fn activate_for(source: Source) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    let hwnd = window(source).ok_or_else(|| format!("未找到 {} 桌面窗口", source.label()))?;
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            Ok(())
        } else {
            Err(format!("请从任务栏返回 {}", source.label()))
        }
    }
}
#[cfg(not(windows))]
pub fn available() -> bool {
    false
}
#[cfg(not(windows))]
pub fn available_for(_: Source) -> bool {
    false
}
#[cfg(not(windows))]
pub fn activate() -> Result<(), String> {
    Err("当前平台不支持激活 Codex 窗口".into())
}
#[cfg(not(windows))]
pub fn activate_for(_: Source) -> Result<(), String> {
    Err("当前平台不支持激活 Agent 窗口".into())
}
