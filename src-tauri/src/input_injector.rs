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
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_ADD, VK_APPS, VK_BACK,
        VK_CAPITAL, VK_DECIMAL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME,
        VK_INSERT, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MULTIPLY, VK_NEXT,
        VK_NUMLOCK, VK_NUMPAD0, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6,
        VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_PRIOR, VK_RCONTROL,
        VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SCROLL, VK_SPACE, VK_SUBTRACT,
        VK_TAB, VK_UP, VK_0, VK_A,
    };

    // 将 DOM KeyboardEvent.code 映射为 (虚拟键码, 是否需要 KEYEVENTF_EXTENDEDKEY)。
    // 方向键、右 Control/右 Alt、Win 键、编辑键等扩展键必须带 EXTENDEDKEY 标志。
    fn map_code(code: &str) -> Option<(VIRTUAL_KEY, bool)> {
        let vk = |c: &str| -> Option<(VIRTUAL_KEY, bool)> {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9, VK_A, VK_B, VK_C,
                VK_D, VK_E, VK_F, VK_G, VK_H, VK_I, VK_J, VK_K, VK_L, VK_M, VK_N, VK_O, VK_P,
                VK_Q, VK_R, VK_S, VK_T, VK_U, VK_V, VK_W, VK_X, VK_Y, VK_Z,
            };
            let letters = [
                ("KeyA", VK_A),
                ("KeyB", VK_B),
                ("KeyC", VK_C),
                ("KeyD", VK_D),
                ("KeyE", VK_E),
                ("KeyF", VK_F),
                ("KeyG", VK_G),
                ("KeyH", VK_H),
                ("KeyI", VK_I),
                ("KeyJ", VK_J),
                ("KeyK", VK_K),
                ("KeyL", VK_L),
                ("KeyM", VK_M),
                ("KeyN", VK_N),
                ("KeyO", VK_O),
                ("KeyP", VK_P),
                ("KeyQ", VK_Q),
                ("KeyR", VK_R),
                ("KeyS", VK_S),
                ("KeyT", VK_T),
                ("KeyU", VK_U),
                ("KeyV", VK_V),
                ("KeyW", VK_W),
                ("KeyX", VK_X),
                ("KeyY", VK_Y),
                ("KeyZ", VK_Z),
                ("Digit0", VK_0),
                ("Digit1", VK_1),
                ("Digit2", VK_2),
                ("Digit3", VK_3),
                ("Digit4", VK_4),
                ("Digit5", VK_5),
                ("Digit6", VK_6),
                ("Digit7", VK_7),
                ("Digit8", VK_8),
                ("Digit9", VK_9),
            ];
            letters
                .iter()
                .find(|(name, _)| *name == c)
                .map(|(_, vk)| (*vk, false))
        };

        match code {
            // 小键盘数字与运算符
            "Numpad0" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 0), false)),
            "Numpad1" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 1), false)),
            "Numpad2" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 2), false)),
            "Numpad3" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 3), false)),
            "Numpad4" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 4), false)),
            "Numpad5" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 5), false)),
            "Numpad6" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 6), false)),
            "Numpad7" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 7), false)),
            "Numpad8" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 8), false)),
            "Numpad9" => Some((VIRTUAL_KEY(VK_NUMPAD0.0 + 9), false)),
            "NumpadAdd" => Some((VK_ADD, false)),
            "NumpadSubtract" => Some((VK_SUBTRACT, false)),
            "NumpadMultiply" => Some((VK_MULTIPLY, false)),
            "NumpadDivide" => Some((VK_DIVIDE, false)),
            "NumpadDecimal" => Some((VK_DECIMAL, false)),
            // 小键盘 Enter 与主键盘 Enter 共用 VK_RETURN，靠 EXTENDEDKEY 区分
            "NumpadEnter" => Some((VK_RETURN, true)),
            // 控制与编辑键
            "Space" => Some((VK_SPACE, false)),
            "Enter" => Some((VK_RETURN, false)),
            "Tab" => Some((VK_TAB, false)),
            "Backspace" => Some((VK_BACK, false)),
            "Escape" => Some((VK_ESCAPE, false)),
            "Delete" => Some((VK_DELETE, true)),
            "Insert" => Some((VK_INSERT, true)),
            "Home" => Some((VK_HOME, true)),
            "End" => Some((VK_END, true)),
            "PageUp" => Some((VK_PRIOR, true)),
            "PageDown" => Some((VK_NEXT, true)),
            // 方向键（扩展键）
            "ArrowUp" => Some((VK_UP, true)),
            "ArrowDown" => Some((VK_DOWN, true)),
            "ArrowLeft" => Some((VK_LEFT, true)),
            "ArrowRight" => Some((VK_RIGHT, true)),
            // 锁定键
            "CapsLock" => Some((VK_CAPITAL, false)),
            "NumLock" => Some((VK_NUMLOCK, false)),
            "ScrollLock" => Some((VK_SCROLL, false)),
            // 修饰键（右 Control / 右 Alt 带 EXTENDEDKEY 区分左右）
            "ControlLeft" => Some((VK_LCONTROL, false)),
            "ControlRight" => Some((VK_RCONTROL, true)),
            "ShiftLeft" => Some((VK_LSHIFT, false)),
            "ShiftRight" => Some((VK_RSHIFT, false)),
            "AltLeft" => Some((VK_LMENU, false)),
            "AltRight" => Some((VK_RMENU, true)),
            // Win 键
            "MetaLeft" => Some((VK_LWIN, true)),
            "MetaRight" => Some((VK_RWIN, true)),
            // 符号键
            "Minus" => Some((VK_OEM_MINUS, false)),
            "Equal" => Some((VK_OEM_PLUS, false)),
            "BracketLeft" => Some((VK_OEM_4, false)),
            "BracketRight" => Some((VK_OEM_6, false)),
            "Semicolon" => Some((VK_OEM_1, false)),
            "Quote" => Some((VK_OEM_7, false)),
            "Backquote" => Some((VK_OEM_3, false)),
            "Comma" => Some((VK_OEM_COMMA, false)),
            "Period" => Some((VK_OEM_PERIOD, false)),
            "Slash" => Some((VK_OEM_2, false)),
            "Backslash" => Some((VK_OEM_5, false)),
            "IntlBackslash" => Some((VK_OEM_5, false)),
            "ContextMenu" => Some((VK_APPS, false)),
            // 字母区 A-Z / 数字区 0-9 及功能键 F1-F24：动态构造 VK
            _ => {
                // F1-F24（VK_F1..VK_F24 连续）
                if let Some(n) = code.strip_prefix('F').and_then(|s| s.parse::<u16>().ok()) {
                    if (1..=24).contains(&n) {
                        return Some((VIRTUAL_KEY(VK_F1.0 + n - 1), false));
                    }
                }
                // KeyX / DigitN 由 vk 表精确匹配
                vk(code)
            }
        }
    }

    // 兜底 1：code 以 "Key" 开头且长度为 4（如 "KeyQ"），按字母映射。
    // 兜底 2：单个可打印字符走字符 -> VK 映射。
    let fallback = |c: &str| -> Option<(VIRTUAL_KEY, bool)> {
        if c.len() == 4 && c.starts_with("Key") {
            let ch = c.as_bytes()[3];
            if ch.is_ascii_alphabetic() {
                let vk = if ch.is_ascii_lowercase() {
                    VK_A.0 + (ch - b'a') as u16
                } else {
                    VK_A.0 + (ch - b'A') as u16
                };
                return Some((VIRTUAL_KEY(vk), false));
            }
            return None;
        }
        let ch = c.chars().next()?;
        if ch.is_ascii_alphabetic() {
            let base = if ch.is_ascii_lowercase() { b'a' } else { b'A' };
            return Some((VIRTUAL_KEY(VK_A.0 + (ch as u8 - base) as u16), false));
        }
        if ch.is_ascii_digit() {
            return Some((VIRTUAL_KEY(VK_0.0 + (ch as u8 - b'0') as u16), false));
        }
        None
    };

    let code = code.map(|c| c.to_string()).unwrap_or_else(|| key.to_string());
    let (vk, extended) = map_code(&code)
        .or_else(|| fallback(&code))
        .or_else(|| fallback(key))
        .unwrap_or_else(|| {
            log::warn!(
                "[input] 无法映射按键 code={code}, key={key}, modifiers={modifiers:?}，忽略本次注入"
            );
            return (VIRTUAL_KEY(0), false);
        });
    if vk.0 == 0 {
        return Ok(InputEventReceipt {
            ok: false,
            message: format!("unmappable key: code={code}, key={key}"),
        });
    }

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
