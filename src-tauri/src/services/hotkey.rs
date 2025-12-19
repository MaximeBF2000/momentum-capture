use crate::error::{AppError, AppResult};
use std::sync::Arc;

pub type HotkeyCallback = Arc<dyn Fn() + Send + Sync + 'static>;

#[cfg(target_os = "macos")]
pub fn register_hotkey(shortcut: &str, callback: HotkeyCallback) -> AppResult<()> {
    macos::register_hotkey(shortcut, callback)
}

#[cfg(target_os = "macos")]
pub fn unregister_hotkey() -> AppResult<()> {
    macos::unregister_hotkey()
}

#[cfg(not(target_os = "macos"))]
pub fn register_hotkey(_shortcut: &str, _callback: HotkeyCallback) -> AppResult<()> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn unregister_hotkey() -> AppResult<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{AppError, AppResult, HotkeyCallback};
    use std::{
        ffi::c_void,
        sync::{Mutex, OnceLock},
    };

    #[derive(Default)]
    struct HotkeyState {
        hotkey_ref: Option<EventHotKeyRef>,
        handler_ref: Option<EventHandlerRef>,
        callback: Option<HotkeyCallback>,
    }

    unsafe impl Send for HotkeyState {}
    unsafe impl Sync for HotkeyState {}

    static HOTKEY_STATE: OnceLock<Mutex<HotkeyState>> = OnceLock::new();

    const HOTKEY_SIGNATURE: u32 = u32::from_be_bytes(*b"IMRS");
    const EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
    const EVENT_HOT_KEY_PRESSED: u32 = 6;
    const EVENT_PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
    const EVENT_PARAM_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");

    const CMD_KEY: u32 = 1 << 8;
    const SHIFT_KEY: u32 = 1 << 9;
    const OPTION_KEY: u32 = 1 << 11;
    const CONTROL_KEY: u32 = 1 << 12;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct EventHotKeyID {
        signature: u32,
        id: u32,
    }

    #[repr(C)]
    struct EventTypeSpec {
        event_class: u32,
        event_kind: u32,
    }

    type EventHotKeyRef = *mut c_void;
    type EventHandlerRef = *mut c_void;
    type EventHandlerCallRef = *mut c_void;
    type EventTargetRef = *mut c_void;
    type EventRef = *mut c_void;
    type OSStatus = i32;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn RegisterEventHotKey(
            hotKeyCode: u32,
            hotKeyModifiers: u32,
            hotKeyID: EventHotKeyID,
            eventTarget: EventTargetRef,
            options: u32,
            outRef: *mut EventHotKeyRef,
        ) -> OSStatus;

        fn UnregisterEventHotKey(hotKeyRef: EventHotKeyRef) -> OSStatus;

        fn InstallEventHandler(
            target: EventTargetRef,
            handler: EventHandlerUPP,
            numTypes: u32,
            typeList: *const EventTypeSpec,
            userData: *mut c_void,
            outHandlerRef: *mut EventHandlerRef,
        ) -> OSStatus;

        fn RemoveEventHandler(handlerRef: EventHandlerRef) -> OSStatus;

        fn GetApplicationEventTarget() -> EventTargetRef;

        fn GetEventParameter(
            event: EventRef,
            name: u32,
            desiredType: u32,
            actualType: *mut u32,
            bufferSize: u32,
            outActualSize: *mut u32,
            outData: *mut c_void,
        ) -> OSStatus;
    }

    type EventHandlerUPP =
        extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

    #[derive(Debug)]
    struct ParsedShortcut {
        key_code: u32,
        modifiers: u32,
    }

    pub(super) fn register_hotkey(shortcut: &str, callback: HotkeyCallback) -> AppResult<()> {
        let parsed = parse_shortcut(shortcut)?;
        let state = HOTKEY_STATE.get_or_init(|| Mutex::new(HotkeyState::default()));
        let mut guard = state.lock().unwrap();

        unsafe {
            if let Some(handler) = guard.handler_ref.take() {
                RemoveEventHandler(handler);
            }
            if let Some(hotkey) = guard.hotkey_ref.take() {
                UnregisterEventHotKey(hotkey);
            }
        }

        let mut hotkey_ref: EventHotKeyRef = std::ptr::null_mut();
        let status = unsafe {
            RegisterEventHotKey(
                parsed.key_code,
                parsed.modifiers,
                EventHotKeyID {
                    signature: HOTKEY_SIGNATURE,
                    id: 1,
                },
                GetApplicationEventTarget(),
                0,
                &mut hotkey_ref,
            )
        };

        if status != 0 {
            return Err(AppError::Settings(format!(
                "Failed to register immersive shortcut (status: {})",
                status
            )));
        }

        let event_spec = EventTypeSpec {
            event_class: EVENT_CLASS_KEYBOARD,
            event_kind: EVENT_HOT_KEY_PRESSED,
        };

        let mut handler_ref: EventHandlerRef = std::ptr::null_mut();
        let handler_status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                hotkey_handler,
                1,
                &event_spec,
                std::ptr::null_mut(),
                &mut handler_ref,
            )
        };

        if handler_status != 0 {
            unsafe {
                UnregisterEventHotKey(hotkey_ref);
            }
            return Err(AppError::Settings(format!(
                "Failed to install shortcut handler (status: {})",
                handler_status
            )));
        }

        guard.hotkey_ref = Some(hotkey_ref);
        guard.handler_ref = Some(handler_ref);
        guard.callback = Some(callback);
        Ok(())
    }

    pub(super) fn unregister_hotkey() -> AppResult<()> {
        if let Some(state) = HOTKEY_STATE.get() {
            let mut guard = state.lock().unwrap();
            unsafe {
                if let Some(handler) = guard.handler_ref.take() {
                    RemoveEventHandler(handler);
                }
                if let Some(hotkey) = guard.hotkey_ref.take() {
                    UnregisterEventHotKey(hotkey);
                }
            }
            guard.callback = None;
        }
        Ok(())
    }

    extern "C" fn hotkey_handler(
        _: EventHandlerCallRef,
        event: EventRef,
        _: *mut c_void,
    ) -> OSStatus {
        unsafe {
            let mut hotkey_id = EventHotKeyID {
                signature: 0,
                id: 0,
            };
            let mut actual_type = 0u32;
            let mut actual_size = 0u32;
            let status = GetEventParameter(
                event,
                EVENT_PARAM_DIRECT_OBJECT,
                EVENT_PARAM_HOT_KEY_ID,
                &mut actual_type,
                std::mem::size_of::<EventHotKeyID>() as u32,
                &mut actual_size,
                &mut hotkey_id as *mut _ as *mut c_void,
            );

            if status == 0
                && actual_size as usize == std::mem::size_of::<EventHotKeyID>()
                && hotkey_id.signature == HOTKEY_SIGNATURE
            {
                trigger_callback();
            }
        }
        0
    }

    fn trigger_callback() {
        if let Some(state) = HOTKEY_STATE.get() {
            let callback = {
                let guard = state.lock().unwrap();
                guard.callback.clone()
            };

            if let Some(cb) = callback {
                cb();
            }
        }
    }

    fn parse_shortcut(shortcut: &str) -> AppResult<ParsedShortcut> {
        let mut modifiers = 0u32;
        let mut key_code: Option<u32> = None;

        for segment in shortcut.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            match segment.to_ascii_lowercase().as_str() {
                "command" | "cmd" | "⌘" => modifiers |= CMD_KEY,
                "control" | "ctrl" | "^" => modifiers |= CONTROL_KEY,
                "option" | "alt" | "⌥" => modifiers |= OPTION_KEY,
                "shift" | "⇧" => modifiers |= SHIFT_KEY,
                _ => {
                    if key_code.is_some() {
                        return Err(AppError::Settings(
                            "Shortcut must include exactly one base key".into(),
                        ));
                    }
                    key_code = key_code_for(segment);
                    if key_code.is_none() {
                        return Err(AppError::Settings(format!(
                            "Unsupported key '{}'",
                            segment
                        )));
                    }
                }
            }
        }

        let key_code = key_code.ok_or_else(|| {
            AppError::Settings("Shortcut must include a non-modifier key".into())
        })?;

        Ok(ParsedShortcut { key_code, modifiers })
    }

    fn key_code_for(key: &str) -> Option<u32> {
        let normalized = key.to_ascii_uppercase();
        let simple = normalized.as_str();

        match simple {
            "A" => Some(0x00),
            "S" => Some(0x01),
            "D" => Some(0x02),
            "F" => Some(0x03),
            "H" => Some(0x04),
            "G" => Some(0x05),
            "Z" => Some(0x06),
            "X" => Some(0x07),
            "C" => Some(0x08),
            "V" => Some(0x09),
            "B" => Some(0x0B),
            "Q" => Some(0x0C),
            "W" => Some(0x0D),
            "E" => Some(0x0E),
            "R" => Some(0x0F),
            "Y" => Some(0x10),
            "T" => Some(0x11),
            "1" => Some(0x12),
            "2" => Some(0x13),
            "3" => Some(0x14),
            "4" => Some(0x15),
            "6" => Some(0x16),
            "5" => Some(0x17),
            "=" => Some(0x18),
            "9" => Some(0x19),
            "7" => Some(0x1A),
            "-" => Some(0x1B),
            "8" => Some(0x1C),
            "0" => Some(0x1D),
            "]" => Some(0x1E),
            "O" => Some(0x1F),
            "U" => Some(0x20),
            "[" => Some(0x21),
            "I" => Some(0x22),
            "P" => Some(0x23),
            "L" => Some(0x25),
            "J" => Some(0x26),
            "'" => Some(0x27),
            "K" => Some(0x28),
            ";" => Some(0x29),
            "\\" => Some(0x2A),
            "," => Some(0x2B),
            "/" => Some(0x2C),
            "N" => Some(0x2D),
            "M" => Some(0x2E),
            "." => Some(0x2F),
            "`" => Some(0x32),
            "SPACE" => Some(0x31),
            "TAB" => Some(0x30),
            "ENTER" | "RETURN" => Some(0x24),
            "ESC" | "ESCAPE" => Some(0x35),
            "BACKSPACE" => Some(0x33),
            "DELETE" => Some(0x75),
            "ARROWUP" | "UP" => Some(0x7E),
            "ARROWDOWN" | "DOWN" => Some(0x7D),
            "ARROWLEFT" | "LEFT" => Some(0x7B),
            "ARROWRIGHT" | "RIGHT" => Some(0x7C),
            "F1" => Some(0x7A),
            "F2" => Some(0x78),
            "F3" => Some(0x63),
            "F4" => Some(0x76),
            "F5" => Some(0x60),
            "F6" => Some(0x61),
            "F7" => Some(0x62),
            "F8" => Some(0x64),
            "F9" => Some(0x65),
            "F10" => Some(0x6D),
            "F11" => Some(0x67),
            "F12" => Some(0x6F),
            _ => None,
        }
    }
}
