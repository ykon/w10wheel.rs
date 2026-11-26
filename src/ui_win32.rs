use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{MaybeUninit, size_of};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

pub(crate) type Handle = *mut c_void;
pub(crate) type WndProc = unsafe extern "system" fn(Handle, u32, usize, isize) -> isize;
type SetProcessDpiAwarenessContextFn = unsafe extern "system" fn(isize) -> i32;
type GetDpiForWindowFn = unsafe extern "system" fn(Handle) -> u32;
type AdjustWindowRectExForDpiFn =
    unsafe extern "system" fn(*mut Rect, u32, i32, u32, u32) -> i32;

const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
const ICC_WIN95_CLASSES: u32 = 0x0000_00FF;
const ICC_STANDARD_CLASSES: u32 = 0x0000_4000;
const ICC_UPDOWN_CLASS: u32 = 0x0000_0010;
const SPI_GETNONCLIENTMETRICS: u32 = 0x0029;
const FW_NORMAL: i32 = 400;
const FW_BOLD: i32 = 700;
const DEFAULT_CHARSET: u8 = 1;
const CLEARTYPE_QUALITY: u8 = 5;

pub(crate) const WM_CREATE: u32 = 0x0001;
pub(crate) const WM_DESTROY: u32 = 0x0002;
pub(crate) const WM_CLOSE: u32 = 0x0010;
pub(crate) const WM_ERASEBKGND: u32 = 0x0014;
pub(crate) const WM_ENDSESSION: u32 = 0x0016;
pub(crate) const WM_SETCURSOR: u32 = 0x0020;
pub(crate) const WM_DRAWITEM: u32 = 0x002B;
pub(crate) const WM_MEASUREITEM: u32 = 0x002C;
pub(crate) const WM_SETFONT: u32 = 0x0030;
pub(crate) const WM_NOTIFY: u32 = 0x004E;
pub(crate) const WM_SETICON: u32 = 0x0080;
pub(crate) const WM_NCDESTROY: u32 = 0x0082;
pub(crate) const WM_COMMAND: u32 = 0x0111;
pub(crate) const WM_TIMER: u32 = 0x0113;
pub(crate) const WM_CTLCOLOREDIT: u32 = 0x0133;
pub(crate) const WM_CTLCOLORLISTBOX: u32 = 0x0134;
pub(crate) const WM_CTLCOLORBTN: u32 = 0x0135;
pub(crate) const WM_CTLCOLORSTATIC: u32 = 0x0138;
pub(crate) const WM_DPICHANGED: u32 = 0x02E0;

pub(crate) const WM_USER: u32 = 0x0400;
const TTM_ACTIVATE: u32 = WM_USER + 1;
const TTM_SETDELAYTIME: u32 = WM_USER + 3;
const TTM_ADDTOOLW: u32 = WM_USER + 50;
const TTM_UPDATETIPTEXTW: u32 = WM_USER + 57;
const TTM_SETMAXTIPWIDTH: u32 = WM_USER + 24;
const TTM_TRACKACTIVATE: u32 = WM_USER + 17;
const TTM_TRACKPOSITION: u32 = WM_USER + 18;
const TTDT_AUTOPOP: usize = 2;
const TTF_IDISHWND: u32 = 0x0001;
const TTF_SUBCLASS: u32 = 0x0010;
const TTF_TRACK: u32 = 0x0020;
const TTF_ABSOLUTE: u32 = 0x0080;
const TME_LEAVE: u32 = 0x0000_0002;
pub(crate) const WM_MOUSEMOVE: u32 = 0x0200;
pub(crate) const WM_MOUSELEAVE: u32 = 0x02A3;
pub(crate) const WM_ACTIVATE: u32 = 0x0006;
pub(crate) const WM_SHOWWINDOW: u32 = 0x0018;
/// How long a tooltip stays (as the .NET version: AutoPopDelay 20 s) and the delay before one shows
/// (InitialDelay 0.5 s)
pub(crate) const TIP_AUTOPOP_MS: u32 = 20_000;
pub(crate) const TIP_INITIAL_DELAY_MS: u32 = 500;

#[repr(C)]
struct TrackMouseEventInfo {
    cb_size: u32,
    flags: u32,
    hwnd_track: Handle,
    hover_time: u32,
}
const TTS_ALWAYSTIP: u32 = 0x01;
const TTS_NOPREFIX: u32 = 0x02;

pub(crate) const CW_USEDEFAULT: i32 = i32::MIN;
pub(crate) const WS_OVERLAPPED: u32 = 0x0000_0000;
pub(crate) const WS_POPUP: u32 = 0x8000_0000;
pub(crate) const WS_CHILD: u32 = 0x4000_0000;
pub(crate) const WS_VISIBLE: u32 = 0x1000_0000;
pub(crate) const WS_DISABLED: u32 = 0x0800_0000;
pub(crate) const WS_CLIPCHILDREN: u32 = 0x0200_0000;
pub(crate) const WS_CAPTION: u32 = 0x00C0_0000;
pub(crate) const WS_BORDER: u32 = 0x0080_0000;
pub(crate) const WS_VSCROLL: u32 = 0x0020_0000;
pub(crate) const WS_HSCROLL: u32 = 0x0010_0000;
pub(crate) const WS_SYSMENU: u32 = 0x0008_0000;
pub(crate) const WS_TABSTOP: u32 = 0x0001_0000;
pub(crate) const WS_GROUP: u32 = 0x0002_0000;
pub(crate) const WS_EX_CONTROLPARENT: u32 = 0x0001_0000;
pub(crate) const WS_EX_TOPMOST: u32 = 0x0000_0008;

pub(crate) const BS_PUSHBUTTON: u32 = 0x0000;
pub(crate) const BS_DEFPUSHBUTTON: u32 = 0x0001;
pub(crate) const BS_AUTOCHECKBOX: u32 = 0x0003;
pub(crate) const BS_AUTORADIOBUTTON: u32 = 0x0009;
pub(crate) const SS_LEFT: u32 = 0x0000;
pub(crate) const SS_ICON: u32 = 0x0003;
pub(crate) const SS_REALSIZECONTROL: u32 = 0x0040;
pub(crate) const SS_NOTIFY: u32 = 0x0100;
pub(crate) const ES_AUTOHSCROLL: u32 = 0x0080;
pub(crate) const ES_NUMBER: u32 = 0x2000;
pub(crate) const CBS_DROPDOWNLIST: u32 = 0x0003;
pub(crate) const CBS_OWNERDRAWFIXED: u32 = 0x0010;
pub(crate) const CBS_HASSTRINGS: u32 = 0x0200;
pub(crate) const LBS_NOTIFY: u32 = 0x0001;
pub(crate) const LBS_OWNERDRAWFIXED: u32 = 0x0010;
pub(crate) const LBS_HASSTRINGS: u32 = 0x0040;
pub(crate) const LBS_NOINTEGRALHEIGHT: u32 = 0x0100;
pub(crate) const LBS_EXTENDEDSEL: u32 = 0x0800;

pub(crate) const UDS_SETBUDDYINT: u32 = 0x0002;
pub(crate) const UDS_ARROWKEYS: u32 = 0x0020;
pub(crate) const UDS_NOTHOUSANDS: u32 = 0x0080;

pub(crate) const BN_CLICKED: u16 = 0;
pub(crate) const CBN_SELCHANGE: u16 = 1;
pub(crate) const LBN_SELCHANGE: u16 = 1;
pub(crate) const EN_KILLFOCUS: u16 = 0x0200;
pub(crate) const EN_CHANGE: u16 = 0x0300;

pub(crate) const BM_GETCHECK: u32 = 0x00F0;
pub(crate) const BM_SETCHECK: u32 = 0x00F1;
pub(crate) const BST_UNCHECKED: usize = 0;
pub(crate) const BST_CHECKED: usize = 1;
pub(crate) const CB_ADDSTRING: u32 = 0x0143;
pub(crate) const CB_GETCURSEL: u32 = 0x0147;
pub(crate) const CB_RESETCONTENT: u32 = 0x014B;
pub(crate) const CB_SETCURSEL: u32 = 0x014E;
pub(crate) const CB_SETITEMHEIGHT: u32 = 0x0153;
pub(crate) const CB_SETDROPPEDWIDTH: u32 = 0x0160;
pub(crate) const LB_ADDSTRING: u32 = 0x0180;
pub(crate) const LB_RESETCONTENT: u32 = 0x0184;
pub(crate) const LB_GETCOUNT: u32 = 0x018B;
pub(crate) const LB_GETSELCOUNT: u32 = 0x0190;
pub(crate) const LB_GETSELITEMS: u32 = 0x0191;
pub(crate) const LB_SETCURSEL: u32 = 0x0186;
pub(crate) const LB_GETCURSEL: u32 = 0x0188;
pub(crate) const LB_SETITEMHEIGHT: u32 = 0x01A0;
pub(crate) const STM_SETICON: u32 = 0x0170;
pub(crate) const UDM_SETBUDDY: u32 = WM_USER + 105;
pub(crate) const UDM_SETRANGE32: u32 = WM_USER + 111;
pub(crate) const UDM_SETPOS32: u32 = WM_USER + 113;

pub(crate) const BCM_SETSHIELD: u32 = 0x160C;
pub(crate) const SW_SHOWNORMAL: i32 = 1;
pub(crate) const SW_SHOW: i32 = 5;
pub(crate) const GWLP_USERDATA: i32 = -21;
pub(crate) const MONITOR_DEFAULTTONEAREST: u32 = 2;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const RDW_INVALIDATE: u32 = 0x0001;
const RDW_ERASE: u32 = 0x0004;
const RDW_ALLCHILDREN: u32 = 0x0080;
const RDW_UPDATENOW: u32 = 0x0100;
const RDW_FRAME: u32 = 0x0400;
const IDC_HAND: usize = 32649;
const HWND_TOP: Handle = null_mut();
const IDOK: i32 = 1;
const IDCANCEL: i32 = 2;
const DT_WORDBREAK: u32 = 0x0000_0010;
const DT_SINGLELINE: u32 = 0x0000_0020;
const DT_CALCRECT: u32 = 0x0000_0400;
const DT_NOPREFIX: u32 = 0x0000_0800;

const OFN_ALLOWMULTISELECT: u32 = 0x0000_0200;
const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
const OFN_EXPLORER: u32 = 0x0008_0000;
const OFN_NODEREFERENCELINKS: u32 = 0x0010_0000;

const COINIT_APARTMENTTHREADED: u32 = 0x2;
const COINIT_DISABLE_OLE1DDE: u32 = 0x4;
const RPC_E_CHANGED_MODE: i32 = 0x8001_0106_u32 as i32;
const CLSCTX_INPROC_SERVER: u32 = 0x1;
const STGM_READ: u32 = 0x0000_0000;

#[derive(Clone, Copy)]
pub(crate) enum UiFontKind {
    Normal,
    Bold,
    Link,
}

#[repr(C)]
struct InitCommonControlsEx {
    size: u32,
    icc: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Msg {
    hwnd: Handle,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: Point,
}

#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    wnd_proc: WndProc,
    cls_extra: i32,
    wnd_extra: i32,
    instance: Handle,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u16,
    class_name: *const u16,
    icon_sm: Handle,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct PaintStruct {
    pub hdc: Handle,
    pub f_erase: i32,
    pub rc_paint: Rect,
    pub f_restore: i32,
    pub f_inc_update: i32,
    pub rgb_reserved: [u8; 32],
}

impl Default for PaintStruct {
    fn default() -> Self {
        Self {
            hdc: null_mut(),
            f_erase: 0,
            rc_paint: Rect::default(),
            f_restore: 0,
            f_inc_update: 0,
            rgb_reserved: [0; 32],
        }
    }
}

#[repr(C)]
struct MonitorInfo {
    cb_size: u32,
    rc_monitor: Rect,
    rc_work: Rect,
    dw_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LogFontW {
    lf_height: i32,
    lf_width: i32,
    lf_escapement: i32,
    lf_orientation: i32,
    lf_weight: i32,
    lf_italic: u8,
    lf_underline: u8,
    lf_strike_out: u8,
    lf_char_set: u8,
    lf_out_precision: u8,
    lf_clip_precision: u8,
    lf_quality: u8,
    lf_pitch_and_family: u8,
    lf_face_name: [u16; 32],
}

#[repr(C)]
struct NonClientMetricsW {
    cb_size: u32,
    i_border_width: i32,
    i_scroll_width: i32,
    i_scroll_height: i32,
    i_caption_width: i32,
    i_caption_height: i32,
    lf_caption_font: LogFontW,
    i_sm_caption_width: i32,
    i_sm_caption_height: i32,
    lf_sm_caption_font: LogFontW,
    i_menu_width: i32,
    i_menu_height: i32,
    lf_menu_font: LogFontW,
    lf_status_font: LogFontW,
    lf_message_font: LogFontW,
    i_padded_border_width: i32,
}

#[repr(C)]
struct ToolInfoW {
    cb_size: u32,
    u_flags: u32,
    hwnd: Handle,
    u_id: usize,
    rect: Rect,
    hinst: Handle,
    lpsz_text: *mut u16,
    lparam: isize,
    lp_reserved: Handle,
}

#[repr(C)]
struct OpenFileNameW {
    l_struct_size: u32,
    hwnd_owner: Handle,
    h_instance: Handle,
    lpstr_filter: *const u16,
    lpstr_custom_filter: *mut u16,
    n_max_cust_filter: u32,
    n_filter_index: u32,
    lpstr_file: *mut u16,
    n_max_file: u32,
    lpstr_file_title: *mut u16,
    n_max_file_title: u32,
    lpstr_initial_dir: *const u16,
    lpstr_title: *const u16,
    flags: u32,
    n_file_offset: u16,
    n_file_extension: u16,
    lpstr_def_ext: *const u16,
    l_cust_data: isize,
    lpfn_hook: Handle,
    lp_template_name: *const u16,
    pv_reserved: Handle,
    dw_reserved: u32,
    flags_ex: u32,
}

#[repr(C)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

const CLSID_SHELL_LINK: Guid = Guid {
    data1: 0x0002_1401,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

const IID_ISHELL_LINK_W: Guid = Guid {
    data1: 0x0002_14F9,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

const IID_IPERSIST_FILE: Guid = Guid {
    data1: 0x0000_010B,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

const _: () = {
    assert!(size_of::<InitCommonControlsEx>() == 8);
    assert!(size_of::<Msg>() == 48);
    assert!(size_of::<WndClassExW>() == 80);
    assert!(size_of::<PaintStruct>() == 72);
    assert!(size_of::<MonitorInfo>() == 40);
    assert!(size_of::<LogFontW>() == 92);
    assert!(size_of::<NonClientMetricsW>() == 504);
    assert!(size_of::<ToolInfoW>() == 72);
    assert!(size_of::<OpenFileNameW>() == 152);
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetProcAddress(module: Handle, proc_name: *const u8) -> *const c_void;
    fn MulDiv(number: i32, numerator: i32, denominator: i32) -> i32;
    fn GetCurrentThreadId() -> u32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EndMenu() -> i32;
    fn PostThreadMessageW(thread: u32, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn EnumThreadWindows(
        thread: u32,
        callback: unsafe extern "system" fn(Handle, isize) -> i32,
        lparam: isize,
    ) -> i32;
    fn GetClassNameW(hwnd: Handle, class_name: *mut u16, max_count: i32) -> i32;
}

/// The dialogs of Windows on this thread (class "#32770": the file dialog of the excluded
/// programs) are canceled, as their Cancel button would
unsafe extern "system" fn cancel_system_dialog(hwnd: Handle, _: isize) -> i32 {
    let mut buf = [0u16; 16];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len > 0
        && String::from_utf16_lossy(&buf[..len as usize]) == "#32770"
        && unsafe { IsWindowVisible(hwnd) } != 0
    {
        unsafe { PostMessageW(hwnd, WM_COMMAND, 2 /* IDCANCEL */, 0) };
    }
    1
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(wnd_class: *const WndClassExW) -> u16;
    fn DefWindowProcW(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Handle,
        menu: Handle,
        instance: Handle,
        param: Handle,
    ) -> Handle;
    fn DestroyWindow(hwnd: Handle) -> i32;
    fn IsWindow(hwnd: Handle) -> i32;
    fn IsWindowEnabled(hwnd: Handle) -> i32;
    fn IsWindowVisible(hwnd: Handle) -> i32;
    fn TrackMouseEvent(event_track: *mut TrackMouseEventInfo) -> i32;
    fn EnableWindow(hwnd: Handle, enable: i32) -> i32;
    fn ShowWindow(hwnd: Handle, cmd_show: i32) -> i32;
    fn SetForegroundWindow(hwnd: Handle) -> i32;
    fn SetFocus(hwnd: Handle) -> Handle;
    fn SetTimer(hwnd: Handle, id: usize, elapse: u32, func: Handle) -> usize;
    fn KillTimer(hwnd: Handle, id: usize) -> i32;
    fn SendMessageW(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn PostMessageW(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn SetWindowTextW(hwnd: Handle, text: *const u16) -> i32;
    fn GetWindowTextLengthW(hwnd: Handle) -> i32;
    fn GetWindowTextW(hwnd: Handle, text: *mut u16, max_count: i32) -> i32;
    fn GetWindowRect(hwnd: Handle, rect: *mut Rect) -> i32;
    fn SetWindowPos(
        hwnd: Handle,
        insert_after: Handle,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, ex_style: u32) -> i32;
    fn BeginPaint(hwnd: Handle, lp_paint: *mut PaintStruct) -> Handle;
    fn EndPaint(hwnd: Handle, lp_paint: *const PaintStruct) -> i32;
    fn RedrawWindow(
        hwnd: Handle,
        lprc_update: *const Rect,
        hrgn_update: Handle,
        flags: u32,
    ) -> i32;
    fn InvalidateRect(hwnd: Handle, rect: *const Rect, erase: i32) -> i32;
    fn SetWindowLongPtrW(hwnd: Handle, index: i32, new_long: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: Handle, index: i32) -> isize;
    fn LoadCursorW(instance: Handle, name: usize) -> Handle;
    fn SetCursor(hcursor: Handle) -> Handle;
    fn MonitorFromWindow(hwnd: Handle, flags: u32) -> Handle;
    fn MonitorFromPoint(pt: Point, flags: u32) -> Handle;
    fn GetMonitorInfoW(monitor: Handle, lpmi: *mut MonitorInfo) -> i32;
    fn GetCursorPos(pt: *mut Point) -> i32;
    fn GetDC(hwnd: Handle) -> Handle;
    fn ReleaseDC(hwnd: Handle, hdc: Handle) -> i32;
    fn DrawTextW(
        hdc: Handle,
        lpch_text: *const u16,
        cch_text: i32,
        lprc: *mut Rect,
        format: u32,
    ) -> i32;
    fn GetMessageW(msg: *mut Msg, hwnd: Handle, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn IsDialogMessageW(dlg: Handle, msg: *const Msg) -> i32;
    fn SystemParametersInfoW(
        action: u32,
        param: u32,
        pv_param: *mut c_void,
        win_ini: u32,
    ) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateFontIndirectW(lplf: *const LogFontW) -> Handle;
    fn SelectObject(hdc: Handle, object: Handle) -> Handle;
    fn DeleteObject(object: Handle) -> i32;
}

#[link(name = "comctl32")]
unsafe extern "system" {
    fn InitCommonControlsEx(picce: *const InitCommonControlsEx) -> i32;
}

#[link(name = "comdlg32")]
unsafe extern "system" {
    fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        hwnd: Handle,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show_cmd: i32,
    ) -> Handle;
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: Handle, co_init: u32) -> i32;
    fn CoUninitialize();
    fn CoCreateInstance(
        rclsid: *const Guid,
        outer: Handle,
        cls_context: u32,
        riid: *const Guid,
        ppv: *mut Handle,
    ) -> i32;
}

pub(crate) fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn module_handle() -> Handle {
    unsafe { GetModuleHandleW(null()) }
}

pub(crate) fn init_ui_process() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| unsafe {
        let user32 = to_wide("user32.dll");
        let h_user32 = GetModuleHandleW(user32.as_ptr());
        if !h_user32.is_null() {
            let proc = GetProcAddress(h_user32, c"SetProcessDpiAwarenessContext".as_ptr().cast());
            if !proc.is_null() {
                let set_dpi = std::mem::transmute::<*const c_void, SetProcessDpiAwarenessContextFn>(
                    proc,
                );
                let _ = set_dpi(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            }
        }
        let icc = InitCommonControlsEx {
            size: size_of::<InitCommonControlsEx>() as u32,
            icc: ICC_WIN95_CLASSES | ICC_STANDARD_CLASSES | ICC_UPDOWN_CLASS,
        };
        let _ = InitCommonControlsEx(&icc);
    });
}

pub(crate) fn dpi_for_window(hwnd: Handle) -> u32 {
    static GET_DPI: OnceLock<Option<GetDpiForWindowFn>> = OnceLock::new();
    if hwnd.is_null() {
        return 96;
    }
    let f = *GET_DPI.get_or_init(|| unsafe {
        let user32 = to_wide("user32.dll");
        let h_user32 = GetModuleHandleW(user32.as_ptr());
        if h_user32.is_null() {
            return None;
        }
        let proc = GetProcAddress(h_user32, c"GetDpiForWindow".as_ptr().cast());
        (!proc.is_null()).then(|| std::mem::transmute::<*const c_void, GetDpiForWindowFn>(proc))
    });
    if let Some(get_dpi) = f {
        let dpi = unsafe { get_dpi(hwnd) };
        if dpi > 0 {
            return dpi;
        }
    }
    96
}

pub(crate) fn scale(px: i32, dpi: u32) -> i32 {
    unsafe { MulDiv(px, dpi as i32, 96) }
}

pub(crate) fn adjust_window_size(
    style: u32,
    ex_style: u32,
    dpi: u32,
    client_w: i32,
    client_h: i32,
) -> (i32, i32) {
    static ADJUST_FOR_DPI: OnceLock<Option<AdjustWindowRectExForDpiFn>> = OnceLock::new();
    let f = *ADJUST_FOR_DPI.get_or_init(|| unsafe {
        let user32 = to_wide("user32.dll");
        let h_user32 = GetModuleHandleW(user32.as_ptr());
        if h_user32.is_null() {
            return None;
        }
        let proc = GetProcAddress(h_user32, c"AdjustWindowRectExForDpi".as_ptr().cast());
        (!proc.is_null())
            .then(|| std::mem::transmute::<*const c_void, AdjustWindowRectExForDpiFn>(proc))
    });
    let mut rc = Rect {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    let ok = if let Some(adjust) = f {
        unsafe { adjust(&mut rc, style, 0, ex_style, dpi) != 0 }
    } else {
        unsafe { AdjustWindowRectEx(&mut rc, style, 0, ex_style) != 0 }
    };
    if ok {
        (rc.right - rc.left, rc.bottom - rc.top)
    } else {
        (client_w + scale(16, dpi), client_h + scale(39, dpi))
    }
}

pub(crate) fn register_window_class(
    class_name: &str,
    wnd_proc: WndProc,
    cursor_id: usize,
    icon: Handle,
) -> bool {
    let class_w = to_wide(class_name);
    let cursor = unsafe { LoadCursorW(null_mut(), cursor_id) };
    let wx = WndClassExW {
        cb_size: size_of::<WndClassExW>() as u32,
        style: 0,
        wnd_proc,
        cls_extra: 0,
        wnd_extra: 0,
        instance: module_handle(),
        icon,
        cursor,
        background: null_mut(),
        menu_name: null(),
        class_name: class_w.as_ptr(),
        icon_sm: icon,
    };
    if unsafe { RegisterClassExW(&wx) } != 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(ERROR_CLASS_ALREADY_EXISTS as i32)
}

pub(crate) fn create_window(
    ex_style: u32,
    class_name: &str,
    title: &str,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    parent: Handle,
    id: usize,
    param: Handle,
) -> Handle {
    let class_w = to_wide(class_name);
    let title_w = to_wide(title);
    unsafe {
        CreateWindowExW(
            ex_style,
            class_w.as_ptr(),
            title_w.as_ptr(),
            style,
            x,
            y,
            w,
            h,
            parent,
            id as Handle,
            module_handle(),
            param,
        )
    }
}

pub(crate) fn def_window_proc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

pub(crate) fn destroy_window(hwnd: Handle) {
    if !hwnd.is_null() {
        unsafe { DestroyWindow(hwnd) };
    }
}

pub(crate) fn is_window(hwnd: Handle) -> bool {
    !hwnd.is_null() && unsafe { IsWindow(hwnd) != 0 }
}

pub(crate) fn set_user_data<T>(hwnd: Handle, ptr: *mut T) {
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize) };
}

pub(crate) unsafe fn user_data<T>(hwnd: Handle) -> *mut T {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut T }
}


pub(crate) fn set_window_icon(hwnd: Handle, small: bool, icon: Handle) {
    let _ = send_msg(hwnd, WM_SETICON, usize::from(!small), icon as isize);
}

pub(crate) fn set_timer(hwnd: Handle, id: usize, ms: u32) {
    unsafe { SetTimer(hwnd, id, ms, null_mut()) };
}

pub(crate) fn kill_timer(hwnd: Handle, id: usize) {
    unsafe { KillTimer(hwnd, id) };
}

pub(crate) fn invalidate(hwnd: Handle) {
    if !hwnd.is_null() {
        unsafe { InvalidateRect(hwnd, null(), 1) };
    }
}

pub(crate) fn begin_paint(hwnd: Handle, ps: &mut PaintStruct) -> Handle {
    unsafe { BeginPaint(hwnd, ps) }
}

pub(crate) fn end_paint(hwnd: Handle, ps: &PaintStruct) {
    unsafe {
        let _ = EndPaint(hwnd, ps);
    }
}

pub(crate) fn redraw_window_all(hwnd: Handle, include_frame: bool) {
    if hwnd.is_null() {
        return;
    }
    let mut flags = RDW_ERASE | RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW;
    if include_frame {
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        flags |= RDW_FRAME;
    }
    unsafe {
        let _ = RedrawWindow(hwnd, null(), null_mut(), flags);
    }
}

pub(crate) fn set_hand_cursor_now() {
    unsafe {
        let hand = LoadCursorW(null_mut(), IDC_HAND);
        if !hand.is_null() {
            let _ = SetCursor(hand);
        }
    }
}

pub(crate) fn set_foreground(hwnd: Handle) {
    if !hwnd.is_null() {
        unsafe { SetForegroundWindow(hwnd) };
    }
}

pub(crate) fn set_focus(hwnd: Handle) {
    if !hwnd.is_null() {
        unsafe { SetFocus(hwnd) };
    }
}

pub(crate) fn move_window(hwnd: Handle, x: i32, y: i32, w: i32, h: i32) {
    if !hwnd.is_null() {
        unsafe { SetWindowPos(hwnd, HWND_TOP, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    }
}

pub(crate) fn center_window(hwnd: Handle, owner: Handle) {
    if hwnd.is_null() {
        return;
    }
    let mut rc = Rect::default();
    unsafe { GetWindowRect(hwnd, &mut rc) };
    let w = rc.right - rc.left;
    let h = rc.bottom - rc.top;

    let monitor = if !owner.is_null() && is_window(owner) {
        unsafe { MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST) }
    } else {
        let mut pt = Point::default();
        unsafe {
            GetCursorPos(&mut pt);
            MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
        }
    };
    let mut mi = MonitorInfo {
        cb_size: size_of::<MonitorInfo>() as u32,
        rc_monitor: Rect::default(),
        rc_work: Rect {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 720,
        },
        dw_flags: 0,
    };
    if !monitor.is_null() {
        unsafe { GetMonitorInfoW(monitor, &mut mi) };
    }
    let work = mi.rc_work;
    let mut target_x = work.left + (work.right - work.left - w) / 2;
    let mut target_y = work.top + (work.bottom - work.top - h) / 2;

    if !owner.is_null() && is_window(owner) {
        let mut orc = Rect::default();
        if unsafe { GetWindowRect(owner, &mut orc) } != 0 {
            target_x = orc.left + (orc.right - orc.left - w) / 2;
            target_y = orc.top + (orc.bottom - orc.top - h) / 2;
        }
    }

    let x = target_x.clamp(work.left, (work.right - w).max(work.left));
    let y = target_y.clamp(work.top, (work.bottom - h).max(work.top));
    unsafe {
        SetWindowPos(hwnd, HWND_TOP, x, y, 0, 0, SWP_NOZORDER | 0x0001 /* SWP_NOSIZE */ | SWP_NOACTIVATE);
    }
}

thread_local! {
    // The nested message loops running on the UI thread (modal dialogs, the file dialog, the tray
    // menu): a window procedure is waiting below each, with references to its state
    static MODAL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    // Set when W10Wheel exits during one: they end, and no new one starts
    static END_MODALS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Held while a nested message loop runs (see is_modal_active)
pub(crate) struct ModalGuard(());

impl ModalGuard {
    pub(crate) fn enter() -> Self {
        MODAL_DEPTH.with(|d| d.set(d.get() + 1));
        Self(())
    }
}

impl Drop for ModalGuard {
    fn drop(&mut self) {
        MODAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// A nested message loop is running: the window procedure that started it is still on the stack,
/// so its window and state must not be destroyed yet (it uses them when the loop returns). An exit
/// received meanwhile (e.g. --sendExit, the end of the session) waits for them to return.
pub(crate) fn is_modal_active() -> bool {
    MODAL_DEPTH.with(|d| d.get() > 0)
}

/// W10Wheel exits: the modal dialogs and the tray menu end (as canceled), and none starts any more
pub(crate) fn end_modals() {
    END_MODALS.with(|e| e.set(true));
    unsafe {
        EndMenu();
        // The file dialog of Windows runs its own loop, which the flag above does not end
        EnumThreadWindows(GetCurrentThreadId(), cancel_system_dialog, 0);
        // Wakes the loops waiting in GetMessage, which then see it
        PostThreadMessageW(GetCurrentThreadId(), 0 /* WM_NULL */, 0, 0);
    }
}

pub(crate) fn modals_ending() -> bool {
    END_MODALS.with(|e| e.get())
}

pub(crate) fn run_modal_loop(dialog_hwnd: Handle, owner: Handle, done: &std::cell::Cell<bool>) {
    let _guard = ModalGuard::enter();
    if modals_ending() {
        // Exiting: not shown, as canceled
        destroy_window(dialog_hwnd);
        return;
    }
    if !owner.is_null() && is_window(owner) {
        enable_window(owner, false);
    }
    show_window(dialog_hwnd, SW_SHOW);
    set_foreground(dialog_hwnd);

    let mut msg = MaybeUninit::<Msg>::uninit();
    while !done.get() && !modals_ending() && is_window(dialog_hwnd) {
        let res = unsafe { GetMessageW(msg.as_mut_ptr(), null_mut(), 0, 0) };
        if res <= 0 {
            if res == 0 {
                let m = unsafe { msg.assume_init() };
                unsafe { PostMessageW(null_mut(), 0x0012 /* WM_QUIT */, m.wparam, m.lparam) };
            }
            break;
        }
        let m = unsafe { msg.assume_init() };
        if unsafe { IsDialogMessageW(dialog_hwnd, &m) == 0 } {
            unsafe {
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
    }

    if !owner.is_null() && is_window(owner) {
        enable_window(owner, true);
        set_foreground(owner);
    }
    destroy_window(dialog_hwnd);
}

pub(crate) fn run_main_message_loop(is_dialog_target: impl Fn() -> Handle) -> i32 {
    let mut msg = MaybeUninit::<Msg>::uninit();
    loop {
        let res = unsafe { GetMessageW(msg.as_mut_ptr(), null_mut(), 0, 0) };
        if res <= 0 {
            return if res == 0 {
                unsafe { msg.assume_init().wparam as i32 }
            } else {
                1
            };
        }
        let m = unsafe { msg.assume_init() };
        let dlg = is_dialog_target();
        if !dlg.is_null() && unsafe { IsDialogMessageW(dlg, &m) != 0 } {
            continue;
        }
        unsafe {
            TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
}

pub(crate) const fn dialog_ids() -> (i32, i32) {
    (IDOK, IDCANCEL)
}

pub(crate) struct FontHandle(Handle);

impl FontHandle {
    pub fn raw(&self) -> Handle {
        self.0
    }
}

impl Drop for FontHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { DeleteObject(self.0) };
            self.0 = null_mut();
        }
    }
}

pub(crate) fn create_ui_font(dpi: u32, kind: UiFontKind) -> Option<FontHandle> {
    let mut metrics = MaybeUninit::<NonClientMetricsW>::zeroed();
    let ptr = metrics.as_mut_ptr();
    unsafe {
        (*ptr).cb_size = size_of::<NonClientMetricsW>() as u32;
    }
    let mut lf = if unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            size_of::<NonClientMetricsW>() as u32,
            ptr.cast::<c_void>(),
            0,
        )
    } != 0
    {
        unsafe { metrics.assume_init() }.lf_message_font
    } else {
        let mut fallback = LogFontW {
            lf_height: -scale(12, dpi),
            lf_width: 0,
            lf_escapement: 0,
            lf_orientation: 0,
            lf_weight: FW_NORMAL,
            lf_italic: 0,
            lf_underline: 0,
            lf_strike_out: 0,
            lf_char_set: DEFAULT_CHARSET,
            lf_out_precision: 0,
            lf_clip_precision: 0,
            lf_quality: CLEARTYPE_QUALITY,
            lf_pitch_and_family: 0,
            lf_face_name: [0; 32],
        };
        let face = to_wide("Segoe UI");
        let n = face.len().min(fallback.lf_face_name.len());
        fallback.lf_face_name[..n].copy_from_slice(&face[..n]);
        fallback
    };
    lf.lf_height = -((scale(12, dpi).abs()).max(9));
    lf.lf_weight = match kind {
        UiFontKind::Bold => FW_BOLD,
        UiFontKind::Normal | UiFontKind::Link => FW_NORMAL,
    };
    lf.lf_underline = u8::from(matches!(kind, UiFontKind::Link));
    let font = unsafe { CreateFontIndirectW(&lf) };
    (!font.is_null()).then_some(FontHandle(font))
}

pub(crate) fn send_msg(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize {
    unsafe { SendMessageW(hwnd, msg, wparam, lparam) }
}

pub(crate) fn set_font(hwnd: Handle, font: Handle) {
    if !hwnd.is_null() && !font.is_null() {
        let _ = send_msg(hwnd, WM_SETFONT, font as usize, 1);
    }
}

fn measure_text_rect(
    hwnd: Handle,
    font: Handle,
    text: &str,
    max_width: Option<i32>,
) -> (i32, i32) {
    if text.is_empty() {
        return (0, 0);
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return (0, 0);
    }
    let hdc = unsafe { GetDC(hwnd) };
    if hdc.is_null() {
        return (0, 0);
    }
    let old_font = if !font.is_null() {
        unsafe { SelectObject(hdc, font) }
    } else {
        null_mut()
    };
    let (flags, right) = match max_width {
        Some(w) => (DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX, w.max(1)),
        None => (DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX, 0),
    };
    let mut rc = Rect {
        left: 0,
        top: 0,
        right,
        bottom: 0,
    };
    unsafe {
        let _ = DrawTextW(hdc, wide.as_ptr(), wide.len() as i32, &mut rc, flags);
        if !old_font.is_null() {
            let _ = SelectObject(hdc, old_font);
        }
        let _ = ReleaseDC(hwnd, hdc);
    }
    ((rc.right - rc.left).max(0), (rc.bottom - rc.top).max(0))
}

pub(crate) fn measure_text_width(hwnd: Handle, font: Handle, text: &str) -> i32 {
    measure_text_rect(hwnd, font, text, None).0
}

pub(crate) fn measure_text_height(
    hwnd: Handle,
    font: Handle,
    text: &str,
    max_width: i32,
) -> i32 {
    measure_text_rect(hwnd, font, text, Some(max_width)).1
}

pub(crate) fn set_window_text(hwnd: Handle, text: &str) {
    if hwnd.is_null() {
        return;
    }
    let wide = to_wide(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, wide.as_ptr());
    }
}

pub(crate) fn get_window_text(hwnd: Handle) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if copied <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..copied as usize])
}

pub(crate) fn enable_window(hwnd: Handle, enable: bool) {
    if !hwnd.is_null() {
        unsafe {
            let _ = EnableWindow(hwnd, i32::from(enable));
        }
    }
}

pub(crate) fn show_window(hwnd: Handle, cmd: i32) {
    if !hwnd.is_null() {
        unsafe {
            let _ = ShowWindow(hwnd, cmd);
        }
    }
}

pub(crate) fn set_button_shield(hwnd: Handle, show: bool) {
    let _ = send_msg(hwnd, BCM_SETSHIELD, 0, isize::from(show));
    // Also drawn by the dark custom draw (which skips the drawing of the button)
    crate::theme::set_button_shield_prop(hwnd, show);
}

pub(crate) fn open_shell_target(target: &str) -> bool {
    // ShellExecute can pump messages (COM) while it runs: an exit meanwhile waits for it
    let _guard = ModalGuard::enter();
    let verb = to_wide("open");
    let file = to_wide(target);
    let res = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    (res as usize) > 32
}

// The ID of the tool of the tracking tooltip (the one shown for the disabled controls)
const DISABLED_TIP_TOOL_ID: usize = 1;

fn create_tooltip_window(owner: Handle, dpi: u32) -> Handle {
    let class = to_wide("tooltips_class32");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            class.as_ptr(),
            null(),
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            owner,
            null_mut(),
            module_handle(),
            null_mut(),
        )
    };
    if !hwnd.is_null() {
        send_msg(hwnd, TTM_ACTIVATE, 1, 0);
        send_msg(hwnd, TTM_SETMAXTIPWIDTH, 0, scale(360, dpi) as isize);
        send_msg(hwnd, TTM_SETDELAYTIME, TTDT_AUTOPOP, TIP_AUTOPOP_MS as isize);
    }
    hwnd
}

pub(crate) struct TooltipManager {
    hwnd: Handle,
    owner: Handle,
    // The text of each control's tip (also to show it while the control is disabled)
    registered: HashMap<usize, String>,
    // A disabled control gets no mouse messages, so the tooltip above never shows its tip: this
    // one is shown by the owner instead (show_tracking), as the .NET version does
    track_hwnd: Handle,
}

impl TooltipManager {
    pub fn new(owner: Handle, dpi: u32, dark: bool) -> Self {
        let hwnd = create_tooltip_window(owner, dpi);
        let track_hwnd = create_tooltip_window(owner, dpi);
        if !track_hwnd.is_null() {
            let mut empty = [0u16; 1];
            let mut ti = ToolInfoW {
                cb_size: size_of::<ToolInfoW>() as u32,
                u_flags: TTF_TRACK | TTF_ABSOLUTE,
                hwnd: owner,
                u_id: DISABLED_TIP_TOOL_ID,
                rect: Rect::default(),
                hinst: null_mut(),
                lpsz_text: empty.as_mut_ptr(),
                lparam: 0,
                lp_reserved: null_mut(),
            };
            send_msg(track_hwnd, TTM_ADDTOOLW, 0, (&raw mut ti) as isize);
        }
        let mgr = Self {
            hwnd,
            owner,
            registered: HashMap::new(),
            track_hwnd,
        };
        mgr.apply_theme(dark);
        mgr
    }

    pub fn apply_theme(&self, dark: bool) {
        for hwnd in [self.hwnd, self.track_hwnd] {
            crate::theme::apply_control_theme(hwnd, dark, crate::theme::ControlTheme::Explorer);
        }
    }

    pub fn update_dpi(&self, dpi: u32) {
        for hwnd in [self.hwnd, self.track_hwnd] {
            if !hwnd.is_null() {
                send_msg(hwnd, TTM_SETMAXTIPWIDTH, 0, scale(360, dpi) as isize);
            }
        }
    }

    pub fn set_tip(&mut self, control: Handle, text: &str) {
        if self.hwnd.is_null() || control.is_null() {
            return;
        }
        let mut wide = to_wide(text);
        let mut ti = ToolInfoW {
            cb_size: size_of::<ToolInfoW>() as u32,
            u_flags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd: self.owner,
            u_id: control as usize,
            rect: Rect::default(),
            hinst: null_mut(),
            lpsz_text: wide.as_mut_ptr(),
            lparam: 0,
            lp_reserved: null_mut(),
        };
        if self.registered.contains_key(&(control as usize)) {
            send_msg(self.hwnd, TTM_UPDATETIPTEXTW, 0, (&raw mut ti) as isize);
            self.registered.insert(control as usize, text.to_owned());
        } else {
            let ok = send_msg(self.hwnd, TTM_ADDTOOLW, 0, (&raw mut ti) as isize);
            if ok != 0 {
                self.registered.insert(control as usize, text.to_owned());
            }
        }
    }

    /// The visible, disabled control with a tip at `screen_pt` (the controls of the owner are not
    /// nested: their rectangles are looked up). None if there is none, or the control there is
    /// enabled (its own tooltip shows then).
    pub fn disabled_tool_at(&self, screen_pt: Point) -> Option<Handle> {
        self.registered.keys().copied().map(|c| c as Handle).find(|&control| unsafe {
            let mut rect = Rect::default();
            IsWindowVisible(control) != 0
                && IsWindowEnabled(control) == 0
                && GetWindowRect(control, &mut rect) != 0
                && screen_pt.x >= rect.left
                && screen_pt.x < rect.right
                && screen_pt.y >= rect.top
                && screen_pt.y < rect.bottom
        })
    }

    /// The text of the tip of a control
    pub fn tip_text(&self, control: Handle) -> Option<&str> {
        self.registered.get(&(control as usize)).map(String::as_str)
    }

    /// Shows the tip of a (disabled) control at a point of the screen (below the mouse)
    pub fn show_tracking(&self, control: Handle, screen_pt: Point) {
        let Some(text) = self.tip_text(control) else {
            return;
        };
        if self.track_hwnd.is_null() {
            return;
        }
        let mut wide = to_wide(text);
        let mut ti = ToolInfoW {
            cb_size: size_of::<ToolInfoW>() as u32,
            u_flags: TTF_TRACK | TTF_ABSOLUTE,
            hwnd: self.owner,
            u_id: DISABLED_TIP_TOOL_ID,
            rect: Rect::default(),
            hinst: null_mut(),
            lpsz_text: wide.as_mut_ptr(),
            lparam: 0,
            lp_reserved: null_mut(),
        };
        send_msg(self.track_hwnd, TTM_UPDATETIPTEXTW, 0, (&raw mut ti) as isize);
        let pos = ((screen_pt.y as u32 & 0xFFFF) << 16) | (screen_pt.x as u32 & 0xFFFF);
        send_msg(self.track_hwnd, TTM_TRACKPOSITION, 0, pos as isize);
        send_msg(self.track_hwnd, TTM_TRACKACTIVATE, 1, (&raw mut ti) as isize);
    }

    pub fn hide_tracking(&self) {
        if self.track_hwnd.is_null() {
            return;
        }
        let mut ti = ToolInfoW {
            cb_size: size_of::<ToolInfoW>() as u32,
            u_flags: TTF_TRACK | TTF_ABSOLUTE,
            hwnd: self.owner,
            u_id: DISABLED_TIP_TOOL_ID,
            rect: Rect::default(),
            hinst: null_mut(),
            lpsz_text: null_mut(),
            lparam: 0,
            lp_reserved: null_mut(),
        };
        send_msg(self.track_hwnd, TTM_TRACKACTIVATE, 0, (&raw mut ti) as isize);
    }
}

/// Asks for WM_MOUSELEAVE when the mouse leaves the client area of `hwnd` (also onto one of its
/// child windows that gets the mouse messages)
pub(crate) fn track_mouse_leave(hwnd: Handle) {
    let mut info = TrackMouseEventInfo {
        cb_size: size_of::<TrackMouseEventInfo>() as u32,
        flags: TME_LEAVE,
        hwnd_track: hwnd,
        hover_time: 0,
    };
    unsafe { TrackMouseEvent(&mut info) };
}

pub(crate) fn cursor_pos() -> Point {
    let mut pt = Point::default();
    unsafe { GetCursorPos(&mut pt) };
    pt
}

impl Drop for TooltipManager {
    fn drop(&mut self) {
        for hwnd in [&mut self.hwnd, &mut self.track_hwnd] {
            if !hwnd.is_null() {
                unsafe { DestroyWindow(*hwnd) };
                *hwnd = null_mut();
            }
        }
    }
}

struct ComPtr(Handle);

impl ComPtr {
    fn vtable(&self) -> *const usize {
        unsafe { *(self.0.cast::<*const usize>()) }
    }
}

impl Drop for ComPtr {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let release_ptr = *self.vtable().add(2);
                let release: unsafe extern "system" fn(Handle) -> u32 =
                    std::mem::transmute(release_ptr);
                let _ = release(self.0);
            }
            self.0 = null_mut();
        }
    }
}

struct ComApartment {
    should_uninit: bool,
}

impl ComApartment {
    fn init() -> Option<Self> {
        let hr =
            unsafe { CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        if hr >= 0 {
            Some(Self { should_uninit: true })
        } else if hr == RPC_E_CHANGED_MODE {
            Some(Self {
                should_uninit: false,
            })
        } else {
            None
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.should_uninit {
            unsafe { CoUninitialize() };
        }
    }
}

fn resolve_shortcut_once(path: &Path) -> Option<PathBuf> {
    let mut link_raw: Handle = null_mut();
    let hr = unsafe {
        CoCreateInstance(
            &CLSID_SHELL_LINK,
            null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISHELL_LINK_W,
            &mut link_raw,
        )
    };
    if hr < 0 || link_raw.is_null() {
        return None;
    }
    let link = ComPtr(link_raw);

    let mut persist_raw: Handle = null_mut();
    let qi_res = unsafe {
        let qi_ptr = *link.vtable();
        let qi: unsafe extern "system" fn(Handle, *const Guid, *mut Handle) -> i32 =
            std::mem::transmute(qi_ptr);
        qi(link.0, &IID_IPERSIST_FILE, &mut persist_raw)
    };
    if qi_res < 0 || persist_raw.is_null() {
        return None;
    }
    let persist = ComPtr(persist_raw);

    let wide_path: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let load_res = unsafe {
        let load_ptr = *persist.vtable().add(5);
        let load: unsafe extern "system" fn(Handle, *const u16, u32) -> i32 =
            std::mem::transmute(load_ptr);
        load(persist.0, wide_path.as_ptr(), STGM_READ)
    };
    if load_res < 0 {
        return None;
    }

    let mut target_buf = vec![0u16; 32768];
    let get_path_res = unsafe {
        let get_path_ptr = *link.vtable().add(3);
        let get_path: unsafe extern "system" fn(Handle, *mut u16, i32, Handle, u32) -> i32 =
            std::mem::transmute(get_path_ptr);
        get_path(
            link.0,
            target_buf.as_mut_ptr(),
            target_buf.len() as i32,
            null_mut(),
            0,
        )
    };
    if get_path_res < 0 {
        return None;
    }
    let end = target_buf
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(target_buf.len());
    if end == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&target_buf[..end])))
}

fn resolve_exe_target(path: &Path) -> Option<String> {
    let mut current = path.to_path_buf();
    for _ in 0..8 {
        let is_lnk = current
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
        if !is_lnk {
            break;
        }
        current = resolve_shortcut_once(&current)?;
    }
    let is_exe = current
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if is_exe && current.is_file() {
        Some(current.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn parse_open_file_buffer(buf: &[u16]) -> Vec<PathBuf> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    while start < buf.len() {
        let rel_end = buf[start..].iter().position(|&c| c == 0).unwrap_or(0);
        if rel_end == 0 {
            break;
        }
        parts.push(String::from_utf16_lossy(&buf[start..start + rel_end]));
        start += rel_end + 1;
    }
    match parts.as_slice() {
        [] => Vec::new(),
        [single] => vec![PathBuf::from(single)],
        [dir, files @ ..] => {
            let base = PathBuf::from(dir);
            files.iter().map(|name| base.join(name)).collect()
        }
    }
}

pub(crate) fn filter_exe_selection(paths: &[PathBuf]) -> (Vec<String>, Vec<String>) {
    let _com = ComApartment::init();
    let mut valid = Vec::new();
    let mut rejected = Vec::new();
    for path in paths {
        if let Some(exe) = resolve_exe_target(path) {
            if !valid
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&exe))
            {
                valid.push(exe);
            }
        } else {
            rejected.push(path.to_string_lossy().into_owned());
        }
    }
    (valid, rejected)
}

/// The programs to exclude, chosen in the file dialog of Windows: only the programs (*.exe) are
/// listed, as in the .NET version. A file that is not one (e.g. its name typed in the box) is
/// returned apart (the second list), to be told and not added.
pub(crate) fn pick_excluded_programs(
    owner: Handle,
    title: &str,
    exe_filter_label: &str,
) -> Option<(Vec<String>, Vec<String>)> {
    let mut filter_w = Vec::new();
    filter_w.extend(exe_filter_label.encode_utf16());
    filter_w.push(0);
    filter_w.extend("*.exe".encode_utf16());
    filter_w.push(0);
    filter_w.push(0);

    // The file dialog runs its own message loop (see is_modal_active); not opened when exiting
    let _guard = ModalGuard::enter();
    if modals_ending() {
        return None;
    }
    let title_w = to_wide(title);
    let mut file_buf = vec![0u16; 32 * 1024];

    let mut ofn = OpenFileNameW {
        l_struct_size: size_of::<OpenFileNameW>() as u32,
        hwnd_owner: owner,
        h_instance: null_mut(),
        lpstr_filter: filter_w.as_ptr(),
        lpstr_custom_filter: null_mut(),
        n_max_cust_filter: 0,
        n_filter_index: 1,
        lpstr_file: file_buf.as_mut_ptr(),
        n_max_file: file_buf.len() as u32,
        lpstr_file_title: null_mut(),
        n_max_file_title: 0,
        lpstr_initial_dir: null(),
        lpstr_title: title_w.as_ptr(),
        flags: OFN_ALLOWMULTISELECT
            | OFN_EXPLORER
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_NODEREFERENCELINKS,
        n_file_offset: 0,
        n_file_extension: 0,
        lpstr_def_ext: null(),
        l_cust_data: 0,
        lpfn_hook: null_mut(),
        lp_template_name: null(),
        pv_reserved: null_mut(),
        dw_reserved: 0,
        flags_ex: 0,
    };

    if unsafe { GetOpenFileNameW(&mut ofn) } == 0 {
        return None;
    }
    let selected = parse_open_file_buffer(&file_buf);
    Some(filter_exe_selection(&selected))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn parses_single_and_multiselect_open_file_buffers() {
        let single: Vec<u16> = "C:\\Apps\\one.exe\0\0".encode_utf16().collect();
        assert_eq!(
            parse_open_file_buffer(&single),
            vec![PathBuf::from("C:\\Apps\\one.exe")]
        );

        let multi: Vec<u16> = "C:\\Apps\0first.exe\0second.txt\0\0"
            .encode_utf16()
            .collect();
        assert_eq!(
            parse_open_file_buffer(&multi),
            vec![
                PathBuf::from("C:\\Apps\\first.exe"),
                PathBuf::from("C:\\Apps\\second.txt"),
            ]
        );
    }

    #[test]
    fn filter_exe_selection_keeps_existing_exe_and_rejects_non_exe() -> std::io::Result<()> {
        let exe = env::current_exe()?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tmp_txt =
            env::temp_dir().join(format!("w10wheel-nonexe-{}-{ts}.txt", std::process::id()));
        fs::write(&tmp_txt, "not an exe")?;

        let (valid, rejected) = filter_exe_selection(&[exe.clone(), tmp_txt.clone()]);
        let _ = fs::remove_file(&tmp_txt);

        assert_eq!(valid, vec![exe.to_string_lossy().into_owned()]);
        assert_eq!(rejected, vec![tmp_txt.to_string_lossy().into_owned()]);
        Ok(())
    }
}
