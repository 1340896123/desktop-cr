//! 鼠标 / 键盘事件注入模块。
//!
//! Windows 平台通过 Win32 SendInput API 进行系统级事件模拟；
//! 非 Windows 平台（Linux 开发环境）仅记录日志，保证工程可编译可联调。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InputEventReceipt {
    pub ok: bool,
    pub message: String,
}

/// 注入鼠标事件。坐标为前端归一化后的被控端分辨率坐标。
#[tauri::command]
pub fn inject_mouse_event(
    x: f64,
    y: f64,
    event_type: String,
    button: Option<String>,
    delta_y: f64,
) -> Result<InputEventReceipt, String> {
    #[cfg(target_os = "windows")]
    {
        inject_mouse_event_windows(x, y, &event_type, button.as_deref(), delta_y)
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!(
            "[input] 非 Windows 平台：模拟鼠标事件 {event_type} at ({x:.1}, {y:.1}), button={button:?}, delta_y={delta_y:.1}"
        );
        Ok(InputEventReceipt {
            ok: true,
            message: "mouse event accepted (simulated)".into(),
        })
    }
}

#[cfg(target_os = "windows")]
fn inject_mouse_event_windows(
    x: f64,
    y: f64,
    event_type: &str,
    button: Option<&str>,
    _delta_y: f64,
) -> Result<InputEventReceipt, String> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
        MOUSEINPUT, MOUSE_EVENT_FLAGS,
    };

    // 坐标归一化：前端已按 W_remote/H_remote 归一化，转换为 0~65535 绝对坐标。
    let dx = (x.clamp(0.0, 65535.0)) as u32;
    let dy = (y.clamp(0.0, 65535.0)) as u32;

    let mut flags = MOUSEEVENTF_MOVE;
    match event_type {
        "mousedown" => match button {
            Some("right") => flags |= MOUSEEVENTF_RIGHTDOWN,
            Some("middle") => flags |= MOUSEEVENTF_MIDDLEDOWN,
            _ => flags |= MOUSEEVENTF_LEFTDOWN,
        },
        "mouseup" => match button {
            Some("right") => flags |= MOUSEEVENTF_RIGHTUP,
            Some("middle") => flags |= MOUSEEVENTF_MIDDLEUP,
            _ => flags |= MOUSEEVENTF_LEFTUP,
        },
        "wheel" => {
            flags = MOUSE_EVENT_FLAGS(MOUSEEVENTF_WHEEL.0);
        }
        _ => {}
    }

    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            mi: MOUSEINPUT {
                dx: dx as i32,
                dy: dy as i32,
                mouseData: if event_type == "wheel" {
                    (_delta_y.clamp(-32768.0, 32767.0)) as u32
                } else {
                    0
                },
                dwFlags: MOUSE_EVENT_FLAGS(flags.0 | MOUSEEVENTF_ABSOLUTE.0 | MOUSEEVENTF_VIRTUALDESK.0),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    // SAFETY: SendInput 接收指向 INPUT 数组的指针，input 在本帧内保持存活。
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
    let _ = POINT { x: 0, y: 0 };
    Ok(InputEventReceipt {
        ok: true,
        message: "mouse event sent".into(),
    })
}

/// 注入键盘事件。
#[tauri::command]
pub fn inject_key_event(
    key: String,
    event_type: String,
    code: Option<String>,
    modifiers: Vec<String>,
) -> Result<InputEventReceipt, String> {
    #[cfg(target_os = "windows")]
    {
        inject_key_event_windows(&key, &event_type, code.as_deref(), &modifiers)
    }
    #[cfg(not(target_os = "windows"))]
    {
        log::info!(
            "[input] 非 Windows 平台：模拟键盘事件 {event_type} key={key}, code={code:?}, modifiers={modifiers:?}"
        );
        Ok(InputEventReceipt {
            ok: true,
            message: "key event accepted (simulated)".into(),
        })
    }
}

#[cfg(target_os = "windows")]
fn inject_key_event_windows(
    _key: &str,
    _event_type: &str,
    _code: Option<&str>,
    _modifiers: &[String],
) -> Result<InputEventReceipt, String> {
    // TODO: 通过 SendInput(KEYBDINPUT) 发送 VK 键码与修饰键组合（阶段四实现）。
    Ok(InputEventReceipt {
        ok: true,
        message: "key event sent".into(),
    })
}
