use std::cell::Cell;
use std::path::Path;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use w10wheel::config::UiLanguage;
use w10wheel::is_process_elevated;
use w10wheel::locale::{profile_name_error_key, tr};
use w10wheel::properties::validate_new_profile_name;

use crate::icons::{IconHandle, StockIcon, create_app_icon, create_stock_icon, system_icon_size};
use crate::theme::{
    ControlTheme, ThemePalette, apply_control_theme, apply_window_dark_mode,
    handle_dark_button_custom_draw,
};
use crate::ui_win32::{
    BM_GETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, BST_CHECKED,
    CW_USEDEFAULT, EN_CHANGE, ES_AUTOHSCROLL, Handle, SS_ICON, SS_LEFT, SS_NOTIFY,
    SS_REALSIZECONTROL, STM_SETICON, TooltipManager, UiFontKind, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_CLIPCHILDREN, WS_DISABLED, WS_EX_CONTROLPARENT, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX,
    WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_NCDESTROY, WM_NOTIFY, WM_SETCURSOR, adjust_window_size,
    center_window, create_ui_font, create_window, def_window_proc, dialog_ids, dpi_for_window,
    enable_window, get_window_text, open_shell_target, register_window_class,
    run_modal_loop, scale, send_msg, set_button_shield, set_focus, set_font, set_foreground,
    set_hand_cursor_now, set_user_data, set_window_icon, set_window_text, user_data,
};
use crate::ui_win32::{measure_text_height, measure_text_width, move_window};

pub(crate) const PROJECT_URL: &str = "https://github.com/ykon/w10wheel.net";
pub(crate) const WIKI_URL: &str = "https://github.com/ykon/w10wheel.net/wiki";
// The version of Cargo.toml (as the version information of the exe)
const PROGRAM_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (Rust)");
const MODAL_CLASS: &str = "W10WheelModalDialog";

const IDC_ARROW: usize = 32512;

static ABOUT_OPEN: AtomicBool = AtomicBool::new(false);
static ABOUT_HWND: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
enum DialogKind {
    AdminPrompt,
    About,
    ProfileName,
    Message,
}

struct ModalState {
    kind: DialogKind,
    dpi: u32,
    palette: ThemePalette,
    done: Cell<bool>,
    link_controls: Vec<Handle>,
    error_controls: Vec<Handle>,
    muted_controls: Vec<Handle>,
    _fonts: Vec<crate::ui_win32::FontHandle>,
    _icons: Vec<IconHandle>,
    _tips: Option<TooltipManager>,

    // Admin prompt
    dont_ask_chk: Handle,
    admin_result: (bool, bool),

    // Profile input
    profile_dir: Option<std::path::PathBuf>,
    profile_edit: Handle,
    profile_error_lbl: Handle,
    profile_ok_btn: Handle,
    lang: UiLanguage,
    profile_result: Option<String>,

    // Confirm / Message
    confirmed: bool,
}

impl ModalState {
    fn new(kind: DialogKind, dark: bool, lang: UiLanguage) -> Self {
        Self {
            kind,
            dpi: 96,
            palette: ThemePalette::new(dark),
            done: Cell::new(false),
            link_controls: Vec::new(),
            error_controls: Vec::new(),
            muted_controls: Vec::new(),
            _fonts: Vec::new(),
            _icons: Vec::new(),
            _tips: None,
            dont_ask_chk: null_mut(),
            admin_result: (false, false),
            profile_dir: None,
            profile_edit: null_mut(),
            profile_error_lbl: null_mut(),
            profile_ok_btn: null_mut(),
            lang,
            profile_result: None,
            confirmed: false,
        }
    }
}

unsafe extern "system" fn modal_wnd_proc(
    hwnd: Handle,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let state_ptr = unsafe { user_data::<ModalState>(hwnd) };
    if state_ptr.is_null() {
        return def_window_proc(hwnd, msg, wparam, lparam);
    }
    let state = unsafe { &mut *state_ptr };

    match msg {
        WM_ERASEBKGND => {
            return state.palette.erase_background(hwnd, wparam as Handle);
        }
        WM_SETCURSOR => {
            if state.link_controls.contains(&(wparam as Handle)) {
                set_hand_cursor_now();
                return 1;
            }
        }
        WM_NOTIFY => {
            if let Some(res) =
                handle_dark_button_custom_draw(hwnd, lparam, state.dpi, &state.palette, true)
            {
                return res;
            }
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let ctrl = lparam as Handle;
            let hdc = wparam as Handle;
            let is_link = state.link_controls.contains(&ctrl);
            let is_err = state.error_controls.contains(&ctrl);
            let is_muted = state.muted_controls.contains(&ctrl);
            if is_link {
                let saved = state.palette.handle_ctlcolor(hdc, msg, false, false, false);
                let link_col = if state.palette.dark {
                    0x00F6_9D58
                } else {
                    0x00CC_6600
                };
                unsafe {
                    set_text_color_raw(hdc, link_col);
                }
                return saved;
            }
            return state
                .palette
                .handle_ctlcolor(hdc, msg, is_muted, false, is_err);
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u16;
            let code = ((wparam >> 16) & 0xFFFF) as u16;
            let (idok, idcancel) = dialog_ids();

            match state.kind {
                DialogKind::AdminPrompt => {
                    if code == BN_CLICKED {
                        match id {
                            101 => {
                                state.admin_result = (true, false);
                                state.done.set(true);
                                return 0;
                            }
                            102 => {
                                let checked =
                                    send_msg(state.dont_ask_chk, BM_GETCHECK, 0, 0) as usize
                                        == BST_CHECKED;
                                state.admin_result = (false, checked);
                                state.done.set(true);
                                return 0;
                            }
                            103 => {
                                let _ = open_shell_target(WIKI_URL);
                                return 0;
                            }
                            x if x == idcancel as u16 => {
                                state.admin_result = (false, false);
                                state.done.set(true);
                                return 0;
                            }
                            x if x == idok as u16 => {
                                state.admin_result = (true, false);
                                state.done.set(true);
                                return 0;
                            }
                            _ => {}
                        }
                    }
                }
                DialogKind::About => {
                    if code == BN_CLICKED {
                        match id {
                            101 => {
                                let _ = open_shell_target(PROJECT_URL);
                                return 0;
                            }
                            102 => {
                                let _ = open_shell_target(WIKI_URL);
                                return 0;
                            }
                            x if x == idok as u16 || x == idcancel as u16 => {
                                state.done.set(true);
                                return 0;
                            }
                            _ => {}
                        }
                    }
                }
                DialogKind::ProfileName => {
                    if id == 101 && code == EN_CHANGE {
                        let text = get_window_text(state.profile_edit);
                        let (valid, err_msg) = if text.is_empty() {
                            (false, " ".to_owned())
                        } else if let Some(dir) = state.profile_dir.as_deref() {
                            match validate_new_profile_name(dir, &text) {
                                Ok(()) => (true, " ".to_owned()),
                                Err(err) => (false, tr(state.lang, profile_name_error_key(err)).to_owned()),
                            }
                        } else {
                            (false, " ".to_owned())
                        };
                        set_window_text(state.profile_error_lbl, &err_msg);
                        enable_window(state.profile_ok_btn, valid);
                        return 0;
                    }
                    if code == BN_CLICKED {
                        if id == idok as u16 {
                            let text = get_window_text(state.profile_edit);
                            if let Some(dir) = state.profile_dir.as_deref()
                                && !text.is_empty()
                                && validate_new_profile_name(dir, &text).is_ok()
                            {
                                state.profile_result = Some(text);
                                state.done.set(true);
                            }
                            return 0;
                        }
                        if id == idcancel as u16 {
                            state.profile_result = None;
                            state.done.set(true);
                            return 0;
                        }
                    }
                }
                DialogKind::Message => {
                    if code == BN_CLICKED {
                        if id == 101 || id == idok as u16 {
                            state.confirmed = true;
                            state.done.set(true);
                            return 0;
                        }
                        if id == 102 || id == idcancel as u16 {
                            state.confirmed = false;
                            state.done.set(true);
                            return 0;
                        }
                    }
                }
            }
        }
        WM_CLOSE => {
            state.done.set(true);
            return 0;
        }
        WM_NCDESTROY => {
            set_user_data::<ModalState>(hwnd, null_mut());
            return 0;
        }
        _ => {}
    }

    def_window_proc(hwnd, msg, wparam, lparam)
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn SetTextColor(hdc: Handle, color: u32) -> u32;
}

unsafe fn set_text_color_raw(hdc: Handle, color: u32) {
    unsafe { SetTextColor(hdc, color) };
}

fn spawn_modal_shell(
    owner: Handle,
    title: &str,
    client_w: i32,
    client_h: i32,
    state: &mut Box<ModalState>,
) -> Option<(Handle, u32)> {
    let class_icon = create_app_icon(system_icon_size(96, true))?;
    register_window_class(MODAL_CLASS, modal_wnd_proc, IDC_ARROW, class_icon.raw());
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let ex_style = WS_EX_CONTROLPARENT;
    let base_dpi = if !owner.is_null() {
        dpi_for_window(owner)
    } else {
        96
    };
    let scaled_w = scale(client_w, base_dpi);
    let scaled_h = scale(client_h, base_dpi);
    let (win_w, win_h) = adjust_window_size(style, ex_style, base_dpi, scaled_w, scaled_h);
    let hwnd = create_window(
        ex_style,
        MODAL_CLASS,
        title,
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        win_w,
        win_h,
        owner,
        0,
        null_mut(),
    );
    if hwnd.is_null() {
        return None;
    }
    let dpi = dpi_for_window(hwnd);
    if dpi != base_dpi {
        resize_modal_client(hwnd, owner, dpi, scale(client_w, dpi), scale(client_h, dpi));
    }
    let icon = create_app_icon(system_icon_size(dpi, true)).unwrap_or(class_icon);
    set_window_icon(hwnd, true, icon.raw());
    state._icons.push(icon);
    state.dpi = dpi;
    apply_window_dark_mode(hwnd, state.palette.dark);
    set_user_data(hwnd, (&raw mut **state).cast::<ModalState>());
    center_window(hwnd, owner);
    Some((hwnd, dpi))
}

fn resize_modal_client(
    hwnd: Handle,
    owner: Handle,
    dpi: u32,
    client_w_px: i32,
    client_h_px: i32,
) {
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let ex_style = WS_EX_CONTROLPARENT;
    let (rw, rh) = adjust_window_size(style, ex_style, dpi, client_w_px, client_h_px);
    move_window(hwnd, 0, 0, rw, rh);
    center_window(hwnd, owner);
}

fn add_label_px(
    parent: Handle,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font: Handle,
    id: usize,
    extra_style: u32,
) -> Handle {
    let hwnd = create_window(
        0,
        "STATIC",
        text,
        WS_CHILD | WS_VISIBLE | SS_LEFT | extra_style,
        x,
        y,
        w,
        h,
        parent,
        id,
        null_mut(),
    );
    set_font(hwnd, font);
    hwnd
}

fn add_button_px(
    parent: Handle,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font: Handle,
    id: usize,
    default_btn: bool,
    dark: bool,
) -> Handle {
    let bs = if default_btn {
        BS_DEFPUSHBUTTON
    } else {
        BS_PUSHBUTTON
    };
    let hwnd = create_window(
        0,
        "BUTTON",
        text,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | bs,
        x,
        y,
        w,
        h,
        parent,
        id,
        null_mut(),
    );
    set_font(hwnd, font);
    apply_control_theme(hwnd, dark, ControlTheme::Explorer);
    hwnd
}

pub(crate) fn ask_restart_as_admin(lang: UiLanguage, dark: bool) -> (bool, bool) {
    let mut state = Box::new(ModalState::new(DialogKind::AdminPrompt, dark, lang));
    let Some((hwnd, dpi)) = spawn_modal_shell(null_mut(), "W10Wheel", 420, 180, &mut state) else {
        return (false, false);
    };

    let normal = create_ui_font(dpi, UiFontKind::Normal);
    let bold = create_ui_font(dpi, UiFontKind::Bold);
    let link_font = create_ui_font(dpi, UiFontKind::Link);
    let nf = normal.as_ref().map_or(null_mut(), |f| f.raw());
    let bf = bold.as_ref().map_or(nf, |f| f.raw());
    let lf = link_font.as_ref().map_or(nf, |f| f.raw());

    let margin = scale(14, dpi);
    let ico_sz = scale(32, dpi);
    let text_x = margin + ico_sz + scale(12, dpi);

    let head_txt = tr(lang, "W10Wheel is not running as administrator");
    let body_txt = tr(
        lang,
        "So it won't work in the windows of some programs (those running as administrator). It's highly recommended to run it as administrator.",
    );
    let wiki_txt = tr(lang, "More info (Wiki)");
    let dont_txt = tr(lang, "Do not ask again");
    let restart_txt = tr(lang, "Restart as administrator");
    let cont_txt = tr(lang, "Continue without administrator rights");

    let restart_w = (measure_text_width(hwnd, nf, restart_txt) + scale(44, dpi)).max(scale(140, dpi));
    let cont_w = (measure_text_width(hwnd, nf, cont_txt) + scale(22, dpi)).max(scale(120, dpi));
    let btn_gap = scale(8, dpi);
    let btn_h = scale(28, dpi);

    let col_w = (restart_w + btn_gap + cont_w)
        .max(measure_text_width(hwnd, bf, head_txt))
        .max(scale(340, dpi));
    let head_h = measure_text_height(hwnd, bf, head_txt, col_w).max(scale(18, dpi));
    let body_h = measure_text_height(hwnd, nf, body_txt, col_w).max(scale(32, dpi));
    let wiki_w = measure_text_width(hwnd, lf, wiki_txt) + scale(4, dpi);
    let wiki_h = measure_text_height(hwnd, lf, wiki_txt, col_w).max(scale(16, dpi));
    let chk_w = (measure_text_width(hwnd, nf, dont_txt) + scale(26, dpi)).min(col_w);
    let chk_h = scale(22, dpi);

    let body_y = margin + head_h + scale(6, dpi);
    let wiki_y = body_y + body_h + scale(6, dpi);
    let chk_y = wiki_y + wiki_h + scale(6, dpi);
    let btn_y = chk_y + chk_h + scale(10, dpi);
    resize_modal_client(
        hwnd,
        null_mut(),
        dpi,
        text_x + col_w + margin,
        btn_y + btn_h + margin,
    );

    add_stock_icon(hwnd, &mut state, StockIcon::Info, margin, margin, ico_sz);

    add_label_px(hwnd, head_txt, text_x, margin, col_w, head_h, bf, 0, 0);
    add_label_px(hwnd, body_txt, text_x, body_y, col_w, body_h, nf, 0, 0);
    let wiki_link = add_label_px(
        hwnd,
        wiki_txt,
        text_x,
        wiki_y,
        wiki_w,
        wiki_h,
        lf,
        103,
        SS_NOTIFY,
    );
    state.link_controls.push(wiki_link);

    let dont_ask = create_window(
        0,
        "BUTTON",
        dont_txt,
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
        text_x,
        chk_y,
        chk_w,
        chk_h,
        hwnd,
        104,
        null_mut(),
    );
    set_font(dont_ask, nf);
    apply_control_theme(dont_ask, dark, ControlTheme::ButtonCheckRadio);
    state.dont_ask_chk = dont_ask;

    let cont_x = text_x + col_w - cont_w;
    let restart_x = cont_x - btn_gap - restart_w;
    let restart_btn = add_button_px(
        hwnd,
        restart_txt,
        restart_x,
        btn_y,
        restart_w,
        btn_h,
        nf,
        101,
        true,
        dark,
    );
    set_button_shield(restart_btn, true);
    let mut tips = TooltipManager::new(hwnd, dpi, dark);
    tips.set_tip(
        restart_btn,
        tr(lang, "Windows asks for permission (User Account Control)"),
    );
    state._tips = Some(tips);

    let _continue_btn = add_button_px(
        hwnd,
        cont_txt,
        cont_x,
        btn_y,
        cont_w,
        btn_h,
        nf,
        102,
        false,
        dark,
    );

    if let Some(f) = normal {
        state._fonts.push(f);
    }
    if let Some(f) = bold {
        state._fonts.push(f);
    }
    if let Some(f) = link_font {
        state._fonts.push(f);
    }

    set_focus(restart_btn);
    run_modal_loop(hwnd, null_mut(), &state.done);
    state.admin_result
}

pub(crate) fn show_about(owner: Handle, lang: UiLanguage, dark: bool) {
    if ABOUT_OPEN.swap(true, Ordering::AcqRel) {
        let existing = ABOUT_HWND.load(Ordering::Acquire) as Handle;
        if !existing.is_null() {
            set_foreground(existing);
        }
        return;
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut state = Box::new(ModalState::new(DialogKind::About, dark, lang));
        let Some((hwnd, dpi)) = spawn_modal_shell(owner, tr(lang, "About"), 300, 140, &mut state)
        else {
            return;
        };
        ABOUT_HWND.store(hwnd as usize, Ordering::Release);

        let normal = create_ui_font(dpi, UiFontKind::Normal);
        let bold = create_ui_font(dpi, UiFontKind::Bold);
        let link_font = create_ui_font(dpi, UiFontKind::Link);
        let nf = normal.as_ref().map_or(null_mut(), |f| f.raw());
        let bf = bold.as_ref().map_or(nf, |f| f.raw());
        let lf = link_font.as_ref().map_or(nf, |f| f.raw());

        let margin = scale(14, dpi);
        let ico_sz = scale(48, dpi);
        let content_x = margin + ico_sz + scale(14, dpi);

        let admin_str = tr(lang, if is_process_elevated() { "Yes" } else { "No" });
        let body = format!(
            "{}: {}\n{}: {}\n{}: x64",
            tr(lang, "Version"),
            PROGRAM_VERSION,
            tr(lang, "Running as administrator"),
            admin_str,
            tr(lang, "Architecture"),
        );
        let ok_txt = tr(lang, "OK");

        let head_w = measure_text_width(hwnd, bf, "W10Wheel");
        let head_h = measure_text_height(hwnd, bf, "W10Wheel", scale(260, dpi)).max(scale(18, dpi));
        let body_max_w = body
            .lines()
            .map(|line| measure_text_width(hwnd, nf, line))
            .max()
            .unwrap_or(0);
        let ok_w = (measure_text_width(hwnd, nf, ok_txt) + scale(24, dpi)).max(scale(74, dpi));
        let ok_h = scale(26, dpi);
        let gh_w = measure_text_width(hwnd, lf, "GitHub") + scale(4, dpi);
        let wiki_w = measure_text_width(hwnd, lf, "Wiki") + scale(4, dpi);
        let link_h = measure_text_height(hwnd, lf, "Wiki", wiki_w.max(scale(40, dpi)))
            .max(scale(16, dpi));
        let link_gap = scale(12, dpi);
        let footer_min_w = gh_w + link_gap + wiki_w + scale(20, dpi) + ok_w;

        let col_w = body_max_w
            .max(head_w)
            .max(footer_min_w)
            .max(scale(210, dpi));
        let body_y = margin + head_h + scale(6, dpi);
        let body_h = measure_text_height(hwnd, nf, &body, col_w).max(scale(42, dpi));
        let footer_y = body_y + body_h + scale(12, dpi);
        let link_y = footer_y + (ok_h - link_h) / 2;
        let client_w = content_x + col_w + margin;
        let client_h = (footer_y + ok_h).max(margin + ico_sz) + margin;
        resize_modal_client(hwnd, owner, dpi, client_w, client_h);

        if let Some(big_ico) = create_app_icon(ico_sz) {
            let icon_hwnd = create_window(
                0,
                "STATIC",
                "",
                WS_CHILD | WS_VISIBLE | SS_ICON | SS_REALSIZECONTROL,
                margin,
                margin,
                ico_sz,
                ico_sz,
                hwnd,
                0,
                null_mut(),
            );
            send_msg(icon_hwnd, STM_SETICON, big_ico.raw() as usize, 0);
            state._icons.push(big_ico);
        }

        add_label_px(hwnd, "W10Wheel", content_x, margin, col_w, head_h, bf, 0, 0);
        add_label_px(hwnd, &body, content_x, body_y, col_w, body_h, nf, 0, 0);

        let gh = add_label_px(hwnd, "GitHub", content_x, link_y, gh_w, link_h, lf, 101, SS_NOTIFY);
        let wiki = add_label_px(
            hwnd,
            "Wiki",
            content_x + gh_w + link_gap,
            link_y,
            wiki_w,
            link_h,
            lf,
            102,
            SS_NOTIFY,
        );
        state.link_controls.push(gh);
        state.link_controls.push(wiki);

        let (idok, _) = dialog_ids();
        let ok_btn = add_button_px(
            hwnd,
            ok_txt,
            content_x + col_w - ok_w,
            footer_y,
            ok_w,
            ok_h,
            nf,
            idok as usize,
            true,
            dark,
        );

        if let Some(f) = normal {
            state._fonts.push(f);
        }
        if let Some(f) = bold {
            state._fonts.push(f);
        }
        if let Some(f) = link_font {
            state._fonts.push(f);
        }

        set_focus(ok_btn);
        run_modal_loop(hwnd, owner, &state.done);
    }));

    ABOUT_HWND.store(0, Ordering::Release);
    ABOUT_OPEN.store(false, Ordering::Release);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

pub(crate) fn ask_profile_name(
    owner: Handle,
    lang: UiLanguage,
    dark: bool,
    dir: &Path,
) -> Option<String> {
    let mut state = Box::new(ModalState::new(DialogKind::ProfileName, dark, lang));
    state.profile_dir = Some(dir.to_path_buf());
    let Some((hwnd, dpi)) =
        spawn_modal_shell(owner, tr(lang, "Add Properties"), 328, 126, &mut state)
    else {
        return None;
    };

    let normal = create_ui_font(dpi, UiFontKind::Normal);
    let nf = normal.as_ref().map_or(null_mut(), |f| f.raw());

    let margin = scale(14, dpi);
    let field_w = scale(300, dpi);
    add_label_px(
        hwnd,
        tr(lang, "Properties Name"),
        margin,
        margin,
        field_w,
        scale(18, dpi),
        nf,
        0,
        0,
    );
    let edit = create_window(
        0,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL,
        margin,
        scale(36, dpi),
        field_w,
        scale(24, dpi),
        hwnd,
        101,
        null_mut(),
    );
    set_font(edit, nf);
    apply_control_theme(edit, dark, ControlTheme::Explorer);
    state.profile_edit = edit;

    let err_lbl = add_label_px(
        hwnd,
        " ",
        margin,
        scale(64, dpi),
        field_w,
        scale(18, dpi),
        nf,
        102,
        0,
    );
    state.error_controls.push(err_lbl);
    state.profile_error_lbl = err_lbl;

    let (idok, idcancel) = dialog_ids();
    let btn_w = scale(76, dpi);
    let btn_h = scale(26, dpi);
    let btn_y = scale(86, dpi);
    let cancel_x = margin + field_w - btn_w;
    let ok_x = cancel_x - scale(8, dpi) - btn_w;
    let ok_btn = create_window(
        0,
        "BUTTON",
        tr(lang, "OK"),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_DISABLED | BS_DEFPUSHBUTTON,
        ok_x,
        btn_y,
        btn_w,
        btn_h,
        hwnd,
        idok as usize,
        null_mut(),
    );
    set_font(ok_btn, nf);
    apply_control_theme(ok_btn, dark, ControlTheme::Explorer);
    state.profile_ok_btn = ok_btn;

    let _cancel_btn = add_button_px(
        hwnd,
        tr(lang, "Cancel"),
        cancel_x,
        btn_y,
        btn_w,
        btn_h,
        nf,
        idcancel as usize,
        false,
        dark,
    );

    if let Some(f) = normal {
        state._fonts.push(f);
    }

    set_focus(edit);
    run_modal_loop(hwnd, owner, &state.done);
    state.profile_result
}

pub(crate) fn confirm_warning(
    owner: Handle,
    lang: UiLanguage,
    dark: bool,
    title: &str,
    heading: &str,
    detail: &str,
    action_text: &str,
) -> bool {
    show_message_dialog_inner(
        owner,
        lang,
        dark,
        title,
        heading,
        detail,
        StockIcon::Warning,
        Some((action_text, tr(lang, "Cancel"))),
    )
}

pub(crate) fn show_warning_dialog(
    owner: Handle,
    lang: UiLanguage,
    dark: bool,
    title: &str,
    heading: &str,
    detail: &str,
) {
    let _ = show_message_dialog_inner(
        owner,
        lang,
        dark,
        title,
        heading,
        detail,
        StockIcon::Warning,
        None,
    );
}

pub(crate) fn show_error_dialog(
    owner: Handle,
    lang: UiLanguage,
    dark: bool,
    title: &str,
    heading: &str,
    detail: &str,
) {
    let _ = show_message_dialog_inner(owner, lang, dark, title, heading, detail, StockIcon::Error, None);
}

fn show_message_dialog_inner(
    owner: Handle,
    lang: UiLanguage,
    dark: bool,
    title: &str,
    heading: &str,
    detail: &str,
    icon: StockIcon,
    confirm_buttons: Option<(&str, &str)>,
) -> bool {
    let mut state = Box::new(ModalState::new(DialogKind::Message, dark, lang));
    let Some((hwnd, dpi)) = spawn_modal_shell(owner, title, 380, 140, &mut state) else {
        return false;
    };

    let normal = create_ui_font(dpi, UiFontKind::Normal);
    let bold = create_ui_font(dpi, UiFontKind::Bold);
    let nf = normal.as_ref().map_or(null_mut(), |f| f.raw());
    let bf = bold.as_ref().map_or(nf, |f| f.raw());

    let margin = scale(14, dpi);
    let ico_sz = scale(32, dpi);
    let text_x = margin + ico_sz + scale(12, dpi);
    let btn_gap = scale(8, dpi);
    let btn_h = scale(26, dpi);

    let (btn_w1, btn_w2, min_btn_col) = match confirm_buttons {
        Some((action_txt, cancel_txt)) => {
            let w1 = (measure_text_width(hwnd, nf, action_txt) + scale(22, dpi)).max(scale(74, dpi));
            let w2 = (measure_text_width(hwnd, nf, cancel_txt) + scale(22, dpi)).max(scale(74, dpi));
            let w = w1.max(w2);
            (w, w, w * 2 + btn_gap)
        }
        None => {
            let w = (measure_text_width(hwnd, nf, tr(lang, "OK")) + scale(24, dpi))
                .max(scale(74, dpi));
            (w, 0, w)
        }
    };

    let head_max_w = measure_text_width(hwnd, bf, heading);
    let detail_max_w = detail
        .lines()
        .map(|l| measure_text_width(hwnd, nf, l))
        .max()
        .unwrap_or(0);
    let col_w = head_max_w
        .max(detail_max_w)
        .clamp(scale(260, dpi), scale(380, dpi))
        .max(min_btn_col);
    let head_h = measure_text_height(hwnd, bf, heading, col_w).max(scale(18, dpi));
    let detail_h = if detail.is_empty() {
        0
    } else {
        measure_text_height(hwnd, nf, detail, col_w)
            .clamp(scale(18, dpi), scale(240, dpi))
    };

    let mut next_y = margin + head_h;
    let detail_y = next_y + scale(6, dpi);
    if detail_h > 0 {
        next_y = detail_y + detail_h;
    }
    let btn_y = next_y.max(margin + ico_sz) + scale(12, dpi);
    resize_modal_client(
        hwnd,
        owner,
        dpi,
        text_x + col_w + margin,
        btn_y + btn_h + margin,
    );

    add_stock_icon(hwnd, &mut state, icon, margin, margin, ico_sz);

    add_label_px(hwnd, heading, text_x, margin, col_w, head_h, bf, 0, 0);
    if detail_h > 0 {
        add_label_px(hwnd, detail, text_x, detail_y, col_w, detail_h, nf, 0, 0);
    }

    let (idok, idcancel) = dialog_ids();
    let focus_btn = match confirm_buttons {
        Some((action_txt, cancel_txt)) => {
            let cancel_x = text_x + col_w - btn_w2;
            let action_x = cancel_x - btn_gap - btn_w1;
            add_button_px(
                hwnd,
                action_txt,
                action_x,
                btn_y,
                btn_w1,
                btn_h,
                nf,
                101,
                false,
                dark,
            );
            // Default Cancel button on destructive confirmations
            add_button_px(
                hwnd,
                cancel_txt,
                cancel_x,
                btn_y,
                btn_w2,
                btn_h,
                nf,
                idcancel as usize,
                true,
                dark,
            )
        }
        None => add_button_px(
            hwnd,
            tr(lang, "OK"),
            text_x + col_w - btn_w1,
            btn_y,
            btn_w1,
            btn_h,
            nf,
            idok as usize,
            true,
            dark,
        ),
    };

    if let Some(f) = normal {
        state._fonts.push(f);
    }
    if let Some(f) = bold {
        state._fonts.push(f);
    }

    set_focus(focus_btn);
    run_modal_loop(hwnd, owner, &state.done);
    state.confirmed
}

/// An icon of the shell (info, warning...) as the icons of the dialogs of the .NET version, made at
/// its size on the display (kept with the dialog)
fn add_stock_icon(
    parent: Handle,
    state: &mut Box<ModalState>,
    icon: StockIcon,
    x: i32,
    y: i32,
    size: i32,
) {
    let Some(ico) = create_stock_icon(icon, size) else {
        return;
    };
    let icon_hwnd = create_window(
        0,
        "STATIC",
        "",
        WS_CHILD | WS_VISIBLE | SS_ICON | SS_REALSIZECONTROL,
        x,
        y,
        size,
        size,
        parent,
        0,
        null_mut(),
    );
    send_msg(icon_hwnd, STM_SETICON, ico.raw() as usize, 0);
    state._icons.push(ico);
}
