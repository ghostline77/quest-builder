//! Windows input used only for CET's toolbar, which has no Lua reload API.
use anyhow::{Result, bail, ensure};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::{ptr, thread, time::Duration};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT},
    Graphics::Gdi::ClientToScreen,
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
        },
        WindowsAndMessaging::{
            EnumWindows, GetForegroundWindow, GetWindowTextW, SetCursorPos, SetForegroundWindow,
        },
    },
};

fn window() -> Result<HWND> {
    unsafe extern "system" fn visit(hwnd: HWND, out: LPARAM) -> i32 {
        let mut title = [0u16; 256];
        // SAFETY: title is writable and its capacity matches the supplied length.
        let len = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
        if String::from_utf16_lossy(&title[..len.max(0) as usize]).starts_with("Cyberpunk 2077") {
            // SAFETY: EnumWindows receives a live, uniquely borrowed HWND pointer below.
            unsafe {
                *(out as *mut HWND) = hwnd;
            }
            return 0;
        }
        1
    }
    let mut hwnd: HWND = ptr::null_mut();
    // SAFETY: enumeration is synchronous; the output pointer lives until it returns.
    unsafe {
        EnumWindows(Some(visit), &mut hwnd as *mut HWND as LPARAM);
    }
    ensure!(!hwnd.is_null(), "Cyberpunk 2077 window not found");
    Ok(hwnd)
}

pub fn focus() -> Result<()> {
    let hwnd = window()?;
    // SAFETY: hwnd was obtained from FindWindowW; no borrowed pointers escape.
    unsafe {
        SetForegroundWindow(hwnd);
    }
    thread::sleep(Duration::from_millis(200));
    ensure!(
        unsafe { GetForegroundWindow() } == hwnd,
        "Game did not receive focus"
    );
    Ok(())
}

pub fn is_running() -> bool {
    window().is_ok()
}

pub fn game_process() -> Result<OwnedHandle> {
    let mut id = 0;
    // SAFETY: id is writable and the window handle was obtained by enumeration.
    ensure!(
        unsafe { GetWindowThreadProcessId(window()?, &mut id) } != 0,
        "Cannot identify game process"
    );
    // SAFETY: opening a synchronize-only handle does not mutate the process.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, id) };
    ensure!(!handle.is_null(), "Cannot wait on game process");
    // SAFETY: OpenProcess returned a new, uniquely owned, non-null handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

pub fn wait_exit(process: &OwnedHandle) -> Result<()> {
    // SAFETY: the borrowed process handle remains valid throughout the wait.
    ensure!(
        unsafe { WaitForSingleObject(process.as_raw_handle(), 60_000) } == 0,
        "Game process did not exit; archives may still be open"
    );
    Ok(())
}

fn send(input: INPUT) -> Result<()> {
    // SAFETY: input is a fully initialized Win32 INPUT with matching discriminant.
    ensure!(
        unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) } == 1,
        "Windows rejected input"
    );
    Ok(())
}

pub fn key(binding: u64) -> Result<()> {
    let keys: Vec<u16> = (0..4)
        .rev()
        .map(|n| ((binding >> (n * 16)) & 0xffff) as u16)
        .filter(|k| *k != 0)
        .collect();
    if keys.is_empty() {
        bail!("CET overlay key is unbound");
    }
    focus()?;
    for released in [false, true] {
        let ordered: Vec<_> = if released {
            keys.iter().rev().copied().collect()
        } else {
            keys.clone()
        };
        for key in ordered {
            send(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: key,
                        wScan: 0,
                        dwFlags: if released { KEYEVENTF_KEYUP } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            })?;
        }
        thread::sleep(Duration::from_millis(70));
    }
    Ok(())
}

pub fn click(x: i32, y: i32) -> Result<()> {
    focus()?;
    let mut p = POINT { x, y };
    // SAFETY: p is writable; HWND refers to the game client area.
    ensure!(
        unsafe { ClientToScreen(window()?, &mut p) } != 0,
        "Cannot locate game client area"
    );
    ensure!(
        unsafe { SetCursorPos(p.x, p.y) } != 0,
        "Cannot position mouse"
    );
    thread::sleep(Duration::from_millis(100));
    for flags in [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP] {
        send(INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        })?;
        thread::sleep(Duration::from_millis(80));
    }
    Ok(())
}
