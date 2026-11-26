use std::env;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{MaybeUninit, align_of};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use crate::cli::build_restart_admin_args;
use crate::config::{ProcessPriority, UiLanguage};
use crate::exclusions::ExclusionMatcher;
use crate::ipc;
use crate::sender::{MouseButton, Point, SendPacket, SendSink};
use crate::trigger::{HookDecision, HookKeyEvent, HookMouseEvent, HookMouseKind, KeyAction};
use crate::wm_app::COMMAND_WAKE_MESSAGE;
use crate::{EngineConfig, SCROLL_ACTIVITY_MESSAGE, ScrollAxis};

type Handle = *mut c_void;
type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;
type WndProc = unsafe extern "system" fn(Handle, u32, usize, isize) -> isize;

const GA_ROOT: u32 = 2;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_CANCELLED: u32 = 1223;
const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
const INITIAL_IMAGE_PATH_LEN: usize = 512;
const MAX_IMAGE_PATH_LEN: usize = 32768;
const WH_KEYBOARD_LL: i32 = 13;
const WH_MOUSE_LL: i32 = 14;
const HC_ACTION: i32 = 0;
const KEY_INJECTED_FLAGS: u32 = 0x12; // LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED
const MOUSE_INJECTED_FLAGS: u32 = 0x03; // LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED
const WM_ENDSESSION: u32 = 0x0016;
const WM_INPUT: u32 = 0x00ff;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_XBUTTONDOWN: u32 = 0x020B;
const WM_XBUTTONUP: u32 = 0x020C;
const WM_MOUSEHWHEEL: u32 = 0x020E;
const XBUTTON1: u16 = 0x0001;
const XBUTTON2: u16 = 0x0002;
const POLL_WAIT_TIMER: usize = 1;
const MSGFLT_ADD: u32 = 1;
const SW_SHOWNORMAL: i32 = 1;
const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_ELEVATION_CLASS: u32 = 20;
const RIDEV_REMOVE: u32 = 0x01;
const RIDEV_INPUTSINK: u32 = 0x0100;
const RID_INPUT: u32 = 0x1000_0003;
const RIM_TYPEMOUSE: u32 = 0;
const MOUSE_MOVE_ABSOLUTE: u16 = 0x01;
const INPUT_MOUSE: u32 = 0;
const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
const MOUSEEVENTF_XDOWN: u32 = 0x0080;
const MOUSEEVENTF_XUP: u32 = 0x0100;
const MOUSEEVENTF_WHEEL: u32 = 0x0800;
const MOUSEEVENTF_HWHEEL: u32 = 0x1000;
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_MENU: i32 = 0x12;
const VK_ESCAPE: i32 = 0x1B;
const NORMAL_PRIORITY_CLASS: u32 = 0x0000_0020;
const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;
const HIGH_PRIORITY_CLASS: u32 = 0x0000_0080;
const LANG_JAPANESE: u16 = 0x0011;
const LOCK_FILE_NAME: &str = "W10Wheel.lock";
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) enum Command {
    Stop,
    Pause(bool),
    /// A setting changed in the settings dialog
    Config(Box<EngineConfig>),
    /// The properties loaded (Reload, switching, adding, deleting or resetting them)
    Load(Box<EngineConfig>),
    InitState,
    /// Installs the hooks again (after a resume or an unlock), to be first in the chain again
    ReinstallHooks,
    /// The input was not seen (a lock, an unlock, a suspend or a resume): the triggers are reset
    InputLost,
    WatchScroll(usize),
}

pub(crate) enum LoopEvent {
    RawMotion { x: i32, y: i32, time: u32 },
    PollTimeout,
    Wake,
}

#[repr(C)]
struct Msg {
    hwnd: Handle,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: Point,
}

#[repr(C)]
struct MouseHookInfo {
    point: Point,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[repr(C)]
struct KeyboardHookInfo {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[repr(C)]
struct RawInputDevice {
    usage_page: u16,
    usage: u16,
    flags: u32,
    target: Handle,
}

#[repr(C)]
struct RawInputHeader {
    kind: u32,
    size: u32,
    device: Handle,
    wparam: usize,
}

#[repr(C)]
struct RawMouse {
    flags: u16,
    buttons: u32,
    raw_buttons: u32,
    x: i32,
    y: i32,
    extra_info: u32,
}

// Only the generic mouse usage is registered, so RAWINPUT's union holds RAWMOUSE.
#[repr(C)]
struct RawMouseInput {
    header: RawInputHeader,
    mouse: RawMouse,
}

// MOUSEINPUT is the largest member of INPUT's union on x64.
#[derive(Clone, Copy)]
#[repr(C)]
struct MouseInputPacket {
    kind: u32,
    mouse: MouseInput,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct MouseInput {
    x: i32,
    y: i32,
    data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
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
struct ShellExecuteInfoW {
    cb_size: u32,
    mask: u32,
    hwnd: Handle,
    verb: *const u16,
    file: *const u16,
    parameters: *const u16,
    directory: *const u16,
    show: i32,
    inst_app: Handle,
    id_list: Handle,
    class: *const u16,
    key_class: Handle,
    hot_key: u32,
    icon_or_monitor: Handle,
    process: Handle,
}

#[repr(C)]
struct TokenElevation {
    token_is_elevated: u32,
}

const _: () = {
    assert!(size_of::<Point>() == 8);
    assert!(align_of::<Point>() == 4);
    assert!(size_of::<Msg>() == 48);
    assert!(size_of::<MouseHookInfo>() == 32);
    assert!(size_of::<KeyboardHookInfo>() == 24);
    assert!(size_of::<WndClassExW>() == 80);
    assert!(size_of::<ShellExecuteInfoW>() == 112);
    assert!(size_of::<RawInputDevice>() == 16);
    assert!(size_of::<RawInputHeader>() == 24);
    assert!(size_of::<RawMouseInput>() == 48);
    assert!(size_of::<MouseInputPacket>() == 40);
    assert!(align_of::<RawMouseInput>() <= align_of::<u64>());
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentThreadId() -> u32;
    fn GetLastError() -> u32;
    fn GetUserDefaultUILanguage() -> u16;
    fn SetPriorityClass(process: Handle, priority_class: u32) -> i32;
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> Handle;
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: u32,
        exe_name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(object: Handle) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(
        process_handle: Handle,
        desired_access: u32,
        token_handle: *mut Handle,
    ) -> i32;
    fn GetTokenInformation(
        token_handle: Handle,
        token_information_class: u32,
        token_information: *mut c_void,
        token_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteExW(exec_info: *mut ShellExecuteInfoW) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassExW(wnd_class: *const WndClassExW) -> u16;
    fn RegisterWindowMessageW(string: *const u16) -> u32;
    fn ChangeWindowMessageFilter(message: u32, flag: u32) -> i32;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
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
    fn DefWindowProcW(hwnd: Handle, message: u32, wparam: usize, lparam: isize) -> isize;
    fn SetWindowsHookExW(id: i32, proc: HookProc, instance: Handle, thread: u32) -> Handle;
    fn UnhookWindowsHookEx(hook: Handle) -> i32;
    fn CallNextHookEx(hook: Handle, code: i32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(message: *mut Msg, hwnd: Handle, first: u32, last: u32) -> i32;
    fn DispatchMessageW(message: *const Msg) -> isize;
    fn PostMessageW(hwnd: Handle, message: u32, wparam: usize, lparam: isize) -> i32;
    fn PostThreadMessageW(thread: u32, message: u32, wparam: usize, lparam: isize) -> i32;
    fn PostQuitMessage(exit_code: i32);
    fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> i32;
    fn GetRawInputData(
        input: Handle,
        command: u32,
        data: *mut c_void,
        size: *mut u32,
        header_size: u32,
    ) -> u32;
    fn SendInput(count: u32, inputs: *const MouseInputPacket, packet_size: i32) -> u32;
    fn SetTimer(hwnd: Handle, id: usize, elapse: u32, func: Handle) -> usize;
    fn KillTimer(hwnd: Handle, id: usize) -> i32;
    fn GetAsyncKeyState(vkey: i32) -> i16;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn WindowFromPoint(point: Point) -> Handle;
    fn GetAncestor(hwnd: Handle, flags: u32) -> Handle;
    fn GetWindowThreadProcessId(hwnd: Handle, process_id: *mut u32) -> u32;
}

fn module_handle() -> io::Result<Handle> {
    let module = unsafe { GetModuleHandleW(null()) };
    if module.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(module)
}

/// Shared single-instance lock file matching Java and .NET W10Wheel (`%TEMP%\W10Wheel.lock`).
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    pub fn acquire(wait: Duration) -> io::Result<Self> {
        Self::acquire_at(&env::temp_dir().join(LOCK_FILE_NAME), wait)
    }

    pub fn acquire_at(path: &Path, wait: Duration) -> io::Result<Self> {
        let deadline = Instant::now().checked_add(wait);
        loop {
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(path)
            {
                Ok(file) => return Ok(Self { _file: file }),
                Err(error) if is_lock_conflict(&error) => {
                    let expired = match deadline {
                        Some(end) => Instant::now() >= end,
                        None => true,
                    };
                    if expired {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "W10Wheel is already running.",
                        ));
                    }
                    thread::sleep(LOCK_RETRY_INTERVAL);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn is_lock_conflict(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(32 | 33))
}

pub fn set_process_priority(priority: ProcessPriority) -> io::Result<()> {
    let class = match priority {
        ProcessPriority::Normal => NORMAL_PRIORITY_CLASS,
        ProcessPriority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        ProcessPriority::High => HIGH_PRIORITY_CLASS,
    };
    if unsafe { SetPriorityClass(GetCurrentProcess(), class) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn system_ui_language() -> UiLanguage {
    let primary = unsafe { GetUserDefaultUILanguage() } & 0x03FF;
    if primary == LANG_JAPANESE {
        UiLanguage::Ja
    } else {
        UiLanguage::En
    }
}

const INPUT_WINDOW_CLASS: [u16; 12] = [
    b'W' as u16,
    b'1' as u16,
    b'0' as u16,
    b'W' as u16,
    b'h' as u16,
    b'e' as u16,
    b'e' as u16,
    b'l' as u16,
    b'_' as u16,
    b'W' as u16,
    b'M' as u16,
    0,
];

static INPUT_WINDOW_CLASS_REG: OnceLock<Result<(), i32>> = OnceLock::new();

unsafe extern "system" fn input_window_proc(
    hwnd: Handle,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if message == WM_ENDSESSION {
        if wparam != 0 {
            crate::handle_session_end();
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn ensure_input_window_class(module: Handle) -> io::Result<()> {
    let reg = INPUT_WINDOW_CLASS_REG.get_or_init(|| {
        let wx = WndClassExW {
            cb_size: size_of::<WndClassExW>() as u32,
            style: 0,
            wnd_proc: input_window_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance: module,
            icon: null_mut(),
            cursor: null_mut(),
            background: null_mut(),
            menu_name: null(),
            class_name: INPUT_WINDOW_CLASS.as_ptr(),
            icon_sm: null_mut(),
        };
        let atom = unsafe { RegisterClassExW(&wx) };
        if atom != 0 {
            Ok(())
        } else {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_CLASS_ALREADY_EXISTS as i32) {
                Ok(())
            } else {
                Err(err.raw_os_error().unwrap_or(0))
            }
        }
    });
    match reg {
        Ok(()) => Ok(()),
        Err(code) => Err(io::Error::from_raw_os_error(*code)),
    }
}

pub(crate) struct InputWindow(Handle);

impl InputWindow {
    pub(crate) fn new() -> io::Result<Self> {
        let module = module_handle()?;
        ensure_input_window_class(module)?;
        // Top-level hidden window (parent = null) receives WM_ENDSESSION on shutdown/logoff.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                INPUT_WINDOW_CLASS.as_ptr(),
                null(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                module,
                null_mut(),
            )
        };
        if hwnd.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(hwnd))
    }

    pub(crate) fn register_mouse(&self) -> bool {
        register_mouse(RIDEV_INPUTSINK, self.0)
    }

    pub(crate) fn unregister_mouse(&self) {
        register_mouse(RIDEV_REMOVE, null_mut());
    }

    pub(crate) fn set_poll_timer(&self, timeout_ms: u32) {
        unsafe { SetTimer(self.0, POLL_WAIT_TIMER, timeout_ms, null_mut()) };
    }

    pub(crate) fn kill_poll_timer(&self) {
        unsafe { KillTimer(self.0, POLL_WAIT_TIMER) };
    }
}

impl Drop for InputWindow {
    fn drop(&mut self) {
        self.kill_poll_timer();
        self.unregister_mouse();
        unsafe { DestroyWindow(self.0) };
    }
}

fn register_mouse(flags: u32, target: Handle) -> bool {
    let device = RawInputDevice {
        usage_page: 1, // Generic Desktop Controls
        usage: 2,      // Mouse
        flags,
        target,
    };
    unsafe { RegisterRawInputDevices(&device, 1, size_of::<RawInputDevice>() as u32) != 0 }
}

fn raw_motion(lparam: isize) -> Option<(i32, i32)> {
    let header_size = size_of::<RawInputHeader>() as u32;
    let min_size = size_of::<RawMouseInput>();
    let mut size = 0u32;
    let query = unsafe {
        GetRawInputData(
            lparam as Handle,
            RID_INPUT,
            null_mut(),
            &mut size,
            header_size,
        )
    };
    if query != 0 || (size as usize) < min_size {
        return None;
    }

    let mut stack_buf = [0u64; 16]; // 128 bytes
    let mut heap_buf = Vec::<u64>::new();
    let ptr = if (size as usize) <= size_of::<[u64; 16]>() {
        stack_buf.as_mut_ptr().cast::<c_void>()
    } else {
        heap_buf.resize((size as usize).div_ceil(size_of::<u64>()), 0);
        heap_buf.as_mut_ptr().cast::<c_void>()
    };

    let mut read_size = size;
    let read = unsafe { GetRawInputData(lparam as Handle, RID_INPUT, ptr, &mut read_size, header_size) };
    if read == u32::MAX || (read as usize) < min_size {
        return None;
    }

    // Win32 wrote at least RawMouseInput bytes into an 8-byte aligned buffer.
    let data = unsafe { &*ptr.cast::<RawMouseInput>() };
    // Any relative move, as Java and .NET take it: also one with no device (hDevice NULL: moves
    // injected by mouse software or drivers). usFlags can have other bits with a relative move
    // (e.g. MOUSE_MOVE_NOCOALESCE). W10Wheel's own wheel events carry no move (0, 0: ignored).
    (data.header.kind == RIM_TYPEMOUSE && data.mouse.flags & MOUSE_MOVE_ABSOLUTE == 0)
        .then_some((data.mouse.x, data.mouse.y))
}

fn xbutton_from_mouse_data(mouse_data: u32) -> Option<MouseButton> {
    match (mouse_data >> 16) as u16 {
        XBUTTON1 => Some(MouseButton::X1),
        XBUTTON2 => Some(MouseButton::X2),
        _ => None,
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: usize, lparam: isize) -> isize {
    if code == HC_ACTION && lparam != 0 {
        // Windows provides this structure for the duration of an HC_ACTION callback.
        let info = unsafe { &*(lparam as *const MouseHookInfo) };
        // A W10Message command: a horizontal wheel of delta 1 (mouseData high word), as Java and
        // .NET send and check it, with the command in dwExtraInfo
        if wparam as u32 == WM_MOUSEHWHEEL
            && (info.flags & MOUSE_INJECTED_FLAGS) != 0
            && (info.mouse_data >> 16) == ipc::W10_MESSAGE_WHEEL_DELTA
            && let Some(command) = ipc::decode_command(info.extra_info)
        {
            crate::handle_ipc_command(command, info.extra_info);
            return 1;
        }
        let kind = match wparam as u32 {
            WM_MOUSEMOVE => Some(HookMouseKind::Move),
            WM_LBUTTONDOWN => Some(HookMouseKind::Down(MouseButton::Left)),
            WM_LBUTTONUP => Some(HookMouseKind::Up(MouseButton::Left)),
            WM_RBUTTONDOWN => Some(HookMouseKind::Down(MouseButton::Right)),
            WM_RBUTTONUP => Some(HookMouseKind::Up(MouseButton::Right)),
            WM_MBUTTONDOWN => Some(HookMouseKind::Down(MouseButton::Middle)),
            WM_MBUTTONUP => Some(HookMouseKind::Up(MouseButton::Middle)),
            WM_XBUTTONDOWN => xbutton_from_mouse_data(info.mouse_data).map(HookMouseKind::Down),
            WM_XBUTTONUP => xbutton_from_mouse_data(info.mouse_data).map(HookMouseKind::Up),
            _ => None,
        };
        if let Some(kind) = kind {
            let event = HookMouseEvent {
                kind,
                point: info.point,
                time: info.time,
                injected: (info.flags & MOUSE_INJECTED_FLAGS) != 0,
                extra_info: info.extra_info,
            };
            if matches!(crate::handle_mouse(event), HookDecision::Suppress) {
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: usize, lparam: isize) -> isize {
    if code == HC_ACTION && lparam != 0 {
        let info = unsafe { &*(lparam as *const KeyboardHookInfo) };
        if (info.flags & KEY_INJECTED_FLAGS) == 0 {
            let action = match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(KeyAction::Down),
                WM_KEYUP | WM_SYSKEYUP => Some(KeyAction::Up),
                _ => None,
            };
            if let Some(action) = action {
                let event = HookKeyEvent {
                    vk_code: info.vk_code,
                    action,
                    time: info.time,
                };
                if matches!(crate::handle_key(event), HookDecision::Suppress) {
                    return 1;
                }
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

struct MouseHook(Handle);

impl MouseHook {
    fn install() -> io::Result<Self> {
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, mouse_hook, module_handle()?, 0) };
        if hook.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(hook))
    }

    /// Unhooks it: false if Windows had removed it already
    fn remove(self) -> bool {
        let removed = unsafe { UnhookWindowsHookEx(self.0) } != 0;
        std::mem::forget(self);
        removed
    }
}

impl Drop for MouseHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

pub(crate) struct KeyboardHook(Handle);

/// What `KeyboardHook::sync` did
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyboardHookSync {
    /// Nothing changed, also when a replacement could not be installed (the old hook stays)
    Unchanged,
    /// Installed where there was none
    Installed,
    /// Replaced by a new one, installed before the old one was removed: no event was missed.
    /// `old_hook_lost`: Windows had removed the old one already (e.g. for a timeout; its
    /// UnhookWindowsHookEx failed), so events may have been missed before.
    Replaced { old_hook_lost: bool },
    /// Removed: no keyboard events are seen any more
    Removed,
    /// Could not be installed where there was none (the Win32 error code)
    Failed(i32),
}

impl KeyboardHook {
    fn install() -> io::Result<Self> {
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, keyboard_hook, module_handle()?, 0) };
        if hook.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(hook))
    }

    /// Unhooks it: false if Windows had removed it already
    fn remove(self) -> bool {
        let removed = unsafe { UnhookWindowsHookEx(self.0) } != 0;
        std::mem::forget(self);
        removed
    }

    /// Installs or removes the hook as `enabled` says. `reinstall`: an installed hook is replaced
    /// by a new one, first in the chain (on every load of the properties, after a resume or an
    /// unlock: also recovers a hook that Windows removed silently). As the mouse hook
    /// (Hooks::reinstall_mouse): the new one is installed before the old one is removed, with no
    /// event handled in between, and the old one stays if that fails.
    pub(crate) fn sync(slot: &mut Option<Self>, enabled: bool, reinstall: bool) -> KeyboardHookSync {
        match (enabled, slot.take()) {
            (true, Some(old)) if reinstall => match Self::install() {
                Ok(hook) => {
                    let old_hook_lost = !old.remove();
                    *slot = Some(hook);
                    KeyboardHookSync::Replaced { old_hook_lost }
                }
                Err(_) => {
                    *slot = Some(old);
                    KeyboardHookSync::Unchanged
                }
            },
            (true, Some(hook)) => {
                *slot = Some(hook);
                KeyboardHookSync::Unchanged
            }
            (true, None) => match Self::install() {
                Ok(hook) => {
                    *slot = Some(hook);
                    KeyboardHookSync::Installed
                }
                Err(err) => KeyboardHookSync::Failed(err.raw_os_error().unwrap_or(0)),
            },
            (false, Some(hook)) => {
                let _ = hook.remove();
                KeyboardHookSync::Removed
            }
            (false, None) => KeyboardHookSync::Unchanged,
        }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.0) };
    }
}

/// The hooks of the hook thread
pub(crate) struct Hooks {
    mouse: MouseHook,
    pub(crate) keyboard: Option<KeyboardHook>,
}

impl Hooks {
    /// Installs the mouse hook again: the newest hook is called first, so this puts it before
    /// the hooks installed after it (e.g. mouse software that installs its hook again after a
    /// resume and then sees the buttons first, and passes on only the ones it injects). The new
    /// hook is installed before the old one is removed (kept if that fails); no event is handled
    /// in between, as the hooks are called on this thread only when it takes its messages.
    /// Ok(true): Windows had removed the old one already (e.g. for a timeout; its
    /// UnhookWindowsHookEx failed), so events may have been missed before.
    pub(crate) fn reinstall_mouse(&mut self) -> io::Result<bool> {
        let old = std::mem::replace(&mut self.mouse, MouseHook::install()?);
        Ok(!old.remove())
    }
}

/// `on_initial_keyboard_hook`: whether the keyboard hook could be installed at the start (Err:
/// the Win32 error code), called before any event is handled; not called if it is not to be
/// installed (`initial_keyboard_hook` false)
pub(crate) fn message_loop(
    initial_keyboard_hook: bool,
    on_initial_keyboard_hook: impl FnOnce(Result<(), i32>),
    on_ready: impl FnOnce(u32),
    on_begin_close: impl FnOnce(),
    mut on_event: impl FnMut(LoopEvent, &mut Hooks),
) -> io::Result<()> {
    let mut hooks = Hooks {
        mouse: MouseHook::install()?,
        keyboard: None,
    };
    if initial_keyboard_hook {
        match KeyboardHook::install() {
            Ok(hook) => {
                hooks.keyboard = Some(hook);
                on_initial_keyboard_hook(Ok(()));
            }
            Err(err) => on_initial_keyboard_hook(Err(err.raw_os_error().unwrap_or(0))),
        }
    }
    on_ready(unsafe { GetCurrentThreadId() });
    let mut loop_result = Ok(());
    loop {
        let mut message = MaybeUninit::<Msg>::uninit();
        match unsafe { GetMessageW(message.as_mut_ptr(), null_mut(), 0, 0) } {
            -1 => {
                loop_result = Err(io::Error::last_os_error());
                break;
            }
            0 => break,
            _ => {}
        }
        let message = unsafe { message.assume_init() };
        match message.message {
            WM_INPUT => {
                let motion = raw_motion(message.lparam);
                // Let Windows clean up the WM_INPUT after reading its handle.
                unsafe {
                    DefWindowProcW(
                        message.hwnd,
                        message.message,
                        message.wparam,
                        message.lparam,
                    );
                }
                if let Some((x, y)) = motion {
                    on_event(
                        LoopEvent::RawMotion {
                            x,
                            y,
                            time: message.time,
                        },
                        &mut hooks,
                    );
                }
            }
            WM_TIMER if message.wparam == POLL_WAIT_TIMER => {
                on_event(LoopEvent::PollTimeout, &mut hooks);
            }
            COMMAND_WAKE_MESSAGE if message.hwnd.is_null() => {
                on_event(LoopEvent::Wake, &mut hooks);
            }
            _ => {
                unsafe { DispatchMessageW(&message) };
            }
        }
    }
    // End pending click sleeps/retries before unhooking to avoid SendInput/hook deadlocks.
    on_begin_close();
    drop(hooks);
    loop_result
}

pub(crate) fn current_thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}

pub(crate) fn post_wake(thread: u32) -> io::Result<()> {
    if unsafe { PostThreadMessageW(thread, COMMAND_WAKE_MESSAGE, 0, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn post_window_message(window: usize, message: u32, wparam: usize, lparam: isize) -> bool {
    unsafe { PostMessageW(window as Handle, message, wparam, lparam) != 0 }
}

pub(crate) fn post_scroll(window: usize, axis: Option<ScrollAxis>) -> bool {
    post_window_message(
        window,
        SCROLL_ACTIVITY_MESSAGE,
        axis.map_or(0, |direction| direction as usize),
        0,
    )
}

pub fn allow_taskbar_created_message() -> bool {
    let name: [u16; 15] = [
        b'T' as u16,
        b'a' as u16,
        b's' as u16,
        b'k' as u16,
        b'b' as u16,
        b'a' as u16,
        b'r' as u16,
        b'C' as u16,
        b'r' as u16,
        b'e' as u16,
        b'a' as u16,
        b't' as u16,
        b'e' as u16,
        b'd' as u16,
        0,
    ];
    let msg = unsafe { RegisterWindowMessageW(name.as_ptr()) };
    msg != 0 && unsafe { ChangeWindowMessageFilter(msg, MSGFLT_ADD) } != 0
}

pub(crate) fn quit() {
    unsafe { PostQuitMessage(0) };
}

pub(crate) fn cursor_pos() -> Point {
    let mut pt = Point::default();
    unsafe { GetCursorPos(&mut pt) };
    pt
}

fn is_vk_down(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) < 0 }
}

/// Whether the key is down now (the state of the keyboard, not of the messages taken so far)
pub(crate) fn is_key_down(vk_code: u32) -> bool {
    i32::try_from(vk_code).is_ok_and(is_vk_down)
}

pub(crate) fn is_escape_down() -> bool {
    is_vk_down(VK_ESCAPE)
}

pub(crate) fn is_modifier_down() -> bool {
    is_vk_down(VK_SHIFT) || is_vk_down(VK_CONTROL) || is_vk_down(VK_MENU)
}

struct KernelHandle(Handle);

impl KernelHandle {
    fn open_process(pid: u32) -> Option<Self> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        (!handle.is_null()).then_some(Self(handle))
    }

    fn open_current_process_token() -> Option<Self> {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0
            && !token.is_null()
        {
            Some(Self(token))
        } else {
            None
        }
    }
}

impl Drop for KernelHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn is_process_elevated() -> bool {
    let Some(token) = KernelHandle::open_current_process_token() else {
        return false;
    };
    let mut elevation = TokenElevation {
        token_is_elevated: 0,
    };
    let mut ret_len = 0u32;
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TOKEN_ELEVATION_CLASS,
            (&raw mut elevation).cast::<c_void>(),
            size_of::<TokenElevation>() as u32,
            &mut ret_len,
        )
    };
    ok != 0 && elevation.token_is_elevated != 0
}

pub enum AdminStartResult {
    Started,
    Canceled,
    Failed(io::Error),
}

fn to_wide_null(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

pub fn restart_as_admin(profile: Option<&str>, open_settings: bool) -> AdminStartResult {
    let exe_path = match env::current_exe() {
        Ok(path) => path,
        Err(err) => return AdminStartResult::Failed(err),
    };
    let verb = to_wide_null("runas".as_ref());
    let file = to_wide_null(exe_path.as_os_str());
    let params = to_wide_null(build_restart_admin_args(profile, open_settings).as_ref());
    let cwd_buf = env::current_dir().ok().map(|p| to_wide_null(p.as_os_str()));
    let directory = cwd_buf.as_ref().map_or(null(), |v| v.as_ptr());

    let mut info = ShellExecuteInfoW {
        cb_size: size_of::<ShellExecuteInfoW>() as u32,
        mask: 0,
        hwnd: null_mut(),
        verb: verb.as_ptr(),
        file: file.as_ptr(),
        parameters: params.as_ptr(),
        directory,
        show: SW_SHOWNORMAL,
        inst_app: null_mut(),
        id_list: null_mut(),
        class: null(),
        key_class: null_mut(),
        hot_key: 0,
        icon_or_monitor: null_mut(),
        process: null_mut(),
    };
    if unsafe { ShellExecuteExW(&raw mut info) } != 0 {
        AdminStartResult::Started
    } else {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(ERROR_CANCELLED as i32) {
            AdminStartResult::Canceled
        } else {
            AdminStartResult::Failed(err)
        }
    }
}

pub(crate) fn send_ipc_hwheel(point: Point, extra_info: usize) -> bool {
    send_input(&[MouseInputPacket::new(
        point,
        MOUSEEVENTF_HWHEEL,
        ipc::W10_MESSAGE_WHEEL_DELTA as i32,
        extra_info,
    )])
}

fn matches_process_id(pid: u32, matcher: &ExclusionMatcher) -> bool {
    let Some(process) = KernelHandle::open_process(pid) else {
        return false;
    };
    let mut stack = [0u16; INITIAL_IMAGE_PATH_LEN];
    let mut size = stack.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.0, 0, stack.as_mut_ptr(), &mut size) } != 0 {
        return matcher.matches_utf16(&stack[..size as usize]);
    }
    let mut cap = INITIAL_IMAGE_PATH_LEN * 2;
    while unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER && cap <= MAX_IMAGE_PATH_LEN {
        let mut heap = vec![0u16; cap];
        let mut heap_size = heap.len() as u32;
        if unsafe { QueryFullProcessImageNameW(process.0, 0, heap.as_mut_ptr(), &mut heap_size) }
            != 0
        {
            return matcher.matches_utf16(&heap[..heap_size as usize]);
        }
        cap *= 2;
    }
    false
}

pub(crate) fn is_point_excluded(point: Point, matcher: &ExclusionMatcher) -> bool {
    if matcher.is_empty() {
        return false;
    }
    let hwnd = unsafe { WindowFromPoint(point) };
    if hwnd.is_null() {
        return false;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != 0 && matches_process_id(pid, matcher) {
        return true;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if !root.is_null() && root != hwnd {
        let mut root_pid = 0u32;
        unsafe { GetWindowThreadProcessId(root, &mut root_pid) };
        if root_pid != 0 && root_pid != pid && matches_process_id(root_pid, matcher) {
            return true;
        }
    }
    false
}

impl MouseInputPacket {
    fn new(point: Point, flags: u32, data: i32, extra_info: usize) -> Self {
        Self {
            kind: INPUT_MOUSE,
            mouse: MouseInput {
                x: point.x,
                y: point.y,
                data: data as u32,
                flags,
                time: 0,
                extra_info,
            },
        }
    }
}

fn send_input(inputs: &[MouseInputPacket]) -> bool {
    let count = inputs.len() as u32;
    unsafe { SendInput(count, inputs.as_ptr(), size_of::<MouseInputPacket>() as i32) == count }
}

pub(crate) struct Win32SendSink;

impl SendSink for Win32SendSink {
    fn send(&mut self, packet: SendPacket) -> bool {
        let input = match packet {
            SendPacket::Wheel { axis, delta } => {
                let flag = match axis {
                    ScrollAxis::Vertical => MOUSEEVENTF_WHEEL,
                    ScrollAxis::Horizontal => MOUSEEVENTF_HWHEEL,
                };
                MouseInputPacket::new(Point::default(), flag, delta, 0)
            }
            SendPacket::ButtonDown {
                button,
                point,
                extra_info,
            } => {
                let (flags, data) = match button {
                    MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, 0),
                    MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, 0),
                    MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, 0),
                    MouseButton::X1 => (MOUSEEVENTF_XDOWN, XBUTTON1 as i32),
                    MouseButton::X2 => (MOUSEEVENTF_XDOWN, XBUTTON2 as i32),
                };
                MouseInputPacket::new(point, flags, data, extra_info)
            }
            SendPacket::ButtonUp {
                button,
                point,
                extra_info,
            } => {
                let (flags, data) = match button {
                    MouseButton::Left => (MOUSEEVENTF_LEFTUP, 0),
                    MouseButton::Right => (MOUSEEVENTF_RIGHTUP, 0),
                    MouseButton::Middle => (MOUSEEVENTF_MIDDLEUP, 0),
                    MouseButton::X1 => (MOUSEEVENTF_XUP, XBUTTON1 as i32),
                    MouseButton::X2 => (MOUSEEVENTF_XUP, XBUTTON2 as i32),
                };
                MouseInputPacket::new(point, flags, data, extra_info)
            }
        };
        send_input(&[input])
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    static LOCK_TEST_SEQ: AtomicU32 = AtomicU32::new(0);

    fn temp_lock_path() -> io::Result<(PathBuf, PathBuf)> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = LOCK_TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!(
            "w10wheel-lock-test-{}-{ts}-{seq}",
            std::process::id()
        ));
        fs::create_dir(&dir)?;
        let path = dir.join(LOCK_FILE_NAME);
        Ok((dir, path))
    }

    #[test]
    fn instance_lock_excludes_second_owner_until_dropped() -> io::Result<()> {
        let (dir, path) = temp_lock_path()?;
        let guard = InstanceLock::acquire_at(&path, Duration::ZERO)?;
        let err = InstanceLock::acquire_at(&path, Duration::ZERO)
            .err()
            .expect("second lock should fail");
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        drop(guard);

        let reacquired = InstanceLock::acquire_at(&path, Duration::ZERO)?;
        drop(reacquired);
        fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn instance_lock_waits_for_release_within_timeout() -> io::Result<()> {
        let (dir, path) = temp_lock_path()?;
        let guard = InstanceLock::acquire_at(&path, Duration::ZERO)?;
        let releaser = thread::spawn(move || {
            thread::sleep(Duration::from_millis(120));
            drop(guard);
        });

        let reacquired = InstanceLock::acquire_at(&path, Duration::from_secs(2))?;
        drop(reacquired);
        releaser.join().unwrap();
        fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn process_exclusion_lookup_matches_current_exe_and_skips_empty() -> io::Result<()> {
        let exe = env::current_exe()?;
        let exe_str = exe.to_str().expect("current_exe should be valid UTF-8");
        let list = crate::exclusions::ExclusionList::from_lines([exe_str]);
        let matcher = list.to_matcher();

        assert!(matches_process_id(std::process::id(), &matcher));
        assert!(!matches_process_id(
            std::process::id(),
            &crate::exclusions::ExclusionList::from_lines(["C:\\NonExistent\\missing.exe"])
                .to_matcher()
        ));
        assert!(!is_point_excluded(
            Point { x: 0, y: 0 },
            &ExclusionMatcher::default()
        ));
        Ok(())
    }

    #[test]
    fn elevation_uipi_and_top_level_input_window_work() -> io::Result<()> {
        let _ = is_process_elevated();
        assert!(allow_taskbar_created_message());
        let first = InputWindow::new()?;
        let second = InputWindow::new()?;
        drop(first);
        drop(second);
        Ok(())
    }
}
