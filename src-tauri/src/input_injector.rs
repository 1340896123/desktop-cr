//! 鼠标 / 键盘事件注入模块。
//!
//! Windows 平台通过 Win32 SendInput API 进行系统级事件模拟；
//! 非 Windows 平台（Linux 开发环境）仅记录日志，保证工程可编译可联调。

use serde::Serialize;

/// DOM KeyboardEvent.code(或按键文本)→ 虚拟键码(VK),纯函数、无平台依赖。
///
/// 返回 VK 数值(如 KeyA→0x41);无法映射返回 None。
/// 非 Windows 非测试构建下仅被测试引用,允许 dead_code。
#[cfg_attr(all(not(target_os = "windows"), not(test)), allow(dead_code))]
pub(crate) fn code_to_vk(code: &str) -> Option<u16> {
    // 小键盘数字与运算符
    let numpad = match code {
        "Numpad0" => Some(0x60),
        "Numpad1" => Some(0x61),
        "Numpad2" => Some(0x62),
        "Numpad3" => Some(0x63),
        "Numpad4" => Some(0x64),
        "Numpad5" => Some(0x65),
        "Numpad6" => Some(0x66),
        "Numpad7" => Some(0x67),
        "Numpad8" => Some(0x68),
        "Numpad9" => Some(0x69),
        "NumpadAdd" => Some(0x6B),
        "NumpadSubtract" => Some(0x6D),
        "NumpadMultiply" => Some(0x6A),
        "NumpadDivide" => Some(0x6F),
        "NumpadDecimal" => Some(0x6E),
        // 小键盘 Enter 与主键盘 Enter 共用 VK_RETURN(0x0D),靠 EXTENDEDKEY 区分
        "NumpadEnter" => Some(0x0D),
        // 控制与编辑键
        "Space" => Some(0x20),
        "Enter" => Some(0x0D),
        "Tab" => Some(0x09),
        "Backspace" => Some(0x08),
        "Escape" => Some(0x1B),
        "Delete" => Some(0x2E),
        "Insert" => Some(0x2D),
        "Home" => Some(0x24),
        "End" => Some(0x23),
        "PageUp" => Some(0x21),
        "PageDown" => Some(0x22),
        // 方向键
        "ArrowUp" => Some(0x26),
        "ArrowDown" => Some(0x28),
        "ArrowLeft" => Some(0x25),
        "ArrowRight" => Some(0x27),
        // 锁定键
        "CapsLock" => Some(0x14),
        "NumLock" => Some(0x90),
        "ScrollLock" => Some(0x91),
        // 修饰键(右 Control / 右 Alt 靠 EXTENDEDKEY 区分左右)
        "ControlLeft" => Some(0xA2),
        "ControlRight" => Some(0xA3),
        "ShiftLeft" => Some(0xA0),
        "ShiftRight" => Some(0xA1),
        "AltLeft" => Some(0xA4),
        "AltRight" => Some(0xA5),
        // Win 键
        "MetaLeft" => Some(0x5B),
        "MetaRight" => Some(0x5C),
        // 符号键
        "Minus" => Some(0xBD),
        "Equal" => Some(0xBB),
        "BracketLeft" => Some(0xDB),
        "BracketRight" => Some(0xDD),
        "Semicolon" => Some(0xBA),
        "Quote" => Some(0xDE),
        "Backquote" => Some(0xC0),
        "Comma" => Some(0xBC),
        "Period" => Some(0xBE),
        "Slash" => Some(0xBF),
        "Backslash" => Some(0xDC),
        "IntlBackslash" => Some(0xDC),
        "ContextMenu" => Some(0x5D),
        _ => None,
    };
    if let Some(vk) = numpad {
        return Some(vk);
    }

    // 字母区 A-Z(如 "KeyA")
    if let Some(rest) = code.strip_prefix("Key") {
        if rest.len() == 1 {
            let ch = rest.as_bytes()[0];
            if ch.is_ascii_alphabetic() {
                return Some(0x41 + (ch.to_ascii_uppercase() - b'A') as u16);
            }
        }
    }
    // 数字区 0-9(如 "Digit9")
    if let Some(rest) = code.strip_prefix("Digit") {
        if rest.len() == 1 {
            let ch = rest.as_bytes()[0];
            if ch.is_ascii_digit() {
                return Some(0x30 + (ch - b'0') as u16);
            }
        }
    }

    // 功能键 F1-F24(VK_F1..VK_F24 连续)
    if let Some(n) = code.strip_prefix('F').and_then(|s| s.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x70 + n - 1);
        }
    }

    // 兜底:单个可打印字符(按键文本,如 "a" / "5");多字符串(如 "Foo")不映射
    if code.chars().count() == 1 {
        let ch = code.chars().next()?;
        if ch.is_ascii_alphabetic() {
            let base = if ch.is_ascii_lowercase() { b'a' } else { b'A' };
            return Some(0x41 + (ch as u8 - base) as u16);
        }
        if ch.is_ascii_digit() {
            return Some(0x30 + (ch as u8 - b'0') as u16);
        }
    }
    None
}

/// 该 DOM code 是否为扩展键(需 KEYEVENTF_EXTENDEDKEY 标志),纯函数无平台依赖。
#[cfg_attr(all(not(target_os = "windows"), not(test)), allow(dead_code))]
pub(crate) fn is_extended_code(code: &str) -> bool {
    matches!(
        code,
        "NumpadEnter"
            | "Delete"
            | "Insert"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "ArrowUp"
            | "ArrowDown"
            | "ArrowLeft"
            | "ArrowRight"
            | "ControlRight"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
    )
}

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

/// Windows 平台真实鼠标注入(network 协议层也直接调用本函数)。
#[cfg(target_os = "windows")]
pub(crate) fn inject_mouse_event_windows(
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
                dwFlags: MOUSE_EVENT_FLAGS(
                    flags.0 | MOUSEEVENTF_ABSOLUTE.0 | MOUSEEVENTF_VIRTUALDESK.0,
                ),
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

/// Windows 平台真实键盘注入(network 协议层也直接调用本函数)。
#[cfg(target_os = "windows")]
pub(crate) fn inject_key_event_windows(
    key: &str,
    event_type: &str,
    code: Option<&str>,
    modifiers: &[String],
) -> Result<InputEventReceipt, String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    // DOM KeyboardEvent.code → 虚拟键码(纯函数 code_to_vk,含字母/数字/符号/F 键/小键盘/修饰键);
    // 方向键、右 Control/右 Alt、Win 键、编辑键等扩展键必须带 EXTENDEDKEY 标志(由 is_extended_code 判定)。
    let code = code
        .map(|c| c.to_string())
        .unwrap_or_else(|| key.to_string());
    let vk_num = code_to_vk(&code)
        .or_else(|| code_to_vk(key))
        .unwrap_or_else(|| {
            log::warn!(
                "[input] 无法映射按键 code={code}, key={key}, modifiers={modifiers:?}，忽略本次注入"
            );
            0
        });
    if vk_num == 0 {
        return Ok(InputEventReceipt {
            ok: false,
            message: format!("unmappable key: code={code}, key={key}"),
        });
    }
    let vk = VIRTUAL_KEY(vk_num);
    let extended = is_extended_code(&code);

    // 组装并发送按键事件（keydown 无标志，keyup 带 KEYEVENTF_KEYUP）
    let mut flags = if extended {
        KEYEVENTF_EXTENDEDKEY
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    if event_type == "keyup" {
        flags |= KEYEVENTF_KEYUP;
    }

    let ki = KEYBDINPUT {
        wVk: vk,
        wScan: 0,
        dwFlags: flags,
        time: 0,
        dwExtraInfo: 0,
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    };

    // SAFETY: SendInput 接收指向 INPUT 数组的指针，input 在本调用内保持存活。
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }

    log::info!(
        "[input] 注入键盘事件 {event_type} code={code} vk={:#04x} extended={extended}",
        vk.0
    );
    Ok(InputEventReceipt {
        ok: true,
        message: "key event sent".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_to_vk_mapping() {
        assert_eq!(code_to_vk("KeyA"), Some(0x41));
        assert_eq!(code_to_vk("Digit9"), Some(0x39));
        assert_eq!(code_to_vk("F12"), Some(0x7B));
        assert_eq!(code_to_vk("Space"), Some(0x20));
        assert_eq!(code_to_vk("ArrowUp"), Some(0x26));
        assert_eq!(code_to_vk("ControlLeft"), Some(0xA2));
        assert_eq!(code_to_vk("MetaRight"), Some(0x5C));
        assert_eq!(code_to_vk("NumpadEnter"), Some(0x0D));
        assert_eq!(code_to_vk("Foo"), None);
    }
}
