use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;
use std::process::ExitCode;
use std::ptr::null_mut;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use w10wheel::cli::{CliAction, IpcCommand, LaunchOptions, parse_args};
use w10wheel::config::UiLanguage;
use w10wheel::locale::{format_tray_tooltip, tr};
use w10wheel::properties::{ConfigStore, DEFAULT_PROFILE};
use w10wheel::{
    AdminStartResult, EngineConfig, EngineControl, IPC_COMMAND_MESSAGE, InputEngine,
    KEYBOARD_HOOK_FAILED_MESSAGE, KEYBOARD_HOOK_FAILED_ON_LOAD,
    InstanceLock, SCROLL_ACTIVITY_MESSAGE, ScrollAxis, allow_taskbar_created_message,
    is_process_elevated, restart_as_admin, set_process_priority, system_ui_language,
};
use winsafe::{self as w, co, msg};

use crate::cursor::{RESTORE_TIMER, ScrollCursor};
use crate::dialogs::{ask_restart_as_admin, show_about, show_error_dialog};
use crate::icons::{IconHandle, create_app_icon, create_tray_icon, system_icon_size};
use crate::settings_ui::{
    ADMIN_RESTART_MESSAGE, ADMIN_RESTART_STARTED, AppShared, SettingsWindow, TRAY_SYNC_MESSAGE,
};
use crate::theme::{
    THEME_CHANGED_MESSAGE, ThemeWatcher, flush_menu_themes, init_uxtheme_dark_mode,
    is_windows_dark,
};
use crate::ui_win32::{
    Handle, WS_OVERLAPPED, WM_CREATE, WM_DESTROY, WM_ENDSESSION, WM_NCDESTROY, WM_TIMER,
    ModalGuard, create_window, def_window_proc, destroy_window, dpi_for_window, end_modals,
    init_ui_process, is_modal_active, is_window, kill_timer, modals_ending, register_window_class,
    run_main_message_loop, set_timer, set_user_data, user_data,
};

// Retries an exit received while a nested message loop ran (see shutdown_host)
const EXIT_RETRY_TIMER: usize = 3;
const EXIT_RETRY_MS: u32 = 50;

// The hooks are installed again after a resume or an unlock (see schedule_reinstall_hooks)
const REINSTALL_HOOKS_TIMER: usize = 4;
const REINSTALL_HOOKS_DELAYS_MS: [u32; 2] = [2_000, 8_000];

const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMRESUMESUSPEND: usize = 0x0007;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WTS_SESSION_UNLOCK: usize = 0x8;
const NOTIFY_FOR_THIS_SESSION: u32 = 0;

const TRAY_CLASS: &str = "W10WheelTrayHost";
const ICON_ID: u32 = 1;
const NOTIFYICON_VERSION_4: u32 = 4;
const WM_TRAY_CALLBACK: u32 = 0x8001; // WM_APP + 1

const WM_LBUTTONDBLCLK: u16 = 0x0203;
const WM_CONTEXTMENU: u16 = 0x007B;
const TRAY_KEY_ACTIVATE: u16 = 0x0401; // NIN_KEYSELECT

const OPEN_SETTINGS: u16 = 100;
const TOGGLE_PAUSE: u16 = 101;
const ABOUT: u16 = 102;
const EXIT: u16 = 103;

const MF_BYCOMMAND: u32 = 0x0000_0000;
const MF_CHECKED: u32 = 0x0000_0008;
const IDI_APPLICATION: usize = 32512;

const ADMIN_RESTART_LOCK_WAIT: Duration = Duration::from_secs(30);

#[repr(C)]
struct NotifyIconIdentifier {
    size: u32,
    window: *mut c_void,
    id: u32,
    guid: w::GUID,
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn Shell_NotifyIconGetRect(
        identifier: *const NotifyIconIdentifier,
        location: *mut w::RECT,
    ) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn IsWindowVisible(hwnd: Handle) -> i32;
    fn SetMenuDefaultItem(hmenu: *mut c_void, item: u32, by_pos: u32) -> i32;
    fn CheckMenuItem(hmenu: *mut c_void, id_check_item: u32, check: u32) -> u32;
    fn LoadIconW(hinstance: Handle, name: usize) -> Handle;
}

#[link(name = "wtsapi32")]
unsafe extern "system" {
    fn WTSRegisterSessionNotification(hwnd: Handle, flags: u32) -> i32;
    fn WTSUnRegisterSessionNotification(hwnd: Handle) -> i32;
}

struct TrayHostState {
    shared: Rc<RefCell<AppShared>>,
    control: EngineControl,
    cursor: ScrollCursor,
    settings: Option<SettingsWindow>,
    theme_watcher: Option<ThemeWatcher>,
    tray_icon: Option<IconHandle>,
    taskbar_created_msg: u32,
    paused: bool,
    shutting_down: bool,
    // An exit waits for the nested message loops to return (see shutdown_host)
    exit_deferred: bool,
    // The next of REINSTALL_HOOKS_DELAYS_MS after the one running
    reinstall_hooks_step: usize,
}

pub(crate) fn main() -> ExitCode {
    init_ui_process();
    init_uxtheme_dark_mode();
    let _ = allow_taskbar_created_message();

    match parse_args(std::env::args().skip(1)) {
        CliAction::Run(options) => run(options),
        CliAction::Command(command) => {
            if w10wheel::ipc::send_command(&command) {
                std::thread::sleep(Duration::from_millis(1000));
                ExitCode::SUCCESS
            } else {
                let lang = detect_cli_language(None);
                show_error_dialog(
                    null_mut(),
                    lang,
                    is_windows_dark(),
                    tr(lang, "Command Error"),
                    tr(lang, "Failed to send IPC command."),
                    "",
                );
                ExitCode::FAILURE
            }
        }
        CliAction::UnknownCommand(name) => {
            let lang = detect_cli_language(None);
            show_error_dialog(
                null_mut(),
                lang,
                is_windows_dark(),
                tr(lang, "Command Error"),
                tr(lang, "Unknown Command"),
                &name,
            );
            ExitCode::FAILURE
        }
        CliAction::InvalidBoolArg { command, value } => {
            let lang = detect_cli_language(None);
            show_error_dialog(
                null_mut(),
                lang,
                is_windows_dark(),
                tr(lang, "Command Error"),
                tr(lang, "Invalid Command Argument"),
                &format!("{command}: {value}"),
            );
            ExitCode::FAILURE
        }
    }
}

fn detect_cli_language(profile: Option<&str>) -> UiLanguage {
    let sys_lang = system_ui_language();
    let Ok(mut store) = ConfigStore::open_user(sys_lang) else {
        return sys_lang;
    };
    let _ = store.load_startup(profile);
    store.config().ui_language
}

fn run(options: LaunchOptions) -> ExitCode {
    let wait = if options.wait_for_lock {
        ADMIN_RESTART_LOCK_WAIT
    } else {
        Duration::ZERO
    };
    let instance_lock = match InstanceLock::acquire(wait) {
        Ok(lock) => lock,
        Err(err) => {
            // As .NET (Program.messageDoubleLaunch): the message alone, in the language of the
            // properties, captioned Error. The English text of the lock error is not added under
            // it; an unexpected error (not the lock held) has its Windows message (in the language
            // of Windows) as the detail.
            let lang = detect_cli_language(options.profile.as_deref());
            let detail = if err.kind() == std::io::ErrorKind::AlreadyExists {
                String::new()
            } else {
                err.to_string()
            };
            show_error_dialog(
                null_mut(),
                lang,
                is_windows_dark(),
                tr(lang, "Error"),
                tr(lang, "W10Wheel is already running. This instance will exit."),
                &detail,
            );
            return ExitCode::FAILURE;
        }
    };

    let system_lang = system_ui_language();
    let mut store = ConfigStore::open_user(system_lang)
        .unwrap_or_else(|error| ConfigStore::session_only(system_lang, error.to_string()));
    let report = store.load_startup(options.profile.as_deref());
    let lang = store.config().ui_language;
    let dark = is_windows_dark();

    if let Some(missing) = report.missing_profile {
        show_error_dialog(
            null_mut(),
            lang,
            dark,
            "W10Wheel",
            &format!("{}: {missing}", tr(lang, "Properties does not exist")),
            "",
        );
    }

    if !is_process_elevated() && store.config().admin_check {
        let (restart, dont_ask) = ask_restart_as_admin(lang, dark);
        if dont_ask {
            store.config_mut().admin_check = false;
            let _ = store.store();
        }
        if restart {
            let selected = store.selected_profile().to_owned();
            let profile_arg = (selected != DEFAULT_PROFILE).then_some(selected.as_str());
            match restart_as_admin(profile_arg, false) {
                AdminStartResult::Started => {
                    drop(instance_lock);
                    return ExitCode::SUCCESS;
                }
                AdminStartResult::Canceled => {}
                AdminStartResult::Failed(err) => {
                    show_error_dialog(
                        null_mut(),
                        lang,
                        dark,
                        "W10Wheel",
                        tr(lang, "Failed to restart as administrator"),
                        &err.to_string(),
                    );
                }
            }
        }
    }

    let _ = set_process_priority(store.config().process_priority);

    let initial_config = EngineConfig::from_store(&store);
    let engine = match InputEngine::start(initial_config) {
        Ok(eng) => eng,
        Err(err) => {
            let detail = err.to_string();
            let heading = if detail.contains("mouse hook") {
                tr(lang, "Failed mouse hook install")
            } else if detail.contains("keyboard hook") {
                tr(lang, "Failed keyboard hook install")
            } else {
                "W10Wheel"
            };
            show_error_dialog(null_mut(), lang, dark, "W10Wheel", heading, &detail);
            drop(instance_lock);
            return ExitCode::FAILURE;
        }
    };

    let shared = Rc::new(RefCell::new(AppShared {
        store,
        control: engine.control(),
        paused: false,
        dark,
        tray_host: null_mut(),
        restarting_admin: false,
        admin_error: Arc::new(Mutex::new(None)),
    }));

    let tray_class_icon = create_app_icon(system_icon_size(96, false));
    let raw_class_icon = tray_class_icon
        .as_ref()
        .map_or(null_mut(), |icon| icon.raw());
    let _ = register_window_class(TRAY_CLASS, tray_wnd_proc, 32512, raw_class_icon);

    let taskbar_created_msg = w::RegisterWindowMessage("TaskbarCreated").unwrap_or(0);
    let tray_host = create_window(
        0,
        TRAY_CLASS,
        "W10Wheel",
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        null_mut(),
        0,
        null_mut(),
    );
    if tray_host.is_null() {
        let _ = engine.shutdown();
        drop(instance_lock);
        return ExitCode::FAILURE;
    }

    shared.borrow_mut().tray_host = tray_host;

    let settings = match SettingsWindow::create(Rc::clone(&shared)) {
        Some(win) => win,
        None => {
            destroy_window(tray_host);
            let _ = engine.shutdown();
            drop(instance_lock);
            return ExitCode::FAILURE;
        }
    };
    let settings_hwnd = settings.hwnd();

    let mut host_state = Box::new(TrayHostState {
        shared: Rc::clone(&shared),
        control: engine.control(),
        cursor: ScrollCursor::default(),
        settings: Some(settings),
        theme_watcher: ThemeWatcher::start(tray_host),
        tray_icon: None,
        taskbar_created_msg,
        paused: false,
        shutting_down: false,
        exit_deferred: false,
        reinstall_hooks_step: 0,
    });

    if let Err(err) = sync_tray(tray_host, &mut host_state) {
        let _ = host_state.theme_watcher.take();
        if let Some(mut s) = host_state.settings.take() {
            s.destroy();
        }
        destroy_window(tray_host);
        let _ = engine.shutdown();
        drop(instance_lock);
        show_error_dialog(
            null_mut(),
            lang,
            dark,
            "W10Wheel",
            tr(lang, "Failed to create system tray icon"),
            &err.to_string(),
        );
        return ExitCode::FAILURE;
    }

    let _ = engine.control().watch_scroll(tray_host as usize);
    // For WTS_SESSION_UNLOCK. Without it (fails), only a resume installs the hooks again.
    unsafe { WTSRegisterSessionNotification(tray_host, NOTIFY_FOR_THIS_SESSION) };

    if options.open_settings
        && let Some(ref mut s) = host_state.settings
    {
        s.show();
    }

    set_user_data(tray_host, Box::into_raw(host_state));

    let _exit_code = run_main_message_loop(|| {
        if !settings_hwnd.is_null()
            && is_window(settings_hwnd)
            && unsafe { IsWindowVisible(settings_hwnd) != 0 }
        {
            settings_hwnd
        } else {
            null_mut()
        }
    });

    let _ = engine.control().watch_scroll(0);
    let _ = shared.borrow_mut().store.store();
    let _ = engine.shutdown();
    drop(instance_lock);
    ExitCode::SUCCESS
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: Handle,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if msg == WM_CREATE {
        return 0;
    }

    let ptr = unsafe { user_data::<TrayHostState>(hwnd) };
    if ptr.is_null() {
        return def_window_proc(hwnd, msg, wparam, lparam);
    }
    let state = unsafe { &mut *ptr };

    match msg {
        m if m == WM_TRAY_CALLBACK => {
            handle_tray_callback(hwnd, state, lparam);
            return 0;
        }
        m if state.taskbar_created_msg != 0 && m == state.taskbar_created_msg => {
            let _ = sync_tray(hwnd, state);
            return 0;
        }
        SCROLL_ACTIVITY_MESSAGE => {
            let axis = if state.paused {
                None
            } else {
                ScrollAxis::from_wparam(wparam)
            };
            let hw = unsafe { w::HWND::from_ptr(hwnd) };
            state.cursor.set_axis(&hw, axis);
            return 0;
        }
        WM_TIMER => {
            if wparam == EXIT_RETRY_TIMER {
                if !is_modal_active() {
                    kill_timer(hwnd, EXIT_RETRY_TIMER);
                    shutdown_host(hwnd, state);
                }
                return 0;
            }
            if wparam == RESTORE_TIMER {
                let hw = unsafe { w::HWND::from_ptr(hwnd) };
                state.cursor.retry_restore(&hw);
                return 0;
            }
            if wparam == REINSTALL_HOOKS_TIMER {
                reinstall_hooks_on_timer(hwnd, state);
                return 0;
            }
        }
        WM_POWERBROADCAST
            if wparam == PBT_APMRESUMEAUTOMATIC || wparam == PBT_APMRESUMESUSPEND =>
        {
            schedule_reinstall_hooks(hwnd, state);
        }
        WM_WTSSESSION_CHANGE if wparam == WTS_SESSION_UNLOCK => {
            schedule_reinstall_hooks(hwnd, state);
        }
        IPC_COMMAND_MESSAGE => {
            handle_ipc_command(hwnd, state, wparam);
            return 0;
        }
        KEYBOARD_HOOK_FAILED_MESSAGE => {
            handle_keyboard_hook_failed(state, wparam, lparam);
            return 0;
        }
        THEME_CHANGED_MESSAGE => {
            let dark = is_windows_dark();
            flush_menu_themes();
            if let Some(ref mut settings) = state.settings {
                settings.apply_theme(dark);
            }
            let _ = sync_tray(hwnd, state);
            return 0;
        }
        ADMIN_RESTART_MESSAGE => {
            if wparam == ADMIN_RESTART_STARTED {
                shutdown_host(hwnd, state);
            } else if let Some(ref mut settings) = state.settings {
                settings.on_admin_restart_result(wparam);
            }
            return 0;
        }
        TRAY_SYNC_MESSAGE => {
            let _ = sync_tray(hwnd, state);
            return 0;
        }
        WM_ENDSESSION => {
            if wparam != 0 {
                shutdown_host(hwnd, state);
            }
            return 0;
        }
        WM_DESTROY => {
            cleanup_tray_resources(hwnd, state);
            w::PostQuitMessage(0);
            return 0;
        }
        WM_NCDESTROY => {
            set_user_data::<TrayHostState>(hwnd, null_mut());
            let boxed = unsafe { Box::from_raw(ptr) };
            drop(boxed);
            return 0;
        }
        _ => {}
    }

    def_window_proc(hwnd, msg, wparam, lparam)
}

fn handle_tray_callback(hwnd: Handle, state: &mut TrayHostState, lparam: isize) {
    let notification = lparam as u32;
    if notification >> 16 != ICON_ID {
        return;
    }

    match notification as u16 {
        WM_LBUTTONDBLCLK => {
            set_paused(hwnd, state, !state.paused);
        }
        TRAY_KEY_ACTIVATE => {
            if let Some(ref mut settings) = state.settings {
                settings.show();
            }
        }
        WM_CONTEXTMENU => {
            let (lang, dark) = {
                let s = state.shared.borrow();
                (s.store.config().ui_language, s.dark)
            };
            match tray_menu(hwnd, lang, state.paused) {
                Ok(Some(OPEN_SETTINGS)) => {
                    if let Some(ref mut settings) = state.settings {
                        settings.show();
                    }
                }
                Ok(Some(TOGGLE_PAUSE)) => {
                    set_paused(hwnd, state, !state.paused);
                }
                Ok(Some(ABOUT)) => {
                    let owner = state
                        .settings
                        .as_ref()
                        .filter(|s| s.is_visible())
                        .map_or(null_mut(), |s| s.hwnd());
                    show_about(owner, lang, dark);
                }
                Ok(Some(EXIT)) => {
                    shutdown_host(hwnd, state);
                }
                _ => {
                    let hw = unsafe { w::HWND::from_ptr(hwnd) };
                    let _ = w::Shell_NotifyIcon(co::NIM::SETFOCUS, &tray_data(&hw));
                }
            }
        }
        _ => {}
    }
}

fn handle_ipc_command(hwnd: Handle, state: &mut TrayHostState, wparam: usize) {
    let Some(cmd) = w10wheel::ipc::decode_command(wparam) else {
        return;
    };
    match cmd {
        IpcCommand::SendExit => shutdown_host(hwnd, state),
        IpcCommand::SendPassMode(next) => set_paused(hwnd, state, next),
        IpcCommand::SendReloadProp => {
            if let Some(ref mut settings) = state.settings {
                settings.on_reload_ipc();
            }
            let _ = sync_tray(hwnd, state);
        }
        IpcCommand::SendInitState => {
            let _ = state.control.init_state();
            let hw = unsafe { w::HWND::from_ptr(hwnd) };
            state.cursor.set_axis(&hw, None);
        }
    }
}

/// The keyboard hook could not be installed (at startup, at a load, or turned on in the dialog): the
/// keyboard trigger is turned off and the error shown, as .NET does (Ctx.setKeyboardHookOf).
fn handle_keyboard_hook_failed(state: &mut TrayHostState, wparam: usize, lparam: isize) {
    let on_load = lparam == KEYBOARD_HOOK_FAILED_ON_LOAD;
    let (lang, dark) = {
        let mut s = state.shared.borrow_mut();
        // Reported again before the setting reached the hook thread: told once
        if !s.store.config().keyboard_hook {
            return;
        }
        s.store.turn_off_keyboard_hook(on_load);
        let next = EngineConfig::from_store(&s.store);
        let _ = s.control.set_config(next);
        if !on_load {
            let _ = s.store.store();
        }
        (s.store.config().ui_language, s.dark)
    };
    if let Some(ref mut settings) = state.settings {
        settings.refresh();
    }
    let owner = state
        .settings
        .as_ref()
        .filter(|s| s.is_visible())
        .map_or(null_mut(), |s| s.hwnd());
    let detail = std::io::Error::from_raw_os_error(wparam as u32 as i32).to_string();
    show_error_dialog(
        owner,
        lang,
        dark,
        "W10Wheel",
        tr(lang, "Failed keyboard hook install"),
        &detail,
    );
}

fn set_paused(hwnd: Handle, state: &mut TrayHostState, paused: bool) {
    let _ = state.control.set_paused(paused);
    state.paused = paused;
    state.shared.borrow_mut().paused = paused;
    let hw = unsafe { w::HWND::from_ptr(hwnd) };
    state.cursor.set_axis(&hw, None);
    let _ = sync_tray(hwnd, state);
    if let Some(ref mut settings) = state.settings {
        settings.refresh();
    }
}

/// After a resume or an unlock, the hooks are installed again, so that they are first in the
/// chain again (the newest hook is called first). Mouse software can install its hook again then
/// (e.g. X-Mouse Button Control). It is then called first, blocks the buttons it remaps and
/// injects others, and W10Wheel ignores injected input: no mouse trigger works any more. The
/// hooks are installed some time after the event, after the other software's, and once more later
/// in case that software was late. Pausing and resuming does it at once (see HookApp::set_paused).
fn schedule_reinstall_hooks(hwnd: Handle, state: &mut TrayHostState) {
    state.reinstall_hooks_step = 1;
    set_timer(hwnd, REINSTALL_HOOKS_TIMER, REINSTALL_HOOKS_DELAYS_MS[0]);
}

fn reinstall_hooks_on_timer(hwnd: Handle, state: &mut TrayHostState) {
    let _ = state.control.reinstall_hooks();
    match REINSTALL_HOOKS_DELAYS_MS.get(state.reinstall_hooks_step) {
        Some(&delay) => {
            state.reinstall_hooks_step += 1;
            set_timer(hwnd, REINSTALL_HOOKS_TIMER, delay);
        }
        None => kill_timer(hwnd, REINSTALL_HOOKS_TIMER),
    }
}

fn cleanup_tray_resources(hwnd: Handle, state: &mut TrayHostState) {
    kill_timer(hwnd, RESTORE_TIMER);
    kill_timer(hwnd, REINSTALL_HOOKS_TIMER);
    unsafe { WTSUnRegisterSessionNotification(hwnd) };
    let _ = state.control.watch_scroll(0);
    let hw = unsafe { w::HWND::from_ptr(hwnd) };
    let _ = state.cursor.restore();
    delete_tray_icon(&hw);
    state.tray_icon = None;
    state.theme_watcher = None;
}

fn shutdown_host(hwnd: Handle, state: &mut TrayHostState) {
    if state.shutting_down {
        return;
    }
    // Received in a nested message loop (a dialog of the settings, the file dialog, the tray menu,
    // About): the window procedure that started it uses its state when it returns, so the settings
    // window (and its state) cannot be destroyed now. The loops are ended, and the exit retried
    // once they have all returned (EXIT_RETRY_TIMER). The settings are stored and the cursor
    // restored at once: at the end of the session (WM_ENDSESSION) the process may end before.
    if is_modal_active() {
        if !state.exit_deferred {
            state.exit_deferred = true;
            end_modals();
            if let Ok(mut shared) = state.shared.try_borrow_mut() {
                let _ = shared.store.store();
            }
            let _ = state.cursor.restore();
            set_timer(hwnd, EXIT_RETRY_TIMER, EXIT_RETRY_MS);
        }
        return;
    }
    kill_timer(hwnd, EXIT_RETRY_TIMER);
    state.shutting_down = true;
    cleanup_tray_resources(hwnd, state);
    if let Some(ref mut settings) = state.settings {
        settings.commit_pending();
        let _ = state.shared.borrow_mut().store.store();
        settings.destroy();
    }
    state.settings = None;
    destroy_window(hwnd);
}

fn tray_data(hwnd: &w::HWND) -> w::NOTIFYICONDATA {
    let mut data = w::NOTIFYICONDATA::default();
    data.hWnd = unsafe { hwnd.raw_copy() };
    data.uID = ICON_ID;
    data
}

fn delete_tray_icon(hwnd: &w::HWND) {
    let _ = w::Shell_NotifyIcon(co::NIM::DELETE, &tray_data(hwnd));
}

fn sync_tray(hwnd: Handle, state: &mut TrayHostState) -> w::AnyResult<()> {
    let (lang, profile) = {
        let s = state.shared.borrow();
        (s.store.config().ui_language, s.store.selected_profile().to_owned())
    };
    let tip = format_tray_tooltip(lang, state.paused, &profile);

    let dpi = dpi_for_window(hwnd);
    let size = system_icon_size(dpi, true);
    let next_icon = create_tray_icon(size, state.paused);
    let fallback_raw = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };

    let hw = unsafe { w::HWND::from_ptr(hwnd) };
    let mut data = tray_data(&hw);
    data.uFlags = co::NIF::MESSAGE | co::NIF::ICON | co::NIF::TIP | co::NIF::SHOWTIP;
    data.uCallbackMessage = unsafe { co::WM::from_raw(WM_TRAY_CALLBACK) };
    data.uVersion = NOTIFYICON_VERSION_4;
    data.hIcon = unsafe {
        w::HICON::from_ptr(
            next_icon
                .as_ref()
                .map_or(fallback_raw, |icon| icon.raw()),
        )
    };
    data.set_szTip(&tip);

    let prev_icon = state.tray_icon.take();
    state.tray_icon = next_icon;

    if w::Shell_NotifyIcon(co::NIM::MODIFY, &data).is_err() {
        w::Shell_NotifyIcon(co::NIM::ADD, &data)?;
        if let Err(error) = w::Shell_NotifyIcon(co::NIM::SETVERSION, &data) {
            delete_tray_icon(&hw);
            drop(prev_icon);
            return Err(error.into());
        }
    }

    drop(prev_icon);
    Ok(())
}

fn tray_anchor(hwnd: &w::HWND) -> w::SysResult<w::POINT> {
    let identifier = NotifyIconIdentifier {
        size: size_of::<NotifyIconIdentifier>() as u32,
        window: hwnd.ptr(),
        id: ICON_ID,
        guid: w::GUID::default(),
    };
    let mut bounds = w::RECT::default();
    if unsafe { Shell_NotifyIconGetRect(&identifier, &mut bounds) } == 0 {
        Ok(w::POINT::with(bounds.right, bounds.top))
    } else {
        w::GetCursorPos()
    }
}

fn tray_menu(hwnd: Handle, lang: UiLanguage, paused: bool) -> w::AnyResult<Option<u16>> {
    let hw = unsafe { w::HWND::from_ptr(hwnd) };
    let menu = unsafe { w::guard::DestroyMenuGuard::new(w::HMENU::CreatePopupMenu()?) };
    menu.append_item(&[
        w::MenuItem::Entry {
            cmd_id: OPEN_SETTINGS,
            text: tr(lang, "Settings..."),
        },
        w::MenuItem::Entry {
            cmd_id: TOGGLE_PAUSE,
            text: tr(lang, "Pause"),
        },
        w::MenuItem::Separator,
        w::MenuItem::Entry {
            cmd_id: ABOUT,
            text: tr(lang, "About"),
        },
        w::MenuItem::Entry {
            cmd_id: EXIT,
            text: tr(lang, "Exit"),
        },
    ])?;

    unsafe {
        SetMenuDefaultItem(menu.ptr(), OPEN_SETTINGS as u32, 0);
        if paused {
            CheckMenuItem(menu.ptr(), TOGGLE_PAUSE as u32, MF_BYCOMMAND | MF_CHECKED);
        }
    }

    let anchor = tray_anchor(&hw)?;
    // The menu runs its own message loop (see is_modal_active): an exit meanwhile closes it, and
    // nothing chosen is done then
    let _guard = ModalGuard::enter();
    if modals_ending() {
        return Ok(None);
    }
    hw.SetForegroundWindow();
    w::SetLastError(co::ERROR::SUCCESS);
    let choice = menu.TrackPopupMenu(
        co::TPM::RIGHTALIGN | co::TPM::BOTTOMALIGN | co::TPM::RETURNCMD | co::TPM::NONOTIFY,
        anchor,
        &hw,
    );
    unsafe { hw.PostMessage(msg::WmNull {})? };
    if modals_ending() {
        return Ok(None);
    }
    Ok(choice?.map(|command| command as u16))
}
