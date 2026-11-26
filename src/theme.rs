use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use crate::ui_win32::{PaintStruct, begin_paint, dpi_for_window, end_paint};

type Handle = *mut c_void;

pub(crate) use w10wheel::wm_app::THEME_CHANGED_MESSAGE;

const HKEY_CURRENT_USER: Handle = 0x8000_0001_u32 as usize as Handle;
const KEY_READ: u32 = 0x0002_0019;
const KEY_NOTIFY: u32 = 0x0010;
const REG_NOTIFY_CHANGE_LAST_SET: u32 = 0x0000_0004;
const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
const ERROR_SUCCESS: u32 = 0;
const WAIT_OBJECT_0: u32 = 0;
const INFINITE: u32 = 0xFFFF_FFFF;

const DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1: u32 = 19;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;

const WM_SETFOCUS: u32 = 0x0007;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_ENABLE: u32 = 0x000A;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_GETFONT: u32 = 0x0031;
const WM_NCDESTROY: u32 = 0x0082;
const WM_QUERYUISTATE: u32 = 0x0129;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_MOUSELEAVE: u32 = 0x02A3;
const BM_GETCHECK: u32 = 0x00F0;
const CB_GETCURSEL: u32 = 0x0147;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_GETLBTEXTLEN: u32 = 0x0149;
const CB_GETDROPPEDSTATE: u32 = 0x0157;
const LB_GETTEXT: u32 = 0x0189;
const LB_GETTEXTLEN: u32 = 0x018A;
const BST_CHECKED: usize = 1;
const ODT_LISTBOX: u32 = 2;
const ODT_COMBOBOX: u32 = 3;
const ODS_SELECTED: u32 = 0x0001;
const ODS_DISABLED: u32 = 0x0004;
const ODS_FOCUS: u32 = 0x0010;
const ODS_COMBOBOXEDIT: u32 = 0x1000;
const TME_LEAVE: u32 = 0x0000_0002;
const COMBO_SUBCLASS_ID: usize = 0x5731_3043;
const UISF_HIDEFOCUS: isize = 0x1;
const UISF_HIDEACCEL: isize = 0x2;

const GWL_STYLE: i32 = -16;
const BS_TYPEMASK: u32 = 0x0F;
const BS_CHECKBOX: u32 = 0x02;
const BS_AUTOCHECKBOX: u32 = 0x03;
const BS_RADIOBUTTON: u32 = 0x04;
const BS_AUTORADIOBUTTON: u32 = 0x09;

const NM_CUSTOMDRAW: u32 = (-12i32) as u32;
const CDDS_PREPAINT: u32 = 0x0000_0001;
const CDRF_SKIPDEFAULT: isize = 0x0000_0004;
const CDIS_GRAYED: u32 = 0x0002;
const CDIS_DISABLED: u32 = 0x0004;
const CDIS_SELECTED: u32 = 0x0001;
const CDIS_FOCUS: u32 = 0x0010;
const CDIS_HOT: u32 = 0x0040;

const DT_LEFT: u32 = 0x0000_0000;
const DT_VCENTER: u32 = 0x0000_0004;
const DT_SINGLELINE: u32 = 0x0000_0020;
const DT_CALCRECT: u32 = 0x0000_0400;
const DT_NOPREFIX: u32 = 0x0000_0800;
const DT_END_ELLIPSIS: u32 = 0x0000_8000;
const DT_HIDEPREFIX: u32 = 0x0010_0000;

const TRANSPARENT: i32 = 1;
const OPAQUE: i32 = 2;
const PS_SOLID: i32 = 0;
const PS_ENDCAP_ROUND: i32 = 0x0000_0000;
const PS_JOIN_ROUND: u32 = 0x0000_0000;
const PS_GEOMETRIC: i32 = 0x0001_0000;
const BS_SOLID_BRUSH: u32 = 0;
const COLOR_HIGHLIGHT: i32 = 13;
const COLOR_HIGHLIGHTTEXT: i32 = 14;
const COLOR_BTNFACE: i32 = 15;

// COLORREF format: 0x00BBGGRR
const DARK_BG: u32 = 0x0020_2020;
const DARK_SURFACE: u32 = 0x002D_2D2D;
const DARK_DISABLED_SURFACE: u32 = 0x0026_2626;
const DARK_TEXT: u32 = 0x00F0_F0F0;
const DARK_MUTED: u32 = 0x00A0_A0A0;
const DARK_DISABLED: u32 = 0x0078_7878;
const DARK_WARN: u32 = 0x0060_D0FF;
const DARK_ERROR: u32 = 0x006B_6BFF;
const DARK_DIVIDER: u32 = 0x003D_3D3D;

const DARK_ACCENT: u32 = 0x00FF_CD60; // #60CDFF Win11 dark accent
const DARK_ACCENT_PRESSED: u32 = 0x00E8_AE42;
const DARK_ACCENT_TEXT: u32 = 0x0010_1010;
const DARK_CTRL_FILL: u32 = 0x002B_2B2B;
const DARK_CTRL_HOT: u32 = 0x0037_3737;
const DARK_CTRL_DISABLED_FILL: u32 = 0x0024_2424;
const DARK_CTRL_BORDER: u32 = 0x008A_8A8A;
const DARK_CTRL_BORDER_HOT: u32 = 0x00B8_B8B8;
const DARK_CTRL_BORDER_DISABLED: u32 = 0x0055_5555;

const LIGHT_SURFACE: u32 = 0x00FF_FFFF;
const LIGHT_DISABLED_SURFACE: u32 = 0x00F0_F0F0;
const LIGHT_TEXT: u32 = 0x0010_1010;
const LIGHT_MUTED: u32 = 0x0066_6666;
const LIGHT_DISABLED: u32 = 0x006D_6D6D;
const LIGHT_WARN: u32 = 0x0000_5AC8;
const LIGHT_ERROR: u32 = 0x0000_00CC;
const LIGHT_DIVIDER: u32 = 0x00D8_D8D8;
const LIGHT_NAV_SELECTED_BG: u32 = 0x00FB_F1E5;
const LIGHT_NAV_ACCENT: u32 = 0x00C0_6700;
const DARK_NAV_SELECTED_BG: u32 = 0x0038_3838;
const DARK_EXCL_SELECTED_BG: u32 = 0x009E_5A00;
const DARK_COMBO_HOT: u32 = 0x0038_3838;
const DARK_COMBO_BORDER: u32 = 0x0052_5252;
const DARK_COMBO_BORDER_HOT: u32 = 0x007A_7A7A;
const DARK_COMBO_BORDER_DISABLED: u32 = 0x003A_3A3A;
const DARK_COMBO_ARROW: u32 = 0x00D0_D0D0;
const DARK_COMBO_ARROW_DISABLED: u32 = 0x0066_6666;

// The area of the pages (as the content panel of the .NET version) and the items of the list of
// the pages, on the background of the window
const DARK_PANEL: u32 = 0x0027_2727;
const DARK_NAV_ITEM: u32 = 0x0027_2727;
const DARK_NAV_TINT: u32 = 0x0055_3F2E; // the accent mixed with the panel (#2E3F55)
const LIGHT_PANEL: u32 = 0x00FF_FFFF;
const LIGHT_FRAME_BORDER: u32 = 0x00AC_ACAC;

// Push buttons drawn in dark mode (as the buttons of Windows 11 and of the .NET version: filled,
// with a subtle border)
const DARK_BTN_FILL: u32 = 0x002D_2D2D;
const DARK_BTN_HOT: u32 = 0x0035_3535;
const DARK_BTN_PRESSED: u32 = 0x0029_2929;
const DARK_BTN_BORDER: u32 = 0x003C_3C3C;
const DARK_BTN_BORDER_HOT: u32 = 0x0048_4848;
const DARK_BTN_DISABLED_FILL: u32 = 0x0029_2929;
const DARK_BTN_DISABLED_BORDER: u32 = 0x0033_3333;

// The buttons of the spin boxes
const DARK_SPIN_HOT: u32 = 0x003A_3A3A;
const DARK_SPIN_PRESSED: u32 = 0x0045_4545;
const LIGHT_SPIN_HOT: u32 = 0x00EC_ECEC;
const LIGHT_SPIN_PRESSED: u32 = 0x00DC_DCDC;
const LIGHT_SPIN_ARROW: u32 = 0x0040_4040;
const LIGHT_SPIN_ARROW_DISABLED: u32 = 0x00A8_A8A8;
const UPDOWN_SUBCLASS_ID: usize = 0x5731_5544;
const DEFAULT_ACCENT: u32 = 0x00D4_7800; // #0078D4

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlTheme {
    Explorer,
    ComboBox,
    ButtonCheckRadio,
    Plain,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ThemeRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct ComboBoxInfo {
    cb_size: u32,
    rc_item: ThemeRect,
    rc_button: ThemeRect,
    state_button: u32,
    hwnd_combo: Handle,
    hwnd_item: Handle,
    hwnd_list: Handle,
}

#[repr(C)]
struct NmHdr {
    hwnd_from: Handle,
    id_from: usize,
    code: u32,
}

#[repr(C)]
struct NmCustomDraw {
    hdr: NmHdr,
    dw_draw_stage: u32,
    hdc: Handle,
    rc: ThemeRect,
    dw_item_spec: usize,
    u_item_state: u32,
    l_item_lparam: isize,
}

#[repr(C)]
struct LogBrush {
    lb_style: u32,
    lb_color: u32,
    lb_hatch: usize,
}

#[repr(C)]
pub(crate) struct MeasureItemStruct {
    pub ctl_type: u32,
    pub ctl_id: u32,
    pub item_id: u32,
    pub item_width: u32,
    pub item_height: u32,
    pub item_data: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ThemePoint {
    x: i32,
    y: i32,
}

#[repr(C)]
struct TrackMouseEventStruct {
    cb_size: u32,
    dw_flags: u32,
    hwnd_track: Handle,
    dw_hover_time: u32,
}

#[repr(C)]
struct DrawItemStruct {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd_item: Handle,
    hdc: Handle,
    rc_item: ThemeRect,
    item_data: usize,
}

const _: () = {
    assert!(size_of::<ThemeRect>() == 16);
    assert!(size_of::<ThemePoint>() == 8);
    assert!(size_of::<TrackMouseEventStruct>() == 24);
    assert!(size_of::<ComboBoxInfo>() == 64);
    assert!(size_of::<NmHdr>() == 24);
    assert!(size_of::<NmCustomDraw>() == 80);
    assert!(size_of::<MeasureItemStruct>() == 32);
    assert!(size_of::<DrawItemStruct>() == 64);
};

type SubclassProc =
    unsafe extern "system" fn(Handle, u32, usize, isize, usize, usize) -> isize;
type RefreshImmersiveColorPolicyStateFn = unsafe extern "system" fn();
type AllowDarkModeForWindowFn = unsafe extern "system" fn(Handle, i32) -> i32;
type SetPreferredAppModeFn = unsafe extern "system" fn(i32) -> i32;
type FlushMenuThemesFn = unsafe extern "system" fn();

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateEventW(
        attrs: Handle,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> Handle;
    fn SetEvent(event: Handle) -> i32;
    fn WaitForMultipleObjects(
        count: u32,
        handles: *const Handle,
        wait_all: i32,
        milliseconds: u32,
    ) -> u32;
    fn CloseHandle(object: Handle) -> i32;
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn LoadLibraryW(name: *const u16) -> Handle;
    fn GetProcAddress(module: Handle, proc_name: *const u8) -> *const c_void;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        hkey: Handle,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        pdw_type: *mut u32,
        pv_data: *mut c_void,
        pcb_data: *mut u32,
    ) -> i32;
    fn RegOpenKeyExW(
        hkey: Handle,
        sub_key: *const u16,
        options: u32,
        sam_desired: u32,
        result: *mut Handle,
    ) -> i32;
    fn RegNotifyChangeKeyValue(
        hkey: Handle,
        watch_subtree: i32,
        notify_filter: u32,
        event: Handle,
        asynchronous: i32,
    ) -> i32;
    fn RegCloseKey(hkey: Handle) -> i32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: Handle,
        attribute: u32,
        value: *const c_void,
        value_size: u32,
    ) -> i32;
}

#[link(name = "uxtheme")]
unsafe extern "system" {
    fn SetWindowTheme(hwnd: Handle, sub_app_name: *const u16, sub_id_list: *const u16) -> i32;
    fn DrawThemeParentBackground(hwnd: Handle, hdc: Handle, prc: *const ThemeRect) -> i32;
}

#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(
        hwnd: Handle,
        pfn_subclass: SubclassProc,
        id_subclass: usize,
        dw_ref_data: usize,
    ) -> i32;
    fn RemoveWindowSubclass(
        hwnd: Handle,
        pfn_subclass: SubclassProc,
        id_subclass: usize,
    ) -> i32;
    fn DefSubclassProc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateSolidBrush(color: u32) -> Handle;
    fn CreatePen(style: i32, width: i32, color: u32) -> Handle;
    fn ExtCreatePen(
        pen_style: u32,
        width: u32,
        lplb: *const LogBrush,
        style_count: u32,
        lp_style: *const u32,
    ) -> Handle;
    fn SelectObject(hdc: Handle, object: Handle) -> Handle;
    fn RoundRect(
        hdc: Handle,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        width: i32,
        height: i32,
    ) -> i32;
    fn Ellipse(hdc: Handle, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    fn MoveToEx(hdc: Handle, x: i32, y: i32, lp_pt: Handle) -> i32;
    fn LineTo(hdc: Handle, x: i32, y: i32) -> i32;
    fn SetTextColor(hdc: Handle, color: u32) -> u32;
    fn SetBkColor(hdc: Handle, color: u32) -> u32;
    fn SetBkMode(hdc: Handle, mode: i32) -> i32;
    fn DeleteObject(object: Handle) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetSysColor(index: i32) -> u32;
    fn GetClientRect(hwnd: Handle, rect: *mut ThemeRect) -> i32;
    fn GetWindowRect(hwnd: Handle, rect: *mut ThemeRect) -> i32;
    fn GetCursorPos(pt: *mut ThemePoint) -> i32;
    fn InvalidateRect(hwnd: Handle, rect: *const ThemeRect, erase: i32) -> i32;
    fn TrackMouseEvent(event_track: *mut TrackMouseEventStruct) -> i32;
    fn FillRect(hdc: Handle, lprc: *const ThemeRect, hbr: Handle) -> i32;
    fn DrawTextW(
        hdc: Handle,
        lpch_text: *const u16,
        cch_text: i32,
        lprc: *mut ThemeRect,
        format: u32,
    ) -> i32;
    fn DrawFocusRect(hdc: Handle, lprc: *const ThemeRect) -> i32;
    fn PostMessageW(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn SendMessageW(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: Handle, index: i32) -> isize;
    fn GetClassNameW(hwnd: Handle, class_name: *mut u16, max_count: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: Handle) -> i32;
    fn GetWindowTextW(hwnd: Handle, text: *mut u16, max_count: i32) -> i32;
    fn IsWindowEnabled(hwnd: Handle) -> i32;
    fn GetFocus() -> Handle;
    fn GetComboBoxInfo(hwnd_combo: Handle, pcbi: *mut ComboBoxInfo) -> i32;
    fn GetPropW(hwnd: Handle, name: *const u16) -> Handle;
    fn SetPropW(hwnd: Handle, name: *const u16, data: Handle) -> i32;
    fn RemovePropW(hwnd: Handle, name: *const u16) -> Handle;
    fn DrawIconEx(
        hdc: Handle,
        x: i32,
        y: i32,
        icon: Handle,
        cx: i32,
        cy: i32,
        step: u32,
        brush: Handle,
        flags: u32,
    ) -> i32;
    fn GetCapture() -> Handle;
    fn GetKeyState(vkey: i32) -> i16;
    fn ScreenToClient(hwnd: Handle, pt: *mut ThemePoint) -> i32;
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct UxThemeProcs {
    refresh_immersive: Option<RefreshImmersiveColorPolicyStateFn>,
    allow_dark_window: Option<AllowDarkModeForWindowFn>,
    set_preferred_app_mode: Option<SetPreferredAppModeFn>,
    flush_menu_themes: Option<FlushMenuThemesFn>,
}

static UXTHEME_PROCS: OnceLock<UxThemeProcs> = OnceLock::new();

fn uxtheme_procs() -> &'static UxThemeProcs {
    UXTHEME_PROCS.get_or_init(|| unsafe {
        let name = to_wide_null("uxtheme.dll");
        let mut module = GetModuleHandleW(name.as_ptr());
        if module.is_null() {
            module = LoadLibraryW(name.as_ptr());
        }
        if module.is_null() {
            return UxThemeProcs {
                refresh_immersive: None,
                allow_dark_window: None,
                set_preferred_app_mode: None,
                flush_menu_themes: None,
            };
        }
        let refresh_ptr = GetProcAddress(module, 104usize as *const u8);
        let allow_ptr = GetProcAddress(module, 133usize as *const u8);
        let set_ptr = GetProcAddress(module, 135usize as *const u8);
        let flush_ptr = GetProcAddress(module, 136usize as *const u8);
        UxThemeProcs {
            refresh_immersive: (!refresh_ptr.is_null()).then(|| {
                std::mem::transmute::<*const c_void, RefreshImmersiveColorPolicyStateFn>(
                    refresh_ptr,
                )
            }),
            allow_dark_window: (!allow_ptr.is_null()).then(|| {
                std::mem::transmute::<*const c_void, AllowDarkModeForWindowFn>(allow_ptr)
            }),
            set_preferred_app_mode: (!set_ptr.is_null())
                .then(|| std::mem::transmute::<*const c_void, SetPreferredAppModeFn>(set_ptr)),
            flush_menu_themes: (!flush_ptr.is_null())
                .then(|| std::mem::transmute::<*const c_void, FlushMenuThemesFn>(flush_ptr)),
        }
    })
}

pub(crate) fn init_uxtheme_dark_mode() {
    let procs = uxtheme_procs();
    if let Some(refresh) = procs.refresh_immersive {
        unsafe { refresh() };
    }
    if let Some(f) = procs.set_preferred_app_mode {
        unsafe {
            let _ = f(1); // AllowDark
        }
    }
    if let Some(flush) = procs.flush_menu_themes {
        unsafe { flush() };
    }
}

pub(crate) fn flush_menu_themes() {
    let procs = uxtheme_procs();
    if let Some(refresh) = procs.refresh_immersive {
        unsafe { refresh() };
    }
    if let Some(f) = procs.flush_menu_themes {
        unsafe { f() };
    }
}

pub(crate) fn allow_dark_mode_for_window(hwnd: Handle, dark: bool) {
    if hwnd.is_null() {
        return;
    }
    if let Some(f) = uxtheme_procs().allow_dark_window {
        unsafe {
            let _ = f(hwnd, i32::from(dark));
        }
    }
}

pub(crate) fn is_windows_dark() -> bool {
    let sub_key = to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value = to_wide_null("AppsUseLightTheme");
    let mut data: u32 = 1;
    let mut size: u32 = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut data).cast::<c_void>(),
            &mut size,
        )
    };
    status == ERROR_SUCCESS as i32 && data == 0
}

pub(crate) fn apply_window_dark_mode(hwnd: Handle, dark: bool) {
    if hwnd.is_null() {
        return;
    }
    allow_dark_mode_for_window(hwnd, dark);
    let val: i32 = i32::from(dark);
    unsafe {
        let hr = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const val).cast::<c_void>(),
            size_of::<i32>() as u32,
        );
        if hr != 0 {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1,
                (&raw const val).cast::<c_void>(),
                size_of::<i32>() as u32,
            );
        }
    }
}

pub(crate) fn apply_control_theme(hwnd: Handle, dark: bool, kind: ControlTheme) {
    if hwnd.is_null() || kind == ControlTheme::Plain {
        return;
    }
    allow_dark_mode_for_window(hwnd, dark);

    let explorer = to_wide_null("DarkMode_Explorer");
    let cfd = to_wide_null("DarkMode_CFD");
    let combobox_cls = to_wide_null("Combobox");
    let (app_name, sub_id): (*const u16, *const u16) = match (dark, kind) {
        (true, ControlTheme::Explorer | ControlTheme::ButtonCheckRadio) => {
            (explorer.as_ptr(), null())
        }
        (true, ControlTheme::ComboBox) => (cfd.as_ptr(), combobox_cls.as_ptr()),
        (false, _) => (null(), null()),
        (_, ControlTheme::Plain) => return,
    };
    unsafe {
        let _ = SetWindowTheme(hwnd, app_name, sub_id);
    }

    if kind == ControlTheme::ComboBox {
        let mut cbi = ComboBoxInfo {
            cb_size: size_of::<ComboBoxInfo>() as u32,
            rc_item: ThemeRect::default(),
            rc_button: ThemeRect::default(),
            state_button: 0,
            hwnd_combo: null_mut(),
            hwnd_item: null_mut(),
            hwnd_list: null_mut(),
        };
        if unsafe { GetComboBoxInfo(hwnd, &mut cbi) } != 0 {
            if !cbi.hwnd_list.is_null() {
                allow_dark_mode_for_window(cbi.hwnd_list, dark);
                unsafe {
                    let _ = SetWindowTheme(
                        cbi.hwnd_list,
                        if dark { explorer.as_ptr() } else { null() },
                        null(),
                    );
                }
            }
            if !cbi.hwnd_item.is_null() {
                allow_dark_mode_for_window(cbi.hwnd_item, dark);
                unsafe {
                    let _ = SetWindowTheme(
                        cbi.hwnd_item,
                        if dark { explorer.as_ptr() } else { null() },
                        null(),
                    );
                }
            }
        }
        unsafe {
            let _ = SetWindowSubclass(hwnd, combo_subclass_proc, COMBO_SUBCLASS_ID, usize::from(dark));
        }
    }
}

unsafe extern "system" fn combo_subclass_proc(
    hwnd: Handle,
    msg: u32,
    wparam: usize,
    lparam: isize,
    id_subclass: usize,
    dw_ref_data: usize,
) -> isize {
    if msg == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, combo_subclass_proc, id_subclass);
            return DefSubclassProc(hwnd, msg, wparam, lparam);
        }
    }

    if dw_ref_data == 0 {
        return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    }

    match msg {
        WM_ERASEBKGND => return 1,
        WM_MOUSEMOVE => {
            let mut tme = TrackMouseEventStruct {
                cb_size: size_of::<TrackMouseEventStruct>() as u32,
                dw_flags: TME_LEAVE,
                hwnd_track: hwnd,
                dw_hover_time: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
                let _ = InvalidateRect(hwnd, null(), 0);
            }
        }
        WM_MOUSELEAVE => {
            unsafe {
                let _ = InvalidateRect(hwnd, null(), 0);
            }
        }
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | WM_LBUTTONUP => {
            let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            unsafe {
                let _ = InvalidateRect(hwnd, null(), 0);
            }
            return res;
        }
        WM_PAINT => {
            paint_dark_combobox(hwnd);
            return 0;
        }
        _ => {}
    }

    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn paint_dark_combobox(hwnd: Handle) {
    let mut ps = PaintStruct::default();
    let hdc = begin_paint(hwnd, &mut ps);
    if hdc.is_null() {
        end_paint(hwnd, &ps);
        return;
    }

    let mut rc = ThemeRect::default();
    unsafe {
        GetClientRect(hwnd, &mut rc);
    }
    let dpi = dpi_for_window(hwnd);
    let enabled = unsafe { IsWindowEnabled(hwnd) != 0 };
    let dropped = unsafe { SendMessageW(hwnd, CB_GETDROPPEDSTATE, 0, 0) != 0 };
    let focused = unsafe { GetFocus() == hwnd };
    let mut hot = false;
    if enabled {
        let mut pt = ThemePoint::default();
        let mut wrc = ThemeRect::default();
        if unsafe { GetCursorPos(&mut pt) != 0 && GetWindowRect(hwnd, &mut wrc) != 0 } {
            hot = pt.x >= wrc.left && pt.x < wrc.right && pt.y >= wrc.top && pt.y < wrc.bottom;
        }
    }

    // The corners in the color under the box (the window or the area of the pages)
    unsafe {
        DrawThemeParentBackground(hwnd, hdc, &rc);
    }

    let fill_col = if !enabled {
        DARK_DISABLED_SURFACE
    } else if dropped || hot {
        DARK_COMBO_HOT
    } else {
        DARK_SURFACE
    };
    let border_col = if !enabled {
        DARK_COMBO_BORDER_DISABLED
    } else if dropped || focused {
        DARK_ACCENT
    } else if hot {
        DARK_COMBO_BORDER_HOT
    } else {
        DARK_COMBO_BORDER
    };

    let rad = scale_dpi(4, dpi).max(2);
    let border_pen = unsafe { CreatePen(PS_SOLID, 1, border_col) };
    let fill_brush = unsafe { CreateSolidBrush(fill_col) };
    unsafe {
        let old_pen = SelectObject(hdc, border_pen);
        let old_brush = SelectObject(hdc, fill_brush);
        RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, rad, rad);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(fill_brush);
        DeleteObject(border_pen);
    }

    let cur = unsafe { SendMessageW(hwnd, CB_GETCURSEL, 0, 0) };
    if cur >= 0 {
        let len = unsafe { SendMessageW(hwnd, CB_GETLBTEXTLEN, cur as usize, 0) };
        if len > 0 {
            let mut buf = vec![0u16; len as usize + 1];
            let copied =
                unsafe { SendMessageW(hwnd, CB_GETLBTEXT, cur as usize, buf.as_mut_ptr() as isize) };
            if copied > 0 {
                let font = unsafe { SendMessageW(hwnd, WM_GETFONT, 0, 0) as Handle };
                let old_font = if !font.is_null() {
                    unsafe { SelectObject(hdc, font) }
                } else {
                    null_mut()
                };
                let left_pad = scale_dpi(8, dpi);
                let right_pad = scale_dpi(26, dpi);
                let text_left = (rc.left + left_pad).min(rc.right);
                let mut text_rc = ThemeRect {
                    left: text_left,
                    top: rc.top,
                    right: (rc.right - right_pad).max(text_left),
                    bottom: rc.bottom,
                };
                unsafe {
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, if enabled { DARK_TEXT } else { DARK_DISABLED });
                    DrawTextW(
                        hdc,
                        buf.as_ptr(),
                        copied as i32,
                        &mut text_rc,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS,
                    );
                    if !old_font.is_null() {
                        SelectObject(hdc, old_font);
                    }
                }
            }
        }
    }

    let arrow_cx = rc.right - scale_dpi(13, dpi);
    let arrow_cy = (rc.top + rc.bottom) / 2;
    let hw = scale_dpi(4, dpi).max(3);
    let hh = scale_dpi(2, dpi).max(2);
    let stroke = ((dpi as i32 + 48) / 96).clamp(1, 3) as u32;
    let lb = LogBrush {
        lb_style: BS_SOLID_BRUSH,
        lb_color: if enabled {
            DARK_COMBO_ARROW
        } else {
            DARK_COMBO_ARROW_DISABLED
        },
        lb_hatch: 0,
    };
    let chev_pen = unsafe {
        ExtCreatePen(
            (PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND) as u32 | PS_JOIN_ROUND,
            stroke,
            &lb,
            0,
            null(),
        )
    };
    if !chev_pen.is_null() {
        let top_y = arrow_cy - hh / 2;
        let bot_y = arrow_cy + (hh + 1) / 2;
        unsafe {
            let old_pen = SelectObject(hdc, chev_pen);
            MoveToEx(hdc, arrow_cx - hw, top_y, null_mut());
            LineTo(hdc, arrow_cx, bot_y);
            LineTo(hdc, arrow_cx + hw, top_y);
            SelectObject(hdc, old_pen);
            DeleteObject(chev_pen);
        }
    }

    end_paint(hwnd, &ps);
}

pub(crate) struct ThemePalette {
    pub dark: bool,
    pub bg_color: u32,
    pub surface_color: u32,
    pub disabled_surface_color: u32,
    pub text_color: u32,
    pub muted_color: u32,
    pub disabled_color: u32,
    pub warn_color: u32,
    pub error_color: u32,
    pub divider_color: u32,
    pub panel_color: u32,
    pub frame_border_color: u32,
    pub frame_border_disabled_color: u32,
    pub accent_color: u32,
    pub bg_brush: Handle,
    pub surface_brush: Handle,
    pub disabled_surface_brush: Handle,
    pub panel_brush: Handle,
}

impl ThemePalette {
    pub fn new(dark: bool) -> Self {
        let (
            bg_color,
            surface_color,
            disabled_surface_color,
            text_color,
            muted_color,
            disabled_color,
            warn_color,
            error_color,
            divider_color,
        ) = if dark {
            (
                DARK_BG,
                DARK_SURFACE,
                DARK_DISABLED_SURFACE,
                DARK_TEXT,
                DARK_MUTED,
                DARK_DISABLED,
                DARK_WARN,
                DARK_ERROR,
                DARK_DIVIDER,
            )
        } else {
            let btnface = unsafe { GetSysColor(COLOR_BTNFACE) };
            (
                btnface,
                LIGHT_SURFACE,
                LIGHT_DISABLED_SURFACE,
                LIGHT_TEXT,
                LIGHT_MUTED,
                LIGHT_DISABLED,
                LIGHT_WARN,
                LIGHT_ERROR,
                LIGHT_DIVIDER,
            )
        };
        let (panel_color, frame_border_color, frame_border_disabled_color) = if dark {
            (DARK_PANEL, DARK_COMBO_BORDER, DARK_COMBO_BORDER_DISABLED)
        } else {
            (LIGHT_PANEL, LIGHT_FRAME_BORDER, LIGHT_DIVIDER)
        };
        let bg_brush = unsafe { CreateSolidBrush(bg_color) };
        let surface_brush = unsafe { CreateSolidBrush(surface_color) };
        let disabled_surface_brush = unsafe { CreateSolidBrush(disabled_surface_color) };
        let panel_brush = unsafe { CreateSolidBrush(panel_color) };
        Self {
            dark,
            bg_color,
            surface_color,
            disabled_surface_color,
            text_color,
            muted_color,
            disabled_color,
            warn_color,
            error_color,
            divider_color,
            panel_color,
            frame_border_color,
            frame_border_disabled_color,
            accent_color: system_accent_color(),
            bg_brush,
            surface_brush,
            disabled_surface_brush,
            panel_brush,
        }
    }

    /// The fill of an item of the list of the pages (selected: tinted with the accent)
    pub fn nav_item_color(&self, selected: bool) -> u32 {
        match (self.dark, selected) {
            (true, true) => DARK_NAV_TINT,
            (true, false) => DARK_NAV_ITEM,
            (false, true) => LIGHT_NAV_SELECTED_BG,
            (false, false) => self.bg_color,
        }
    }

    /// The border of an input box (a number), the accent while it has the focus
    pub fn frame_border(&self, enabled: bool, focused: bool) -> u32 {
        if !enabled {
            self.frame_border_disabled_color
        } else if focused {
            if self.dark { DARK_ACCENT } else { self.accent_color }
        } else {
            self.frame_border_color
        }
    }

    pub fn fill_rect(&self, hdc: Handle, left: i32, top: i32, right: i32, bottom: i32, color: u32) {
        fill_solid(hdc, &ThemeRect { left, top, right, bottom }, color);
    }

    /// A rounded rectangle filled with `fill`, its border in `border`
    pub fn fill_round_rect(
        &self,
        hdc: Handle,
        rc: (i32, i32, i32, i32),
        radius: i32,
        fill: u32,
        border: u32,
    ) {
        round_rect(hdc, &ThemeRect { left: rc.0, top: rc.1, right: rc.2, bottom: rc.3 }, radius, fill, border);
    }

    /// The colors of a label: `fg` on the window or on the area of the pages
    pub fn ctlcolor_label(&self, hdc: Handle, fg: u32, on_panel: bool) -> isize {
        let (bg, brush) = if on_panel {
            (self.panel_color, self.panel_brush)
        } else {
            (self.bg_color, self.bg_brush)
        };
        unsafe {
            SetTextColor(hdc, fg);
            SetBkMode(hdc, TRANSPARENT);
            SetBkColor(hdc, bg);
        }
        brush as isize
    }

    pub fn erase_background(&self, hwnd: Handle, hdc: Handle) -> isize {
        if hwnd.is_null() || hdc.is_null() {
            return 0;
        }
        let mut rc = ThemeRect::default();
        unsafe {
            GetClientRect(hwnd, &mut rc);
            FillRect(hdc, &rc, self.bg_brush);
        }
        1
    }


    pub fn handle_ctlcolor_disabled_edit(&self, hdc: Handle) -> isize {
        unsafe {
            SetTextColor(hdc, self.disabled_color);
            SetBkMode(hdc, OPAQUE);
            SetBkColor(hdc, self.disabled_surface_color);
        }
        self.disabled_surface_brush as isize
    }

    pub fn handle_ctlcolor(
        &self,
        hdc: Handle,
        msg: u32,
        is_muted: bool,
        is_warn: bool,
        is_error: bool,
    ) -> isize {
        let fg = if is_error {
            self.error_color
        } else if is_warn {
            self.warn_color
        } else if is_muted {
            self.muted_color
        } else {
            self.text_color
        };
        unsafe {
            SetTextColor(hdc, fg);
        }
        match msg {
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                unsafe {
                    SetBkMode(hdc, OPAQUE);
                    SetBkColor(hdc, self.surface_color);
                }
                self.surface_brush as isize
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                unsafe {
                    SetBkMode(hdc, TRANSPARENT);
                    SetBkColor(hdc, self.bg_color);
                }
                self.bg_brush as isize
            }
            _ => self.bg_brush as isize,
        }
    }
}

impl Drop for ThemePalette {
    fn drop(&mut self) {
        if !self.bg_brush.is_null() {
            unsafe { DeleteObject(self.bg_brush) };
        }
        if !self.surface_brush.is_null() {
            unsafe { DeleteObject(self.surface_brush) };
        }
        if !self.disabled_surface_brush.is_null() {
            unsafe { DeleteObject(self.disabled_surface_brush) };
        }
        if !self.panel_brush.is_null() {
            unsafe { DeleteObject(self.panel_brush) };
        }
    }
}

fn fill_solid(hdc: Handle, rc: &ThemeRect, color: u32) {
    let brush = unsafe { CreateSolidBrush(color) };
    if !brush.is_null() {
        unsafe {
            FillRect(hdc, rc, brush);
            DeleteObject(brush);
        }
    }
}

fn round_rect(hdc: Handle, rc: &ThemeRect, radius: i32, fill: u32, border: u32) {
    let pen = unsafe { CreatePen(PS_SOLID, 1, border) };
    let brush = unsafe { CreateSolidBrush(fill) };
    unsafe {
        let old_pen = SelectObject(hdc, pen);
        let old_brush = SelectObject(hdc, brush);
        RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

/// The accent color of Windows (as `SystemVisualSettings.AccentColor` of the .NET version)
pub(crate) fn system_accent_color() -> u32 {
    let sub_key = to_wide_null("Software\\Microsoft\\Windows\\DWM");
    let value = to_wide_null("AccentColor");
    let mut data: u32 = 0;
    let mut size: u32 = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut data).cast::<c_void>(),
            &mut size,
        )
    };
    // ABGR: the low 24 bits are a COLORREF
    if status == ERROR_SUCCESS as i32 { data & 0x00FF_FFFF } else { DEFAULT_ACCENT }
}

fn is_dark_color(color: u32) -> bool {
    let r = color & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = (color >> 16) & 0xFF;
    (299 * r + 587 * g + 114 * b) / 1000 < 128
}

/// `color` mixed with white (`amount` > 0) or black (< 0), for the hot and pressed accent buttons
fn shade(color: u32, amount: i32) -> u32 {
    let ch = |shift: u32| {
        let c = ((color >> shift) & 0xFF) as i32;
        let target = if amount >= 0 { 255 } else { 0 };
        (c + (target - c) * amount.abs() / 100) as u32
    };
    ch(0) | (ch(8) << 8) | (ch(16) << 16)
}

const SHIELD_PROP: &str = "W10WheelShield";

/// Marks a push button whose shield (UAC) is drawn by `handle_dark_button_custom_draw` (the custom
/// draw skips the drawing of the button, its shield too)
pub(crate) fn set_button_shield_prop(hwnd: Handle, show: bool) {
    let name = to_wide_null(SHIELD_PROP);
    unsafe {
        if show {
            SetPropW(hwnd, name.as_ptr(), 1usize as Handle);
        } else {
            RemovePropW(hwnd, name.as_ptr());
        }
    }
}

fn has_button_shield(hwnd: Handle) -> bool {
    let name = to_wide_null(SHIELD_PROP);
    unsafe { !GetPropW(hwnd, name.as_ptr()).is_null() }
}

/// Draws `icon` (`size` pixels) at x, y
pub(crate) fn draw_icon(hdc: Handle, icon: Handle, x: i32, y: i32, size: i32) {
    if !hdc.is_null() && !icon.is_null() {
        unsafe {
            DrawIconEx(hdc, x, y, icon, size, size, 0, null_mut(), 0x0003 /* DI_NORMAL */);
        }
    }
}

/// Whether the window is a button control (class "Button", case-insensitive as Windows compares
/// class names)
fn is_button_class(hwnd: Handle) -> bool {
    let mut buf = [0u16; 16];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    len > 0 && String::from_utf16_lossy(&buf[..len as usize]).eq_ignore_ascii_case("Button")
}

/// Draws the check boxes, the radio buttons and the push buttons in dark mode (the themes of
/// Windows draw them light). `accent_default`: the default push button (the focused one, else the
/// one of Enter) is filled with the accent color, as the buttons of the dialogs of the .NET version.
pub(crate) fn handle_dark_button_custom_draw(
    parent: Handle,
    lparam: isize,
    dpi: u32,
    palette: &ThemePalette,
    accent_default: bool,
) -> Option<isize> {
    if !palette.dark || lparam == 0 {
        return None;
    }
    let nm = unsafe { &*(lparam as *const NmCustomDraw) };
    if nm.hdr.code != NM_CUSTOMDRAW || nm.dw_draw_stage != CDDS_PREPAINT {
        return None;
    }
    let ctrl = nm.hdr.hwnd_from;
    // Only the buttons: other controls send NM_CUSTOMDRAW too, and their styles can look like a
    // button type (e.g. a tooltip, TTS_ALWAYSTIP | TTS_NOPREFIX = BS_AUTOCHECKBOX, was drawn as a
    // check box with its text cut on one line)
    if ctrl.is_null() || !is_button_class(ctrl) {
        return None;
    }

    let style = (unsafe { GetWindowLongPtrW(ctrl, GWL_STYLE) } as u32) & BS_TYPEMASK;
    let is_check = matches!(style, BS_CHECKBOX | BS_AUTOCHECKBOX);
    let is_radio = matches!(style, BS_RADIOBUTTON | BS_AUTORADIOBUTTON);
    if matches!(style, BS_PUSHBUTTON_TYPE | BS_DEFPUSHBUTTON_TYPE) {
        return Some(draw_dark_push_button(
            parent,
            nm,
            dpi,
            palette,
            accent_default && style == BS_DEFPUSHBUTTON_TYPE,
        ));
    }
    if !is_check && !is_radio {
        return None;
    }

    let hdc = nm.hdc;
    let checked = unsafe { SendMessageW(ctrl, BM_GETCHECK, 0, 0) } as usize == BST_CHECKED;
    let enabled = unsafe { IsWindowEnabled(ctrl) != 0 }
        && (nm.u_item_state & (CDIS_DISABLED | CDIS_GRAYED)) == 0;
    let pressed = (nm.u_item_state & CDIS_SELECTED) != 0;
    let hot = (nm.u_item_state & CDIS_HOT) != 0;
    let ui_state = if !parent.is_null() {
        unsafe { SendMessageW(parent, WM_QUERYUISTATE, 0, 0) }
    } else {
        0
    };
    let hide_focus = (ui_state & UISF_HIDEFOCUS) != 0;
    let hide_accel = (ui_state & UISF_HIDEACCEL) != 0;
    let has_focus = ((nm.u_item_state & CDIS_FOCUS) != 0 || unsafe { GetFocus() == ctrl })
        && !hide_focus
        && enabled;

    // The background under the box and the text (the window or the area of the pages)
    unsafe {
        DrawThemeParentBackground(ctrl, hdc, &nm.rc);
    }

    let box_sz = ((14 * dpi as i32 + 48) / 96).clamp(13, 42);
    let pad_left = (2 * dpi as i32 + 48) / 96;
    let bx = nm.rc.left + pad_left;
    let by = nm.rc.top + (nm.rc.bottom - nm.rc.top - box_sz) / 2;

    if is_check {
        let (fill_col, border_col) = if !enabled {
            if checked {
                (DARK_CTRL_BORDER_DISABLED, DARK_CTRL_BORDER_DISABLED)
            } else {
                (DARK_CTRL_DISABLED_FILL, DARK_CTRL_BORDER_DISABLED)
            }
        } else if checked {
            let accent = if pressed {
                DARK_ACCENT_PRESSED
            } else {
                DARK_ACCENT
            };
            (accent, accent)
        } else if pressed || hot {
            (DARK_CTRL_HOT, DARK_CTRL_BORDER_HOT)
        } else {
            (DARK_CTRL_FILL, DARK_CTRL_BORDER)
        };

        let rad = (box_sz / 4).max(2);
        let pen = unsafe { CreatePen(PS_SOLID, 1, border_col) };
        let brush = unsafe { CreateSolidBrush(fill_col) };
        unsafe {
            let old_pen = SelectObject(hdc, pen);
            let old_brush = SelectObject(hdc, brush);
            RoundRect(hdc, bx, by, bx + box_sz, by + box_sz, rad, rad);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            DeleteObject(brush);
            DeleteObject(pen);
        }

        if checked {
            let check_col = if enabled {
                DARK_ACCENT_TEXT
            } else {
                palette.disabled_color
            };
            let stroke = (box_sz / 7).clamp(1, 4) as u32;
            let lb = LogBrush {
                lb_style: BS_SOLID_BRUSH,
                lb_color: check_col,
                lb_hatch: 0,
            };
            let check_pen = unsafe {
                ExtCreatePen(
                    (PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND) as u32 | PS_JOIN_ROUND,
                    stroke,
                    &lb,
                    0,
                    null(),
                )
            };
            if !check_pen.is_null() {
                let x0 = bx + (box_sz * 24 + 50) / 100;
                let y0 = by + (box_sz * 52 + 50) / 100;
                let x1 = bx + (box_sz * 44 + 50) / 100;
                let y1 = by + (box_sz * 72 + 50) / 100;
                let x2 = bx + (box_sz * 77 + 50) / 100;
                let y2 = by + (box_sz * 30 + 50) / 100;
                unsafe {
                    let old_pen = SelectObject(hdc, check_pen);
                    MoveToEx(hdc, x0, y0, null_mut());
                    LineTo(hdc, x1, y1);
                    LineTo(hdc, x2, y2);
                    SelectObject(hdc, old_pen);
                    DeleteObject(check_pen);
                }
            }
        }
    } else {
        let (outer_fill, outer_border) = if !enabled {
            if checked {
                (DARK_CTRL_BORDER_DISABLED, DARK_CTRL_BORDER_DISABLED)
            } else {
                (DARK_CTRL_DISABLED_FILL, DARK_CTRL_BORDER_DISABLED)
            }
        } else if checked {
            let accent = if pressed {
                DARK_ACCENT_PRESSED
            } else {
                DARK_ACCENT
            };
            (accent, accent)
        } else if pressed || hot {
            (DARK_CTRL_HOT, DARK_CTRL_BORDER_HOT)
        } else {
            (DARK_CTRL_FILL, DARK_CTRL_BORDER)
        };

        let pen = unsafe { CreatePen(PS_SOLID, 1, outer_border) };
        let brush = unsafe { CreateSolidBrush(outer_fill) };
        unsafe {
            let old_pen = SelectObject(hdc, pen);
            let old_brush = SelectObject(hdc, brush);
            Ellipse(hdc, bx, by, bx + box_sz, by + box_sz);
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            DeleteObject(brush);
            DeleteObject(pen);
        }

        if checked {
            let inset = (box_sz * 28 + 50) / 100;
            let dot_col = if enabled { DARK_BG } else { DARK_CTRL_DISABLED_FILL };
            let dot_pen = unsafe { CreatePen(PS_SOLID, 1, dot_col) };
            let dot_brush = unsafe { CreateSolidBrush(dot_col) };
            unsafe {
                let old_pen = SelectObject(hdc, dot_pen);
                let old_brush = SelectObject(hdc, dot_brush);
                Ellipse(
                    hdc,
                    bx + inset,
                    by + inset,
                    bx + box_sz - inset,
                    by + box_sz - inset,
                );
                SelectObject(hdc, old_brush);
                SelectObject(hdc, old_pen);
                DeleteObject(dot_brush);
                DeleteObject(dot_pen);
            }
        }
    }

    let len = unsafe { GetWindowTextLengthW(ctrl) };
    if len > 0 {
        let mut buf = vec![0u16; len as usize + 1];
        let copied = unsafe { GetWindowTextW(ctrl, buf.as_mut_ptr(), buf.len() as i32) };
        if copied > 0 {
            let font = unsafe { SendMessageW(ctrl, WM_GETFONT, 0, 0) as Handle };
            let old_font = if !font.is_null() {
                unsafe { SelectObject(hdc, font) }
            } else {
                null_mut()
            };
            let gap = (7 * dpi as i32 + 48) / 96;
            let mut text_rc = ThemeRect {
                left: bx + box_sz + gap,
                top: nm.rc.top,
                right: nm.rc.right,
                bottom: nm.rc.bottom,
            };
            let mut dt_flags = DT_LEFT | DT_VCENTER | DT_SINGLELINE;
            if hide_accel {
                dt_flags |= DT_HIDEPREFIX;
            }
            unsafe {
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(
                    hdc,
                    if enabled {
                        palette.text_color
                    } else {
                        palette.disabled_color
                    },
                );
                DrawTextW(hdc, buf.as_ptr(), copied, &mut text_rc, dt_flags);
            }

            if has_focus {
                let mut focus_rc = ThemeRect {
                    left: text_rc.left,
                    top: text_rc.top,
                    right: text_rc.right,
                    bottom: text_rc.bottom,
                };
                unsafe {
                    DrawTextW(
                        hdc,
                        buf.as_ptr(),
                        copied,
                        &mut focus_rc,
                        dt_flags | DT_CALCRECT,
                    );
                }
                let txt_h = focus_rc.bottom - focus_rc.top;
                let cy = nm.rc.top + (nm.rc.bottom - nm.rc.top - txt_h) / 2;
                let pad = (2 * dpi as i32 + 48) / 96;
                let draw_focus = ThemeRect {
                    left: (focus_rc.left - pad).max(nm.rc.left),
                    top: (cy - pad).max(nm.rc.top),
                    right: (focus_rc.right + pad).min(nm.rc.right),
                    bottom: (cy + txt_h + pad).min(nm.rc.bottom),
                };
                unsafe {
                    DrawFocusRect(hdc, &draw_focus);
                }
            }

            if !old_font.is_null() {
                unsafe { SelectObject(hdc, old_font) };
            }
        }
    }

    Some(CDRF_SKIPDEFAULT)
}

const BS_PUSHBUTTON_TYPE: u32 = 0x00;
const BS_DEFPUSHBUTTON_TYPE: u32 = 0x01;

fn draw_dark_push_button(
    parent: Handle,
    nm: &NmCustomDraw,
    dpi: u32,
    palette: &ThemePalette,
    accent: bool,
) -> isize {
    let ctrl = nm.hdr.hwnd_from;
    let hdc = nm.hdc;
    let rc = nm.rc;
    let enabled = unsafe { IsWindowEnabled(ctrl) != 0 }
        && (nm.u_item_state & (CDIS_DISABLED | CDIS_GRAYED)) == 0;
    let pressed = enabled && (nm.u_item_state & CDIS_SELECTED) != 0;
    let hot = enabled && (nm.u_item_state & CDIS_HOT) != 0;
    let ui_state = if parent.is_null() {
        0
    } else {
        unsafe { SendMessageW(parent, WM_QUERYUISTATE, 0, 0) }
    };
    let has_focus = enabled
        && (ui_state & UISF_HIDEFOCUS) == 0
        && ((nm.u_item_state & CDIS_FOCUS) != 0 || unsafe { GetFocus() == ctrl });

    unsafe {
        DrawThemeParentBackground(ctrl, hdc, &rc);
    }

    let accent = accent && enabled;
    let (fill, border, text) = if accent {
        let base = palette.accent_color;
        let fill = if pressed {
            shade(base, -12)
        } else if hot {
            shade(base, 10)
        } else {
            base
        };
        let text = if is_dark_color(base) { 0x00FF_FFFF } else { 0x0000_0000 };
        (fill, fill, text)
    } else if !enabled {
        (DARK_BTN_DISABLED_FILL, DARK_BTN_DISABLED_BORDER, palette.disabled_color)
    } else if pressed {
        (DARK_BTN_PRESSED, DARK_BTN_BORDER, 0x00CF_CFCF)
    } else if hot {
        (DARK_BTN_HOT, DARK_BTN_BORDER_HOT, palette.text_color)
    } else {
        (DARK_BTN_FILL, DARK_BTN_BORDER, palette.text_color)
    };
    let rad = scale_dpi(8, dpi).max(4);
    round_rect(hdc, &rc, rad, fill, border);

    // The text (and the shield before it) centered
    let len = unsafe { GetWindowTextLengthW(ctrl) };
    let mut buf = vec![0u16; len.max(0) as usize + 1];
    let copied = if len > 0 {
        unsafe { GetWindowTextW(ctrl, buf.as_mut_ptr(), buf.len() as i32) }
    } else {
        0
    };
    let font = unsafe { SendMessageW(ctrl, WM_GETFONT, 0, 0) as Handle };
    let old_font = if font.is_null() {
        null_mut()
    } else {
        unsafe { SelectObject(hdc, font) }
    };
    let mut flags = DT_LEFT | DT_VCENTER | DT_SINGLELINE;
    if (ui_state & UISF_HIDEACCEL) != 0 {
        flags |= DT_HIDEPREFIX;
    }
    let mut measure = ThemeRect { left: 0, top: 0, right: rc.right - rc.left, bottom: rc.bottom - rc.top };
    if copied > 0 {
        unsafe {
            DrawTextW(hdc, buf.as_ptr(), copied, &mut measure, flags | DT_CALCRECT);
        }
    }
    let text_w = if copied > 0 { measure.right - measure.left } else { 0 };
    let shield = has_button_shield(ctrl);
    let icon_sz = scale_dpi(16, dpi);
    let icon_gap = scale_dpi(6, dpi);
    let content_w = text_w + if shield { icon_sz + icon_gap } else { 0 };
    let mut x = rc.left + (rc.right - rc.left - content_w) / 2;
    if shield {
        let icon = crate::icons::cached_stock_icon(crate::icons::StockIcon::Shield, icon_sz);
        draw_icon(hdc, icon, x, rc.top + (rc.bottom - rc.top - icon_sz) / 2, icon_sz);
        x += icon_sz + icon_gap;
    }
    if copied > 0 {
        let mut text_rc = ThemeRect { left: x, top: rc.top, right: rc.right, bottom: rc.bottom };
        unsafe {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, text);
            DrawTextW(hdc, buf.as_ptr(), copied, &mut text_rc, flags);
        }
    }
    if !old_font.is_null() {
        unsafe { SelectObject(hdc, old_font) };
    }

    if has_focus && !accent {
        let inset = scale_dpi(3, dpi);
        let focus = ThemeRect {
            left: rc.left + inset,
            top: rc.top + inset,
            right: rc.right - inset,
            bottom: rc.bottom - inset,
        };
        unsafe {
            SetTextColor(hdc, palette.text_color);
            SetBkColor(hdc, fill);
            DrawFocusRect(hdc, &focus);
        }
    }

    CDRF_SKIPDEFAULT
}

fn scale_dpi(px: i32, dpi: u32) -> i32 {
    (px * dpi as i32 + 48) / 96
}

pub(crate) fn handle_owner_draw_listbox(
    parent: Handle,
    lparam: isize,
    dpi: u32,
    palette: &ThemePalette,
    is_nav: bool,
) -> bool {
    if lparam == 0 {
        return false;
    }
    let dis = unsafe { &*(lparam as *const DrawItemStruct) };
    if dis.ctl_type != ODT_LISTBOX || dis.hdc.is_null() || dis.hwnd_item.is_null() {
        return false;
    }

    let selected = (dis.item_state & ODS_SELECTED) != 0;
    let focused = (dis.item_state & ODS_FOCUS) != 0;
    let ui_state = if !parent.is_null() {
        unsafe { SendMessageW(parent, WM_QUERYUISTATE, 0, 0) }
    } else {
        0
    };
    let hide_focus = (ui_state & UISF_HIDEFOCUS) != 0;

    if is_nav {
        draw_nav_item(dis, dpi, palette, selected, focused && !hide_focus);
        return true;
    }

    if dis.item_id == u32::MAX {
        unsafe {
            FillRect(dis.hdc, &dis.rc_item, palette.surface_brush);
            if focused && !hide_focus {
                DrawFocusRect(dis.hdc, &dis.rc_item);
            }
        }
        return true;
    }

    let (bg_color, fg_color) = if is_nav {
        if selected {
            if palette.dark {
                (DARK_NAV_SELECTED_BG, 0x00FF_FFFF)
            } else {
                (LIGHT_NAV_SELECTED_BG, LIGHT_TEXT)
            }
        } else {
            (palette.surface_color, palette.text_color)
        }
    } else if selected {
        if palette.dark {
            (DARK_EXCL_SELECTED_BG, 0x00FF_FFFF)
        } else {
            unsafe {
                (
                    GetSysColor(COLOR_HIGHLIGHT),
                    GetSysColor(COLOR_HIGHLIGHTTEXT),
                )
            }
        }
    } else {
        (palette.surface_color, palette.text_color)
    };

    let bg_brush = unsafe { CreateSolidBrush(bg_color) };
    if !bg_brush.is_null() {
        unsafe {
            FillRect(dis.hdc, &dis.rc_item, bg_brush);
            DeleteObject(bg_brush);
        }
    }

    if is_nav && selected {
        draw_left_accent_bar(dis.hdc, dis.rc_item, dpi, palette.dark, 5);
    }

    let text_len = unsafe { SendMessageW(dis.hwnd_item, LB_GETTEXTLEN, dis.item_id as usize, 0) };
    if text_len > 0 {
        let mut buf = vec![0u16; text_len as usize + 1];
        let copied = unsafe {
            SendMessageW(
                dis.hwnd_item,
                LB_GETTEXT,
                dis.item_id as usize,
                buf.as_mut_ptr() as isize,
            )
        };
        if copied > 0 {
            let left_pad = scale_dpi(if is_nav { 11 } else { 6 }, dpi);
            let right_pad = if is_nav { scale_dpi(4, dpi) } else { 0 };
            draw_owner_item_text(
                dis.hdc,
                dis.hwnd_item,
                dis.rc_item,
                left_pad,
                right_pad,
                fg_color,
                is_nav,
                &buf[..copied as usize],
            );
        }
    }

    if focused && !hide_focus {
        unsafe {
            DrawFocusRect(dis.hdc, &dis.rc_item);
        }
    }

    true
}

/// The gap under each item of the list of the pages (inside its height)
pub(crate) const NAV_ITEM_GAP: i32 = 4;

/// An item of the list of the pages, as a flat button (as in the .NET version): on the background
/// of the window, filled (tinted with the accent when selected, with the accent bar of Windows 11)
fn draw_nav_item(dis: &DrawItemStruct, dpi: u32, palette: &ThemePalette, selected: bool, focus: bool) {
    let rc = dis.rc_item;
    fill_solid(dis.hdc, &rc, palette.bg_color);
    if dis.item_id == u32::MAX {
        return;
    }
    let item = ThemeRect {
        left: rc.left,
        top: rc.top,
        right: rc.right,
        bottom: rc.bottom - scale_dpi(NAV_ITEM_GAP, dpi),
    };
    let fill = palette.nav_item_color(selected);
    round_rect(dis.hdc, &item, scale_dpi(6, dpi).max(3), fill, fill);
    if selected {
        draw_left_accent_bar(dis.hdc, item, dpi, palette.dark, 7);
    }

    let text_len = unsafe { SendMessageW(dis.hwnd_item, LB_GETTEXTLEN, dis.item_id as usize, 0) };
    if text_len > 0 {
        let mut buf = vec![0u16; text_len as usize + 1];
        let copied = unsafe {
            SendMessageW(dis.hwnd_item, LB_GETTEXT, dis.item_id as usize, buf.as_mut_ptr() as isize)
        };
        if copied > 0 {
            draw_owner_item_text(
                dis.hdc,
                dis.hwnd_item,
                item,
                scale_dpi(14, dpi),
                scale_dpi(4, dpi),
                palette.text_color,
                true,
                &buf[..copied as usize],
            );
        }
    }

    if focus {
        let inset = scale_dpi(2, dpi);
        let focus_rc = ThemeRect {
            left: item.left + inset,
            top: item.top + inset,
            right: item.right - inset,
            bottom: item.bottom - inset,
        };
        unsafe {
            SetTextColor(dis.hdc, palette.text_color);
            SetBkColor(dis.hdc, fill);
            DrawFocusRect(dis.hdc, &focus_rc);
        }
    }
}

/// Paints a spin box button (`msctls_updown32` with `UDS_HORZ`) as the ▼ ▲ buttons of the .NET
/// version, in the colors of the box it is in (the themes draw it light, and as a scroll bar)
pub(crate) fn apply_updown_theme(hwnd: Handle, dark: bool) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let _ = SetWindowSubclass(hwnd, updown_subclass_proc, UPDOWN_SUBCLASS_ID, usize::from(dark));
    }
    unsafe {
        let _ = InvalidateRect(hwnd, null(), 1);
    }
}

unsafe extern "system" fn updown_subclass_proc(
    hwnd: Handle,
    msg: u32,
    wparam: usize,
    lparam: isize,
    id_subclass: usize,
    dw_ref_data: usize,
) -> isize {
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_CAPTURECHANGED: u32 = 0x0215;
    const WM_TIMER: u32 = 0x0113;
    match msg {
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, updown_subclass_proc, id_subclass);
            return DefSubclassProc(hwnd, msg, wparam, lparam);
        },
        WM_ERASEBKGND => return 1,
        WM_PAINT => {
            paint_updown(hwnd, dw_ref_data != 0);
            return 0;
        }
        WM_MOUSEMOVE => {
            let mut tme = TrackMouseEventStruct {
                cb_size: size_of::<TrackMouseEventStruct>() as u32,
                dw_flags: TME_LEAVE,
                hwnd_track: hwnd,
                dw_hover_time: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
                let _ = InvalidateRect(hwnd, null(), 0);
            }
        }
        WM_MOUSELEAVE | WM_ENABLE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_CAPTURECHANGED | WM_TIMER => {
            let res = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            unsafe {
                let _ = InvalidateRect(hwnd, null(), 0);
            }
            return res;
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn paint_updown(hwnd: Handle, dark: bool) {
    let mut ps = PaintStruct::default();
    let hdc = begin_paint(hwnd, &mut ps);
    if hdc.is_null() {
        end_paint(hwnd, &ps);
        return;
    }
    let mut rc = ThemeRect::default();
    unsafe {
        GetClientRect(hwnd, &mut rc);
    }
    let dpi = dpi_for_window(hwnd);
    let enabled = unsafe { IsWindowEnabled(hwnd) != 0 };
    let (surface, disabled_surface, arrow, arrow_disabled, hot_col, pressed_col) = if dark {
        (
            DARK_SURFACE,
            DARK_DISABLED_SURFACE,
            DARK_COMBO_ARROW,
            DARK_COMBO_ARROW_DISABLED,
            DARK_SPIN_HOT,
            DARK_SPIN_PRESSED,
        )
    } else {
        (
            LIGHT_SURFACE,
            LIGHT_DISABLED_SURFACE,
            LIGHT_SPIN_ARROW,
            LIGHT_SPIN_ARROW_DISABLED,
            LIGHT_SPIN_HOT,
            LIGHT_SPIN_PRESSED,
        )
    };
    fill_solid(hdc, &rc, if enabled { surface } else { disabled_surface });

    // The half under the cursor: hot, pressed while the button is held on it
    let mut hot_half: Option<bool> = None; // Some(false): left (down), Some(true): right (up)
    if enabled {
        let mut pt = ThemePoint::default();
        if unsafe { GetCursorPos(&mut pt) != 0 && ScreenToClient(hwnd, &mut pt) != 0 }
            && pt.x >= rc.left
            && pt.x < rc.right
            && pt.y >= rc.top
            && pt.y < rc.bottom
        {
            hot_half = Some(pt.x >= (rc.left + rc.right) / 2);
        }
    }
    let held = unsafe { GetCapture() == hwnd && GetKeyState(0x01 /* VK_LBUTTON */) < 0 };

    let mid = (rc.left + rc.right) / 2;
    let halves = [
        (false, ThemeRect { left: rc.left, top: rc.top, right: mid, bottom: rc.bottom }),
        (true, ThemeRect { left: mid, top: rc.top, right: rc.right, bottom: rc.bottom }),
    ];
    let pad = scale_dpi(1, dpi);
    for (up, half) in halves {
        if hot_half == Some(up) {
            let col = if held { pressed_col } else { hot_col };
            let inner = ThemeRect {
                left: half.left + pad,
                top: half.top + pad,
                right: half.right - pad,
                bottom: half.bottom - pad,
            };
            round_rect(hdc, &inner, scale_dpi(4, dpi).max(2), col, col);
        }
        let cx = (half.left + half.right) / 2;
        let cy = (half.top + half.bottom) / 2;
        let hw = scale_dpi(4, dpi).max(3);
        let hh = scale_dpi(2, dpi).max(2);
        let stroke = ((dpi as i32 + 48) / 96).clamp(1, 3) as u32;
        let lb = LogBrush {
            lb_style: BS_SOLID_BRUSH,
            lb_color: if enabled { arrow } else { arrow_disabled },
            lb_hatch: 0,
        };
        let pen = unsafe {
            ExtCreatePen((PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND) as u32 | PS_JOIN_ROUND, stroke, &lb, 0, null())
        };
        if !pen.is_null() {
            // ▼ on the left (down), ▲ on the right (up)
            let top_y = cy - hh / 2;
            let bot_y = cy + (hh + 1) / 2;
            let (tip_y, base_y) = if up { (top_y, bot_y) } else { (bot_y, top_y) };
            unsafe {
                let old = SelectObject(hdc, pen);
                MoveToEx(hdc, cx - hw, base_y, null_mut());
                LineTo(hdc, cx, tip_y);
                LineTo(hdc, cx + hw, base_y);
                SelectObject(hdc, old);
                DeleteObject(pen);
            }
        }
    }
    end_paint(hwnd, &ps);
}

pub(crate) fn handle_owner_draw_combobox(
    parent: Handle,
    lparam: isize,
    dpi: u32,
    palette: &ThemePalette,
) -> bool {
    if lparam == 0 {
        return false;
    }
    let dis = unsafe { &*(lparam as *const DrawItemStruct) };
    if dis.ctl_type != ODT_COMBOBOX || dis.hdc.is_null() || dis.hwnd_item.is_null() {
        return false;
    }

    let is_edit_face = (dis.item_state & ODS_COMBOBOXEDIT) != 0;
    let selected = (dis.item_state & ODS_SELECTED) != 0;
    let disabled = (dis.item_state & ODS_DISABLED) != 0;
    let focused = (dis.item_state & ODS_FOCUS) != 0;
    let ui_state = if !parent.is_null() {
        unsafe { SendMessageW(parent, WM_QUERYUISTATE, 0, 0) }
    } else {
        0
    };
    let hide_focus = (ui_state & UISF_HIDEFOCUS) != 0;

    let (bg_color, fg_color, draw_accent) = if is_edit_face {
        if palette.dark {
            if disabled {
                (DARK_DISABLED_SURFACE, DARK_DISABLED, false)
            } else {
                (DARK_SURFACE, DARK_TEXT, false)
            }
        } else if disabled {
            (LIGHT_DISABLED_SURFACE, LIGHT_DISABLED, false)
        } else {
            (LIGHT_SURFACE, LIGHT_TEXT, false)
        }
    } else if palette.dark {
        if selected {
            (DARK_NAV_SELECTED_BG, 0x00FF_FFFF, true)
        } else {
            (palette.surface_color, palette.text_color, false)
        }
    } else if selected {
        unsafe {
            (
                GetSysColor(COLOR_HIGHLIGHT),
                GetSysColor(COLOR_HIGHLIGHTTEXT),
                false,
            )
        }
    } else {
        (palette.surface_color, palette.text_color, false)
    };

    let bg_brush = unsafe { CreateSolidBrush(bg_color) };
    if !bg_brush.is_null() {
        unsafe {
            FillRect(dis.hdc, &dis.rc_item, bg_brush);
            DeleteObject(bg_brush);
        }
    }

    if draw_accent {
        draw_left_accent_bar(dis.hdc, dis.rc_item, dpi, palette.dark, 4);
    }

    if dis.item_id != u32::MAX {
        let text_len =
            unsafe { SendMessageW(dis.hwnd_item, CB_GETLBTEXTLEN, dis.item_id as usize, 0) };
        if text_len > 0 {
            let mut buf = vec![0u16; text_len as usize + 1];
            let copied = unsafe {
                SendMessageW(
                    dis.hwnd_item,
                    CB_GETLBTEXT,
                    dis.item_id as usize,
                    buf.as_mut_ptr() as isize,
                )
            };
            if copied > 0 {
                let left_pad = scale_dpi(
                    if is_edit_face {
                        4
                    } else if palette.dark {
                        10
                    } else {
                        6
                    },
                    dpi,
                );
                let right_pad = scale_dpi(4, dpi);
                draw_owner_item_text(
                    dis.hdc,
                    dis.hwnd_item,
                    dis.rc_item,
                    left_pad,
                    right_pad,
                    fg_color,
                    true,
                    &buf[..copied as usize],
                );
            }
        }
    }

    if focused && !hide_focus && !is_edit_face {
        unsafe {
            DrawFocusRect(dis.hdc, &dis.rc_item);
        }
    }

    true
}

fn draw_left_accent_bar(hdc: Handle, rc: ThemeRect, dpi: u32, dark: bool, inset_dp: i32) {
    let bar_x = rc.left + scale_dpi(2, dpi);
    let bar_w = scale_dpi(3, dpi).max(2);
    let inset_y = scale_dpi(inset_dp, dpi);
    let bar_rc = ThemeRect {
        left: bar_x,
        top: (rc.top + inset_y).min(rc.bottom),
        right: (bar_x + bar_w).min(rc.right),
        bottom: (rc.bottom - inset_y).max(rc.top),
    };
    let accent = if dark { DARK_ACCENT } else { LIGHT_NAV_ACCENT };
    let accent_brush = unsafe { CreateSolidBrush(accent) };
    if !accent_brush.is_null() {
        unsafe {
            FillRect(hdc, &bar_rc, accent_brush);
            DeleteObject(accent_brush);
        }
    }
}

fn draw_owner_item_text(
    hdc: Handle,
    hwnd_item: Handle,
    rc_item: ThemeRect,
    left_pad: i32,
    right_pad: i32,
    fg_color: u32,
    ellipsis: bool,
    text_utf16: &[u16],
) {
    let font = unsafe { SendMessageW(hwnd_item, WM_GETFONT, 0, 0) as Handle };
    let old_font = if !font.is_null() {
        unsafe { SelectObject(hdc, font) }
    } else {
        null_mut()
    };
    let text_left = rc_item.left + left_pad;
    let mut text_rc = ThemeRect {
        left: text_left.min(rc_item.right),
        top: rc_item.top,
        right: (rc_item.right - right_pad).max(text_left.min(rc_item.right)),
        bottom: rc_item.bottom,
    };
    let mut dt_flags = DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
    if ellipsis {
        dt_flags |= DT_END_ELLIPSIS;
    }
    unsafe {
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, fg_color);
        DrawTextW(
            hdc,
            text_utf16.as_ptr(),
            text_utf16.len() as i32,
            &mut text_rc,
            dt_flags,
        );
        if !old_font.is_null() {
            SelectObject(hdc, old_font);
        }
    }
}

struct SendHandle(Handle);
unsafe impl Send for SendHandle {}

pub(crate) struct ThemeWatcher {
    stop_event: Handle,
    join: Option<JoinHandle<()>>,
}

impl ThemeWatcher {
    pub fn start(target_hwnd: Handle) -> Option<Self> {
        let stop_event = unsafe { CreateEventW(null_mut(), 1, 0, null()) };
        if stop_event.is_null() {
            return None;
        }
        let target = SendHandle(target_hwnd);
        let stop_for_thread = SendHandle(stop_event);
        let stopping = std::sync::Arc::new(AtomicBool::new(false));
        let stop_flag = std::sync::Arc::clone(&stopping);

        let join = thread::Builder::new()
            .name("w10wheel-theme".to_owned())
            .spawn(move || {
                let target_ref = target;
                let stop_ref = stop_for_thread;
                let sub_key =
                    to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
                let mut hkey = null_mut();
                let opened = unsafe {
                    RegOpenKeyExW(
                        HKEY_CURRENT_USER,
                        sub_key.as_ptr(),
                        0,
                        KEY_NOTIFY | KEY_READ,
                        &mut hkey,
                    )
                };
                if opened != ERROR_SUCCESS as i32 || hkey.is_null() {
                    return;
                }
                let change_event = unsafe { CreateEventW(null_mut(), 0, 0, null()) };
                if change_event.is_null() {
                    unsafe { RegCloseKey(hkey) };
                    return;
                }

                let mut last_dark = is_windows_dark();
                while !stop_flag.load(Ordering::Relaxed) {
                    let res = unsafe {
                        RegNotifyChangeKeyValue(
                            hkey,
                            0,
                            REG_NOTIFY_CHANGE_LAST_SET,
                            change_event,
                            1,
                        )
                    };
                    if res != ERROR_SUCCESS as i32 {
                        break;
                    }
                    let current_dark = is_windows_dark();
                    if current_dark != last_dark {
                        last_dark = current_dark;
                        unsafe {
                            let _ = PostMessageW(
                                target_ref.0,
                                THEME_CHANGED_MESSAGE,
                                usize::from(current_dark),
                                0,
                            );
                        }
                    }
                    let handles = [stop_ref.0, change_event];
                    let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
                    if wait == WAIT_OBJECT_0 {
                        break;
                    }
                    if wait != WAIT_OBJECT_0 + 1 {
                        break;
                    }
                    let next_dark = is_windows_dark();
                    if next_dark != last_dark {
                        last_dark = next_dark;
                        unsafe {
                            let _ = PostMessageW(
                                target_ref.0,
                                THEME_CHANGED_MESSAGE,
                                usize::from(next_dark),
                                0,
                            );
                        }
                    }
                }
                unsafe {
                    CloseHandle(change_event);
                    RegCloseKey(hkey);
                }
            })
            .ok()?;

        Some(Self {
            stop_event,
            join: Some(join),
        })
    }
}

impl Drop for ThemeWatcher {
    fn drop(&mut self) {
        if !self.stop_event.is_null() {
            unsafe { SetEvent(self.stop_event) };
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        if !self.stop_event.is_null() {
            unsafe { CloseHandle(self.stop_event) };
            self.stop_event = null_mut();
        }
    }
}
