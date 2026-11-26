//! The private messages of W10Wheel (WM_APP + n), to all its windows and threads, in one place so
//! that none is used twice.

const WM_APP: u32 = 0x8000;

/// To the hook thread (a thread message): commands were queued (EngineControl)
pub const COMMAND_WAKE_MESSAGE: u32 = WM_APP;
/// To the tray window: a notification of its icon (NOTIFYICONDATA's uCallbackMessage)
pub const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;
// WM_APP + 2: not used
/// To the GUI: scroll cursor state starts, changes axis, or stops
pub const SCROLL_ACTIVITY_MESSAGE: u32 = WM_APP + 3;
/// To the GUI: an injected W10Message IPC command arrived
pub const IPC_COMMAND_MESSAGE: u32 = WM_APP + 4;
/// To the tray window: the restart as administrator started, or failed
pub const ADMIN_RESTART_MESSAGE: u32 = WM_APP + 5;
/// To the tray window: the Windows theme (dark or light) changed
pub const THEME_CHANGED_MESSAGE: u32 = WM_APP + 6;
/// To the tray window: the tray icon and its tooltip to be updated
pub const TRAY_SYNC_MESSAGE: u32 = WM_APP + 7;
/// To the GUI: the keyboard hook could not be installed. wparam is the Win32 error code, lparam
/// is KEYBOARD_HOOK_FAILED_ON_LOAD when it was installed for a load of the properties (at
/// startup, Reload, switching...): the setting is then turned off without being written to the
/// file, as .NET does. Otherwise (the setting changed in the dialog) the change is stored as off.
pub const KEYBOARD_HOOK_FAILED_MESSAGE: u32 = WM_APP + 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_is_its_own() {
        let messages = [
            COMMAND_WAKE_MESSAGE,
            TRAY_CALLBACK_MESSAGE,
            SCROLL_ACTIVITY_MESSAGE,
            IPC_COMMAND_MESSAGE,
            ADMIN_RESTART_MESSAGE,
            THEME_CHANGED_MESSAGE,
            TRAY_SYNC_MESSAGE,
            KEYBOARD_HOOK_FAILED_MESSAGE,
        ];
        for (i, a) in messages.iter().enumerate() {
            assert!((WM_APP..0xC000).contains(a));
            assert!(messages[i + 1..].iter().all(|b| b != a));
        }
    }
}
