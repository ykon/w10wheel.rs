use std::ffi::c_void;
use std::ptr::null_mut;

use w10wheel::ScrollAxis;
use winsafe::{self as w, co, gui, prelude::Handle};

pub(super) const RESTORE_TIMER: usize = 2;
const RESTORE_INTERVAL_MS: u32 = 150;
const SPI_SETCURSORS: u32 = 0x0057;

const CURSOR_ROLES: [co::OCR; 3] = [co::OCR::NORMAL, co::OCR::IBEAM, co::OCR::HAND];

#[link(name = "user32")]
unsafe extern "system" {
    fn SystemParametersInfoW(action: u32, param: u32, pv_param: *mut c_void, win_ini: u32) -> i32;
}

#[derive(Default)]
pub(super) struct ScrollCursor {
    requested: Option<ScrollAxis>,
    installed: Option<ScrollAxis>,
    changed: bool,
}

impl ScrollCursor {
    pub(super) fn set_axis(&mut self, hwnd: &w::HWND, axis: Option<ScrollAxis>) {
        self.requested = axis;
        if let Some(axis) = axis {
            let _ = hwnd.KillTimer(RESTORE_TIMER);
            self.show(axis);
        } else {
            self.retry_restore(hwnd);
        }
    }

    pub(super) fn retry_restore(&mut self, hwnd: &w::HWND) {
        // A timer message queued before KillTimer can still arrive during the next drag.
        if self.requested.is_some() || self.restore() {
            let _ = hwnd.KillTimer(RESTORE_TIMER);
        } else {
            let _ = hwnd.SetTimer(RESTORE_TIMER, RESTORE_INTERVAL_MS, None);
        }
    }

    fn show(&mut self, axis: ScrollAxis) {
        if self.installed == Some(axis) {
            return;
        }
        let id = match axis {
            ScrollAxis::Vertical => co::IDC::SIZENS,
            ScrollAxis::Horizontal => co::IDC::SIZEWE,
        };
        let Ok(arrow) = system_cursor(id) else {
            return;
        };
        let mut complete = true;
        for role in CURSOR_ROLES {
            if let Ok(mut copy) = arrow.CopyCursor()
                && copy.SetSystemCursor(role).is_ok()
            {
                // SetSystemCursor consumed the copy. The guard still destroys it on failure.
                let _ = copy.leak();
                self.changed = true;
            } else {
                complete = false;
            }
        }
        self.installed = complete.then_some(axis);
    }

    pub(super) fn restore(&mut self) -> bool {
        self.installed = None;
        if !self.changed {
            return true;
        }
        let restored = unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, null_mut(), 0) != 0 };
        if restored {
            self.changed = false;
        }
        restored
    }
}

fn system_cursor(id: co::IDC) -> w::SysResult<w::HCURSOR> {
    gui::Cursor::Idc(id).as_hcursor(&w::HINSTANCE::NULL)
}

impl Drop for ScrollCursor {
    fn drop(&mut self) {
        self.restore();
    }
}
