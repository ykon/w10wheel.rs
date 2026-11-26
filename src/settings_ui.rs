use std::cell::RefCell;
use std::ptr::null_mut;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use w10wheel::config::{AccelPreset, ProcessPriority, Trigger, TriggerKey, UiLanguage, VhAdjusterMethod};
use w10wheel::locale::{
    format_custom_table_status, format_non_exe_warning, format_tooltip, store_status_key, tr,
};
use w10wheel::properties::{ConfigStore, DEFAULT_PROFILE, list_profiles};
use w10wheel::{
    AdminStartResult, EngineConfig, EngineControl, is_process_elevated, restart_as_admin,
    set_process_priority,
};

use crate::dialogs::{ask_profile_name, confirm_warning, show_error_dialog, show_warning_dialog};
use crate::icons::{IconHandle, StockIcon, cached_stock_icon, create_app_icon, system_icon_size};
use crate::theme::{
    ControlTheme, MeasureItemStruct, ThemePalette, apply_control_theme, apply_updown_theme,
    apply_window_dark_mode, draw_icon, handle_dark_button_custom_draw, handle_owner_draw_combobox,
    handle_owner_draw_listbox,
};
use crate::ui_win32::{
    BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_PUSHBUTTON,
    BST_CHECKED, BST_UNCHECKED, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL,
    CB_SETDROPPEDWIDTH, CB_SETITEMHEIGHT, CBN_SELCHANGE, CBS_DROPDOWNLIST, CBS_HASSTRINGS,
    CBS_OWNERDRAWFIXED, CW_USEDEFAULT, EN_CHANGE, EN_KILLFOCUS, ES_AUTOHSCROLL, ES_NUMBER,
    FontHandle, Handle, LB_ADDSTRING, LB_GETCOUNT, LB_GETCURSEL, LB_GETSELCOUNT, LB_GETSELITEMS,
    LB_RESETCONTENT, LB_SETCURSEL, LB_SETITEMHEIGHT, LBN_SELCHANGE, LBS_EXTENDEDSEL,
    LBS_HASSTRINGS, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY, LBS_OWNERDRAWFIXED, Rect, SS_LEFT,
    TooltipManager, UDM_SETBUDDY, UDM_SETPOS32, UDM_SETRANGE32, UDS_ARROWKEYS, UDS_NOTHOUSANDS,
    UDS_SETBUDDYINT, UiFontKind, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT,
    WS_GROUP, WS_HSCROLL, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL, WM_CLOSE,
    WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC,
    WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_MEASUREITEM, WM_NCDESTROY, WM_NOTIFY, WM_TIMER,
    TIP_AUTOPOP_MS, TIP_INITIAL_DELAY_MS, WM_ACTIVATE, WM_MOUSELEAVE, WM_MOUSEMOVE, WM_SHOWWINDOW,
    cursor_pos, track_mouse_leave, adjust_window_size, center_window, create_ui_font, create_window, def_window_proc,
    destroy_window, dpi_for_window, enable_window, get_window_text, invalidate, is_window, kill_timer,
    measure_text_width, move_window, open_shell_target, pick_excluded_programs, redraw_window_all,
    register_window_class, scale, send_msg, set_button_shield, set_font, set_foreground, set_timer,
    set_user_data, set_window_icon, set_window_text, show_window, to_wide, user_data,
};

pub(crate) use w10wheel::wm_app::{ADMIN_RESTART_MESSAGE, TRAY_SYNC_MESSAGE};
pub(crate) const ADMIN_RESTART_STARTED: usize = 0;
pub(crate) const ADMIN_RESTART_CANCELED: usize = 1;
pub(crate) const ADMIN_RESTART_FAILED: usize = 2;

const SETTINGS_CLASS: &str = "W10WheelSettingsWindow";
const NUMERIC_DEBOUNCE_TIMER: usize = 10;
const NUMERIC_DEBOUNCE_MS: u32 = 300;
// The tip of a disabled control: shown after the delay, hidden after the autopop (see
// on_mouse_move_for_disabled_tips)
const DISABLED_TIP_DELAY_TIMER: usize = 11;
const DISABLED_TIP_AUTOPOP_TIMER: usize = 12;
const WA_INACTIVE: usize = 0;
const IDC_ARROW: usize = 32512;
const SW_HIDE: i32 = 0;
const SW_SHOW: i32 = 5;
const PAGE_ALL: u8 = 0xFF;

// The layout, at 96 DPI (as the .NET version: the list of the pages on the left, the page in an
// area of its own on its right, the properties above them and the buttons of the dialog below)
const CLIENT_W: i32 = 624;
const CLIENT_H: i32 = 392;
const MARGIN: i32 = 12;
// The area of the pages, and of the list of the pages
const PANEL_LEFT: i32 = 188;
const PANEL_TOP: i32 = 50;
const PANEL_RIGHT: i32 = CLIENT_W - MARGIN;
const PANEL_BOTTOM: i32 = 338;
const NAV_W: i32 = 168;
const NAV_ITEM_H: i32 = 34;
// The controls of the pages are laid out from (192, 54): moved by this into the area of the pages
// (its padding)
const PAGE_DX: i32 = 8;
const PAGE_DY: i32 = 6;
// The bottom bar
const BOTTOM_Y: i32 = 350;
const BOTTOM_BTN_H: i32 = 28;
// The icon of a note, and the gap after it
const NOTE_ICON: i32 = 16;
const NOTE_ICON_GAP: i32 = 6;
// The box of a number: its width (3 digits and its ▼ ▲ buttons), the width of the buttons
const NUMBER_BOX_W: i32 = 84;
const NUMBER_BOX_H: i32 = 26;
const NUMBER_SPIN_W: i32 = 34;
// The column of the labels (and of the check boxes before a value) in the area of the pages, the
// least width of its texts, and the gap before the column of the values (see value_columns)
const VALUE_LEFT: i32 = 192 + PAGE_DX;
const VALUE_MIN_LABEL_W: i32 = 64;
const VALUE_GAP: i32 = 20;
const ES_RIGHT: u32 = 0x0002;
const UDS_HORZ: u32 = 0x0040;
const EN_SETFOCUS: u16 = 0x0100;

/// The kind of a note (as NoteKind of the .NET version): Info (an explanation, gray with the info
/// icon), Warning (a setting does not work as set, in the color of the texts with the warning
/// icon), Plain (gray, no icon: the explanation of the setting above it)
#[derive(Clone, Copy, PartialEq, Eq)]
enum NoteKind {
    Info,
    Warning,
    Plain,
}

/// A note of a page: its label is moved after its icon (base: its place without an icon, at 96 DPI)
struct NoteSlot {
    hwnd: Handle,
    lx: i32,
    ly: i32,
    lw: i32,
    lh: i32,
    kind: NoteKind,
}

/// A box drawn around a control (a number with its buttons, the list of the excluded programs), in
/// the area of the pages: its rectangle at 96 DPI, and the control whose state (enabled, focused) it
/// shows
struct FrameSlot {
    page: u8,
    control: Handle,
    lx: i32,
    ly: i32,
    lw: i32,
    lh: i32,
}

// Control IDs
const ID_PROFILE_COMBO: u16 = 101;
const ID_PROFILE_ADD: u16 = 102;
const ID_PROFILE_DELETE: u16 = 103;
const ID_PROFILE_OPEN_DIR: u16 = 104;
const ID_PROFILE_RELOAD: u16 = 105;
const ID_NAV_LIST: u16 = 106;
const ID_RESET_BTN: u16 = 107;
const ID_CLOSE_BTN: u16 = 108;

// Page 0: General
const ID_LANG_COMBO: u16 = 200;
const ID_PRIORITY_COMBO: u16 = 201;
const ID_ADMIN_RESTART_BTN: u16 = 202;
const ID_ADMIN_CHECK: u16 = 203;

// Page 1: Trigger
const ID_MOUSE_CHK: u16 = 210;
const ID_MOUSE_COMBO: u16 = 211;
const ID_SEND_MIDDLE_CHK: u16 = 212;
const ID_DRAGGED_LOCK_CHK: u16 = 213;
const ID_KB_CHK: u16 = 214;
const ID_KB_COMBO: u16 = 215;
const ID_POLL_TIMEOUT_EDIT: u16 = 216;
const ID_SCROLL_LOCKTIME_EDIT: u16 = 217;

// Page 2: Scroll
const ID_CURSOR_CHANGE_CHK: u16 = 220;
const ID_HORIZONTAL_CHK: u16 = 221;
const ID_REVERSE_CHK: u16 = 222;
const ID_SWAP_CHK: u16 = 223;
const ID_V_THRESH_EDIT: u16 = 224;
const ID_H_THRESH_EDIT: u16 = 225;

// Page 3: Accel Table
const ID_ACCEL_ENABLED_CHK: u16 = 230;
const ID_ACCEL_M5: u16 = 231;
const ID_ACCEL_M6: u16 = 232;
const ID_ACCEL_M7: u16 = 233;
const ID_ACCEL_M8: u16 = 234;
const ID_ACCEL_M9: u16 = 235;
const ID_CUSTOM_ACCEL_CHK: u16 = 236;

// Page 4: Real Wheel Mode
const ID_REAL_WHEEL_CHK: u16 = 240;
const ID_WHEEL_DELTA_EDIT: u16 = 241;
const ID_V_WHEEL_MOVE_EDIT: u16 = 242;
const ID_H_WHEEL_MOVE_EDIT: u16 = 243;
const ID_QUICK_FIRST_CHK: u16 = 244;
const ID_QUICK_TURN_CHK: u16 = 245;

// Page 5: VH Adjuster
const ID_VHA_ENABLED_CHK: u16 = 250;
const ID_VHA_FIXED_RADIO: u16 = 251;
const ID_VHA_SWITCHING_RADIO: u16 = 252;
const ID_FIRST_PREFER_V_CHK: u16 = 253;
const ID_FIRST_MIN_EDIT: u16 = 254;
const ID_SWITCHING_EDIT: u16 = 255;

// Page 6: Excluded Programs
const ID_EXCL_LIST: u16 = 260;
const ID_EXCL_ADD: u16 = 261;
const ID_EXCL_REMOVE: u16 = 262;

/// The name of a mouse trigger in the list, as the .NET version shows it (Ctx.triggerItems): as in
/// the Readme ("LR", "LeftDrag"...), with how the buttons are pressed for the LR, Left and Right
/// triggers ("LR (Left <<-->> Right)", translated), and in Japanese the X buttons as they are
/// called there ("X1 (拡張1)": Java and .NET translate "X1" and "X2" so, not their drag triggers)
fn mouse_trigger_label(lang: UiLanguage, trigger: Trigger) -> &'static str {
    match trigger {
        Trigger::Lr => tr(lang, "LR (Left <<-->> Right)"),
        Trigger::Left => tr(lang, "Left (Left -->> Right)"),
        Trigger::Right => tr(lang, "Right (Right -->> Left)"),
        Trigger::X1 => tr(lang, "X1"),
        Trigger::X2 => tr(lang, "X2"),
        _ => {
            let name = trigger.as_property_value();
            name.strip_suffix("Trigger").unwrap_or(name)
        }
    }
}

const ACTIVE_MOUSE_TRIGGERS: [Trigger; 11] = [
    Trigger::Lr,
    Trigger::Left,
    Trigger::Right,
    Trigger::Middle,
    Trigger::X1,
    Trigger::X2,
    Trigger::LeftDrag,
    Trigger::RightDrag,
    Trigger::MiddleDrag,
    Trigger::X1Drag,
    Trigger::X2Drag,
];

const KEYBOARD_KEYS: [(TriggerKey, &str); 23] = [
    (TriggerKey::VkTab, "VK_TAB (Tab)"),
    (TriggerKey::VkPause, "VK_PAUSE (Pause)"),
    (TriggerKey::VkCapital, "VK_CAPITAL (Caps Lock)"),
    (TriggerKey::VkConvert, "VK_CONVERT (Henkan)"),
    (TriggerKey::VkNonconvert, "VK_NONCONVERT (Muhenkan)"),
    (TriggerKey::VkPrior, "VK_PRIOR (Page Up)"),
    (TriggerKey::VkNext, "VK_NEXT (Page Down)"),
    (TriggerKey::VkEnd, "VK_END (End)"),
    (TriggerKey::VkHome, "VK_HOME (Home)"),
    (TriggerKey::VkSnapshot, "VK_SNAPSHOT (Print Screen)"),
    (TriggerKey::VkInsert, "VK_INSERT (Insert)"),
    (TriggerKey::VkDelete, "VK_DELETE (Delete)"),
    (TriggerKey::VkLwin, "VK_LWIN (Left Windows)"),
    (TriggerKey::VkRwin, "VK_RWIN (Right Windows)"),
    (TriggerKey::VkApps, "VK_APPS (Application)"),
    (TriggerKey::VkNumlock, "VK_NUMLOCK (Number Lock)"),
    (TriggerKey::VkScroll, "VK_SCROLL (Scroll Lock)"),
    (TriggerKey::VkLshift, "VK_LSHIFT (Left Shift)"),
    (TriggerKey::VkRshift, "VK_RSHIFT (Right Shift)"),
    (TriggerKey::VkLcontrol, "VK_LCONTROL (Left Ctrl)"),
    (TriggerKey::VkRcontrol, "VK_RCONTROL (Right Ctrl)"),
    (TriggerKey::VkLmenu, "VK_LMENU (Left Alt)"),
    (TriggerKey::VkRmenu, "VK_RMENU (Right Alt)"),
];

const ACCEL_PRESETS: [(AccelPreset, u16, &str); 5] = [
    (AccelPreset::M5, ID_ACCEL_M5, "M5 (1.0 ... 4.8)"),
    (AccelPreset::M6, ID_ACCEL_M6, "M6 (1.2 ... 5.8)"),
    (AccelPreset::M7, ID_ACCEL_M7, "M7 (1.4 ... 6.7)"),
    (AccelPreset::M8, ID_ACCEL_M8, "M8 (1.6 ... 7.7)"),
    (AccelPreset::M9, ID_ACCEL_M9, "M9 (1.8 ... 8.7)"),
];

const PAGE_TITLES: [&str; 7] = [
    "General",
    "Trigger",
    "Scroll",
    "Accel Table",
    "Real Wheel Mode",
    "VH Adjuster",
    "Excluded Programs",
];

#[derive(Clone, Copy)]
enum TextBinding {
    Literal(&'static str),
    RangeLabel(i32, i32, Option<&'static str>),
}

struct ControlMeta {
    hwnd: Handle,
    page: u8,
    lx: i32,
    ly: i32,
    lw: i32,
    lh: i32,
    bold: bool,
    theme_kind: ControlTheme,
    text: Option<TextBinding>,
    tip: Option<(&'static str, Option<&'static str>)>,
}

struct NumberBinding {
    edit_id: u16,
    edit_hwnd: Handle,
    updown_hwnd: Handle,
    range_hwnd: Handle,
    label_hwnd: Handle,
    page: u8,
    // The row at 96 DPI (in the area of the pages): its box is laid out in the column of the values
    row_ly: i32,
    low: i32,
    high: i32,
    dirty: bool,
}

pub(crate) struct AppShared {
    pub store: ConfigStore,
    pub control: EngineControl,
    pub dark: bool,
    pub tray_host: Handle,
    pub restarting_admin: bool,
    pub admin_error: Arc<Mutex<Option<String>>>,
}

impl AppShared {
    /// The properties loaded (Reload, switching, deleting or resetting them): the process
    /// priority set, and the triggers given the settings (the keyboard hook installed again, a
    /// scroll ended: EngineControl::load_config)
    pub fn apply_load(&self) {
        let _ = set_process_priority(self.store.config().process_priority);
        self.load_engine_config();
    }

    /// As apply_load, the priority left as it is: for the properties added, a copy of the
    /// selected ones
    pub fn load_engine_config(&self) {
        let _ = self.control.load_config(EngineConfig::from_store(&self.store));
    }
}

pub(crate) struct SettingsWindow {
    hwnd: Handle,
    shared: Rc<RefCell<AppShared>>,
    state: Box<SettingsState>,
}

struct SettingsState {
    hwnd: Handle,
    dpi: u32,
    palette: ThemePalette,
    normal_font: Option<FontHandle>,
    bold_font: Option<FontHandle>,
    _icon_sm: Option<IconHandle>,
    _icon_lg: Option<IconHandle>,
    tips: TooltipManager,
    // The disabled control whose tip is shown, or waits for the delay (null: none)
    disabled_tip_for: Handle,
    selected_page: u8,
    refreshing: bool,
    last_mouse_trigger: Trigger,
    shared: Rc<RefCell<AppShared>>,
    controls: Vec<ControlMeta>,
    numbers: Vec<NumberBinding>,
    muted_controls: Vec<Handle>,
    disabled_labels: Vec<Handle>,
    notes: Vec<NoteSlot>,
    frames: Vec<FrameSlot>,

    // Key dynamic controls
    profile_lbl: Handle,
    profile_combo: Handle,
    profile_names: Vec<String>,
    profile_add_btn: Handle,
    profile_del_btn: Handle,
    profile_open_btn: Handle,
    profile_reload_btn: Handle,
    nav_list: Handle,
    reset_btn: Handle,
    bottom_status_lbl: Handle,
    bottom_status_is_warn: bool,
    close_btn: Handle,

    // Page 0
    lang_lbl: Handle,
    priority_lbl: Handle,
    lang_combo: Handle,
    priority_combo: Handle,
    admin_note_lbl: Handle,
    admin_restart_btn: Handle,
    admin_check: Handle,

    // Page 1
    mouse_chk: Handle,
    mouse_combo: Handle,
    send_middle_chk: Handle,
    dragged_lock_chk: Handle,
    kb_chk: Handle,
    kb_combo: Handle,
    poll_timeout_lbl: Handle,
    scroll_locktime_lbl: Handle,
    trigger_status_lbl: Handle,

    // Page 2
    cursor_change_chk: Handle,
    horizontal_chk: Handle,
    reverse_chk: Handle,
    swap_chk: Handle,
    v_thresh_lbl: Handle,
    h_thresh_lbl: Handle,

    // Page 3
    accel_enabled_chk: Handle,
    accel_radios: [(AccelPreset, Handle); 5],
    custom_accel_chk: Handle,
    custom_status_lbl: Handle,
    custom_status_is_warn: bool,

    // Page 4
    real_wheel_chk: Handle,
    wheel_delta_lbl: Handle,
    v_wheel_move_lbl: Handle,
    h_wheel_move_lbl: Handle,
    quick_first_chk: Handle,
    quick_turn_chk: Handle,

    // Page 5
    vha_enabled_chk: Handle,
    vha_fixed_radio: Handle,
    vha_switching_radio: Handle,
    first_prefer_v_chk: Handle,
    first_min_lbl: Handle,
    switching_lbl: Handle,

    // Page 6
    excl_list: Handle,
    excl_add_btn: Handle,
    excl_remove_btn: Handle,
}

impl SettingsWindow {
    pub fn create(shared: Rc<RefCell<AppShared>>) -> Option<Self> {
        let dark = shared.borrow().dark;
        let lang = shared.borrow().store.config().ui_language;
        let class_icon = create_app_icon(system_icon_size(96, true));
        let class_icon_raw = class_icon.as_ref().map_or(null_mut(), |i| i.raw());
        register_window_class(SETTINGS_CLASS, settings_wnd_proc, IDC_ARROW, class_icon_raw);

        // A fixed dialog with the close box only (as the .NET version)
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
        let ex_style = WS_EX_CONTROLPARENT;
        let (win_w, win_h) = adjust_window_size(style, ex_style, 96, CLIENT_W, CLIENT_H);

        let hwnd = create_window(
            ex_style,
            SETTINGS_CLASS,
            &window_title(lang),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            win_w,
            win_h,
            null_mut(),
            0,
            null_mut(),
        );
        if hwnd.is_null() {
            return None;
        }

        let dpi = dpi_for_window(hwnd);
        if dpi != 96 {
            let (rw, rh) =
                adjust_window_size(style, ex_style, dpi, scale(CLIENT_W, dpi), scale(CLIENT_H, dpi));
            move_window(hwnd, 0, 0, rw, rh);
        }
        let icon_sm = create_app_icon(system_icon_size(dpi, true)).or(class_icon);
        let icon_lg = create_app_icon(system_icon_size(dpi, false));
        if let Some(i) = &icon_sm {
            set_window_icon(hwnd, true, i.raw());
        }
        if let Some(i) = &icon_lg {
            set_window_icon(hwnd, false, i.raw());
        }
        apply_window_dark_mode(hwnd, dark);

        let initial_mouse = {
            let s = shared.borrow();
            if s.store.config().first_trigger.is_none() {
                Trigger::RightDrag
            } else {
                s.store.config().first_trigger
            }
        };

        let mut state = Box::new(SettingsState {
            hwnd,
            dpi,
            palette: ThemePalette::new(dark),
            normal_font: create_ui_font(dpi, UiFontKind::Normal),
            bold_font: create_ui_font(dpi, UiFontKind::Bold),
            _icon_sm: icon_sm,
            _icon_lg: icon_lg,
            tips: TooltipManager::new(hwnd, dpi, dark),
            disabled_tip_for: null_mut(),
            selected_page: 0,
            refreshing: false,
            last_mouse_trigger: initial_mouse,
            shared: Rc::clone(&shared),
            controls: Vec::new(),
            numbers: Vec::new(),
            muted_controls: Vec::new(),
            disabled_labels: Vec::new(),
            notes: Vec::new(),
            frames: Vec::new(),
            profile_lbl: null_mut(),
            profile_combo: null_mut(),
            profile_names: Vec::new(),
            profile_add_btn: null_mut(),
            profile_del_btn: null_mut(),
            profile_open_btn: null_mut(),
            profile_reload_btn: null_mut(),
            nav_list: null_mut(),
            reset_btn: null_mut(),
            bottom_status_lbl: null_mut(),
            bottom_status_is_warn: false,
            close_btn: null_mut(),
            lang_lbl: null_mut(),
            priority_lbl: null_mut(),
            lang_combo: null_mut(),
            priority_combo: null_mut(),
            admin_note_lbl: null_mut(),
            admin_restart_btn: null_mut(),
            admin_check: null_mut(),
            mouse_chk: null_mut(),
            mouse_combo: null_mut(),
            send_middle_chk: null_mut(),
            dragged_lock_chk: null_mut(),
            kb_chk: null_mut(),
            kb_combo: null_mut(),
            poll_timeout_lbl: null_mut(),
            scroll_locktime_lbl: null_mut(),
            trigger_status_lbl: null_mut(),
            cursor_change_chk: null_mut(),
            horizontal_chk: null_mut(),
            reverse_chk: null_mut(),
            swap_chk: null_mut(),
            v_thresh_lbl: null_mut(),
            h_thresh_lbl: null_mut(),
            accel_enabled_chk: null_mut(),
            accel_radios: [(AccelPreset::M5, null_mut()); 5],
            custom_accel_chk: null_mut(),
            custom_status_lbl: null_mut(),
            custom_status_is_warn: false,
            real_wheel_chk: null_mut(),
            wheel_delta_lbl: null_mut(),
            v_wheel_move_lbl: null_mut(),
            h_wheel_move_lbl: null_mut(),
            quick_first_chk: null_mut(),
            quick_turn_chk: null_mut(),
            vha_enabled_chk: null_mut(),
            vha_fixed_radio: null_mut(),
            vha_switching_radio: null_mut(),
            first_prefer_v_chk: null_mut(),
            first_min_lbl: null_mut(),
            switching_lbl: null_mut(),
            excl_list: null_mut(),
            excl_add_btn: null_mut(),
            excl_remove_btn: null_mut(),
        });

        state.build_controls();
        state.retext_all();
        state.refresh_all();
        state.show_page(0);
        center_window(hwnd, null_mut());
        set_user_data(hwnd, (&raw mut *state).cast::<SettingsState>());

        Some(Self {
            hwnd,
            shared,
            state,
        })
    }

    pub fn hwnd(&self) -> Handle {
        self.hwnd
    }

    pub fn is_visible(&self) -> bool {
        is_window_visible(self.hwnd)
    }

    pub fn show(&mut self) {
        self.state.refresh_all();
        if !self.is_visible() {
            center_window(self.hwnd, null_mut());
        }
        show_window(self.hwnd, SW_SHOW);
        set_foreground(self.hwnd);
    }

    pub fn commit_pending(&mut self) {
        self.state.commit_all_numbers();
    }

    /// Shows the settings again, nothing having been loaded (e.g. the keyboard trigger turned off
    /// because its hook could not be installed): the lists of the properties and of the excluded
    /// programs are not read again, and a number being typed is kept (it is stored as it would
    /// have been), as .NET does (RefreshLater).
    pub fn refresh_values(&mut self) {
        self.state.refresh_values();
    }

    pub fn apply_theme(&mut self, dark: bool) {
        self.shared.borrow_mut().dark = dark;
        self.state.apply_theme(dark);
    }

    /// --sendReloadProp. If the file cannot be read (e.g. locked), nothing changes and the bottom
    /// of the settings dialog says so (store_status), as .NET does: no dialog pops up for a
    /// command that a script may send.
    pub fn on_reload_ipc(&mut self) {
        self.state.drop_pending_numbers();
        {
            let mut s = self.shared.borrow_mut();
            if s.store.reload().is_ok() {
                s.apply_load();
            }
        }
        self.state.retext_all();
        self.state.refresh_all();
    }

    pub fn on_admin_restart_result(&mut self, wparam: usize) {
        self.shared.borrow_mut().restarting_admin = false;
        // Nothing loaded: only the button enabled again
        self.state.refresh_values();
        if wparam == ADMIN_RESTART_FAILED {
            let (lang, dark, err_opt) = {
                let s = self.shared.borrow();
                (
                    s.store.config().ui_language,
                    s.dark,
                    s.admin_error.lock().unwrap().take(),
                )
            };
            if let Some(err) = err_opt {
                show_error_dialog(
                    self.hwnd,
                    lang,
                    dark,
                    "W10Wheel",
                    tr(lang, "Failed to restart as administrator"),
                    &err,
                );
            }
        }
    }

    pub fn destroy(&mut self) {
        if is_window(self.hwnd) {
            destroy_window(self.hwnd);
        }
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn IsWindowVisible(hwnd: Handle) -> i32;
    fn IsWindowEnabled(hwnd: Handle) -> i32;
    fn GetFocus() -> Handle;
    fn GetClientRect(hwnd: Handle, rect: *mut Rect) -> i32;
    fn GetWindowRect(hwnd: Handle, rect: *mut Rect) -> i32;
    fn InvalidateRect(hwnd: Handle, rect: *const Rect, erase: i32) -> i32;
    fn MapWindowPoints(from: Handle, to: Handle, points: *mut Rect, count: u32) -> i32;
}

const LBN_SETFOCUS: u16 = 4;
const LBN_KILLFOCUS: u16 = 5;

fn is_window_visible(hwnd: Handle) -> bool {
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 }
}

/// The rectangle of a control in the client area of `parent`
fn client_rect_of(parent: Handle, ctrl: Handle) -> Option<Rect> {
    let mut r = Rect::default();
    unsafe {
        if GetWindowRect(ctrl, &mut r) == 0 {
            return None;
        }
        MapWindowPoints(null_mut(), parent, &mut r, 2);
    }
    Some(r)
}

/// The title of the settings window (as the .NET version: the program, then the dialog)
fn window_title(lang: UiLanguage) -> String {
    format!("W10Wheel - {}", tr(lang, "Settings"))
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: Handle,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if msg == WM_MEASUREITEM && lparam != 0 {
        let mis = unsafe { &mut *(lparam as *mut MeasureItemStruct) };
        let dpi = dpi_for_window(hwnd);
        if mis.ctl_id == ID_NAV_LIST as u32 {
            mis.item_height = scale(NAV_ITEM_H, dpi).max(16) as u32;
            return 1;
        }
        if mis.ctl_id == ID_EXCL_LIST as u32 {
            mis.item_height = scale(22, dpi).max(16) as u32;
            return 1;
        }
        if matches!(
            mis.ctl_id as u16,
            ID_PROFILE_COMBO | ID_LANG_COMBO | ID_PRIORITY_COMBO | ID_MOUSE_COMBO | ID_KB_COMBO
        ) {
            mis.item_height = scale(24, dpi).max(16) as u32;
            return 1;
        }
    }

    let ptr = unsafe { user_data::<SettingsState>(hwnd) };
    if ptr.is_null() {
        return def_window_proc(hwnd, msg, wparam, lparam);
    }
    let state = unsafe { &mut *ptr };

    match msg {
        WM_ERASEBKGND => {
            // Also asked by the controls for the background under them (DrawThemeParentBackground)
            state.paint_background(wparam as Handle);
            return 1;
        }
        WM_DRAWITEM => {
            let ctl_id = wparam as u16;
            let is_nav = ctl_id == ID_NAV_LIST;
            if (is_nav || ctl_id == ID_EXCL_LIST)
                && handle_owner_draw_listbox(hwnd, lparam, state.dpi, &state.palette, is_nav)
            {
                return 1;
            }
            if matches!(
                ctl_id,
                ID_PROFILE_COMBO
                    | ID_LANG_COMBO
                    | ID_PRIORITY_COMBO
                    | ID_MOUSE_COMBO
                    | ID_KB_COMBO
            ) && handle_owner_draw_combobox(hwnd, lparam, state.dpi, &state.palette)
            {
                return 1;
            }
        }
        WM_NOTIFY => {
            if let Some(res) =
                handle_dark_button_custom_draw(hwnd, lparam, state.dpi, &state.palette, false)
            {
                return res;
            }
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let ctrl = lparam as Handle;
            let hdc = wparam as Handle;
            if msg == WM_CTLCOLORSTATIC && state.numbers.iter().any(|n| n.edit_hwnd == ctrl) {
                return state.palette.handle_ctlcolor_disabled_edit(hdc);
            }
            if msg == WM_CTLCOLORLISTBOX && ctrl == state.nav_list {
                // The list of the pages: its items on the background of the window
                return state.palette.ctlcolor_label(hdc, state.palette.text_color, false);
            }
            if msg == WM_CTLCOLOREDIT || msg == WM_CTLCOLORLISTBOX {
                return state.palette.handle_ctlcolor(hdc, msg, false, false, false);
            }
            return state.ctlcolor_label(hdc, ctrl);
        }
        WM_TIMER => {
            if wparam == NUMERIC_DEBOUNCE_TIMER {
                kill_timer(hwnd, NUMERIC_DEBOUNCE_TIMER);
                state.commit_valid_dirty_numbers();
                return 0;
            }
            if wparam == DISABLED_TIP_DELAY_TIMER {
                state.show_disabled_tip_after_delay();
                return 0;
            }
            if wparam == DISABLED_TIP_AUTOPOP_TIMER {
                // Gone after its time, as a tooltip: not shown again on the same control until
                // the mouse has been elsewhere (disabled_tip_for is kept)
                kill_timer(hwnd, DISABLED_TIP_AUTOPOP_TIMER);
                state.tips.hide_tracking();
                return 0;
            }
        }
        // A disabled control gets no mouse messages: Windows sends them to this window
        WM_MOUSEMOVE => {
            state.on_mouse_move_for_disabled_tips();
        }
        WM_MOUSELEAVE => {
            state.hide_disabled_tip();
        }
        // Also when the mouse does not leave: another window activated (Alt+Tab), or this one
        // hidden (Esc, Close)
        WM_ACTIVATE if (wparam & 0xFFFF) == WA_INACTIVE => {
            state.hide_disabled_tip();
        }
        WM_SHOWWINDOW if wparam == 0 => {
            state.hide_disabled_tip();
        }
        WM_DPICHANGED => {
            let new_dpi = (wparam & 0xFFFF) as u32;
            if new_dpi >= 96 && lparam != 0 {
                let suggested = unsafe { &*(lparam as *const Rect) };
                state.on_dpi_changed(new_dpi, suggested);
            }
            return 0;
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u16;
            let code = ((wparam >> 16) & 0xFFFF) as u16;
            if !state.refreshing {
                state.handle_command(id, code);
            }
            return 0;
        }
        WM_CLOSE => {
            state.commit_all_numbers();
            show_window(hwnd, SW_HIDE);
            return 0;
        }
        WM_NCDESTROY => {
            kill_timer(hwnd, NUMERIC_DEBOUNCE_TIMER);
            kill_timer(hwnd, DISABLED_TIP_DELAY_TIMER);
            kill_timer(hwnd, DISABLED_TIP_AUTOPOP_TIMER);
            set_user_data::<SettingsState>(hwnd, null_mut());
            return 0;
        }
        _ => {}
    }

    def_window_proc(hwnd, msg, wparam, lparam)
}

impl SettingsState {
    fn add_ctrl(
        &mut self,
        page: u8,
        class: &str,
        initial_text: &str,
        style: u32,
        ex_style: u32,
        lx: i32,
        ly: i32,
        lw: i32,
        lh: i32,
        id: u16,
        bold: bool,
        theme_kind: ControlTheme,
        text: Option<TextBinding>,
        tip: Option<(&'static str, Option<&'static str>)>,
    ) -> Handle {
        let initial_style = if page != PAGE_ALL && page != 0 {
            style & !WS_VISIBLE
        } else {
            style
        };
        // The controls of a page into the area of the pages
        let (lx, ly) = if page == PAGE_ALL {
            (lx, ly)
        } else {
            (lx + PAGE_DX, ly + PAGE_DY)
        };
        let hwnd = create_window(
            ex_style,
            class,
            initial_text,
            initial_style,
            scale(lx, self.dpi),
            scale(ly, self.dpi),
            scale(lw, self.dpi),
            scale(lh, self.dpi),
            self.hwnd,
            id as usize,
            null_mut(),
        );
        let font = if bold {
            self.bold_font.as_ref()
        } else {
            self.normal_font.as_ref()
        };
        if let Some(f) = font {
            set_font(hwnd, f.raw());
        }
        apply_control_theme(hwnd, self.palette.dark, theme_kind);
        self.controls.push(ControlMeta {
            hwnd,
            page,
            lx,
            ly,
            lw,
            lh,
            bold,
            theme_kind,
            text,
            tip,
        });
        hwnd
    }

    fn add_label(
        &mut self,
        page: u8,
        key: &'static str,
        lx: i32,
        ly: i32,
        lw: i32,
        lh: i32,
        id: u16,
        tip: Option<(&'static str, Option<&'static str>)>,
    ) -> Handle {
        self.add_ctrl(
            page,
            "STATIC",
            key,
            WS_CHILD | WS_VISIBLE | SS_LEFT,
            0,
            lx,
            ly,
            lw,
            lh,
            id,
            false,
            ControlTheme::Plain,
            Some(TextBinding::Literal(key)),
            tip,
        )
    }

    /// A note of a page (Info: an explanation of the page), or one that follows the settings
    /// (refresh_all sets its text and its kind, see NoteSlot)
    fn add_note(
        &mut self,
        page: u8,
        key: Option<&'static str>,
        kind: NoteKind,
        lx: i32,
        ly: i32,
        lw: i32,
        lh: i32,
    ) -> Handle {
        let h = self.add_ctrl(
            page,
            "STATIC",
            key.unwrap_or(" "),
            WS_CHILD | WS_VISIBLE | SS_LEFT,
            0,
            lx,
            ly,
            lw,
            lh,
            0,
            false,
            ControlTheme::Plain,
            key.map(TextBinding::Literal),
            None,
        );
        let (lx, ly) = if page == PAGE_ALL {
            (lx, ly)
        } else {
            (lx + PAGE_DX, ly + PAGE_DY)
        };
        self.notes.push(NoteSlot {
            hwnd: h,
            lx,
            ly,
            lw,
            lh,
            kind,
        });
        h
    }

    fn set_note_kind(&mut self, hwnd: Handle, kind: NoteKind) {
        if let Some(n) = self.notes.iter_mut().find(|n| n.hwnd == hwnd) {
            n.kind = kind;
        }
    }

    fn note_kind(&self, hwnd: Handle) -> Option<NoteKind> {
        self.notes.iter().find(|n| n.hwnd == hwnd).map(|n| n.kind)
    }

    /// The labels of the notes after their icons (the text where the icon is when it has none)
    fn layout_notes(&self) {
        let dpi = self.dpi;
        for n in &self.notes {
            let shift = if n.kind == NoteKind::Plain { 0 } else { NOTE_ICON + NOTE_ICON_GAP };
            move_window(
                n.hwnd,
                scale(n.lx + shift, dpi),
                scale(n.ly, dpi),
                scale(n.lw - shift, dpi),
                scale(n.lh, dpi),
            );
        }
    }

    /// The colors of a label: of its note, grayed if its setting is disabled, on the area of the
    /// pages or on the window
    fn ctlcolor_label(&self, hdc: Handle, ctrl: Handle) -> isize {
        let p = &self.palette;
        let fg = if let Some(kind) = self.note_kind(ctrl) {
            if kind == NoteKind::Warning { p.text_color } else { p.muted_color }
        } else if ctrl == self.bottom_status_lbl {
            if self.bottom_status_is_warn { p.text_color } else { p.muted_color }
        } else if self.disabled_labels.contains(&ctrl) {
            p.disabled_color
        } else if self.muted_controls.contains(&ctrl) {
            p.muted_color
        } else {
            p.text_color
        };
        let on_panel = self
            .controls
            .iter()
            .find(|m| m.hwnd == ctrl)
            .is_some_and(|m| m.page != PAGE_ALL);
        p.ctlcolor_label(hdc, fg, on_panel)
    }

    /// The background: the window, the area of the pages, the boxes of the numbers and the icons
    /// of the notes. Also painted for a control asking for the background under it (the DC is then
    /// the control's, moved to the client area of the window).
    fn paint_background(&self, hdc: Handle) {
        if hdc.is_null() {
            return;
        }
        let dpi = self.dpi;
        let p = &self.palette;
        let mut rc = Rect::default();
        unsafe {
            GetClientRect(self.hwnd, &mut rc);
        }
        p.fill_rect(hdc, rc.left, rc.top, rc.right, rc.bottom, p.bg_color);
        let panel_border = if p.dark { p.panel_color } else { p.divider_color };
        p.fill_round_rect(
            hdc,
            (
                scale(PANEL_LEFT, dpi),
                scale(PANEL_TOP, dpi),
                scale(PANEL_RIGHT, dpi),
                scale(PANEL_BOTTOM, dpi),
            ),
            scale(8, dpi),
            p.panel_color,
            panel_border,
        );

        let focus = unsafe { GetFocus() };
        for f in &self.frames {
            if f.page != self.selected_page {
                continue;
            }
            let enabled = unsafe { IsWindowEnabled(f.control) != 0 };
            let fill = if enabled {
                p.surface_color
            } else {
                p.disabled_surface_color
            };
            p.fill_round_rect(
                hdc,
                (
                    scale(f.lx, dpi),
                    scale(f.ly, dpi),
                    scale(f.lx + f.lw, dpi),
                    scale(f.ly + f.lh, dpi),
                ),
                scale(6, dpi),
                fill,
                p.frame_border(enabled, focus == f.control),
            );
        }

        let icon_sz = scale(NOTE_ICON, dpi);
        let draw_note_icon = |label: Handle, kind: NoteKind| {
            let icon = match kind {
                NoteKind::Info => StockIcon::Info,
                NoteKind::Warning => StockIcon::Warning,
                NoteKind::Plain => return,
            };
            if !is_window_visible(label) || get_window_text(label).trim().is_empty() {
                return;
            }
            let Some(r) = client_rect_of(self.hwnd, label) else {
                return;
            };
            // Centered on the first line of the text
            let line_h = scale(20, dpi).min(r.bottom - r.top);
            let x = r.left - scale(NOTE_ICON_GAP, dpi) - icon_sz;
            let y = r.top + (line_h - icon_sz) / 2;
            draw_icon(hdc, cached_stock_icon(icon, icon_sz), x, y, icon_sz);
        };
        for n in &self.notes {
            draw_note_icon(n.hwnd, n.kind);
        }
        if self.bottom_status_is_warn {
            draw_note_icon(self.bottom_status_lbl, NoteKind::Warning);
        }
    }

    /// Repaints the box of a control (its border shows the focus)
    fn invalidate_frame_of(&self, control: Handle) {
        let dpi = self.dpi;
        if let Some(f) = self.frames.iter().find(|f| f.control == control) {
            let rc = Rect {
                left: scale(f.lx, dpi) - 1,
                top: scale(f.ly, dpi) - 1,
                right: scale(f.lx + f.lw, dpi) + 1,
                bottom: scale(f.ly + f.lh, dpi) + 1,
            };
            unsafe {
                InvalidateRect(self.hwnd, &rc, 1);
            }
        }
    }

    fn add_checkbox(
        &mut self,
        page: u8,
        key: &'static str,
        lx: i32,
        ly: i32,
        lw: i32,
        id: u16,
        tip_key: &'static str,
        prop_key: &'static str,
    ) -> Handle {
        self.add_ctrl(
            page,
            "BUTTON",
            key,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
            0,
            lx,
            ly,
            lw,
            22,
            id,
            false,
            ControlTheme::ButtonCheckRadio,
            Some(TextBinding::Literal(key)),
            Some((tip_key, Some(prop_key))),
        )
    }

    fn add_button(
        &mut self,
        page: u8,
        key: &'static str,
        lx: i32,
        ly: i32,
        lw: i32,
        lh: i32,
        id: u16,
        tip: Option<(&'static str, Option<&'static str>)>,
    ) -> Handle {
        self.add_ctrl(
            page,
            "BUTTON",
            key,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON,
            0,
            lx,
            ly,
            lw,
            lh,
            id,
            false,
            ControlTheme::Explorer,
            Some(TextBinding::Literal(key)),
            tip,
        )
    }

    fn add_combo(
        &mut self,
        page: u8,
        lx: i32,
        ly: i32,
        lw: i32,
        lh: i32,
        id: u16,
        tip: Option<(&'static str, Option<&'static str>)>,
    ) -> Handle {
        let hwnd = self.add_ctrl(
            page,
            "COMBOBOX",
            "",
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | CBS_DROPDOWNLIST
                | CBS_OWNERDRAWFIXED
                | CBS_HASSTRINGS,
            0,
            lx,
            ly,
            lw,
            lh,
            id,
            false,
            ControlTheme::ComboBox,
            None,
            tip,
        );
        send_msg(
            hwnd,
            CB_SETITEMHEIGHT,
            usize::MAX,
            scale(22, self.dpi) as isize,
        );
        send_msg(hwnd, CB_SETITEMHEIGHT, 0, scale(24, self.dpi) as isize);
        hwnd
    }

    fn add_number_row(
        &mut self,
        page: u8,
        label_key: &'static str,
        ly: i32,
        edit_id: u16,
        low: i32,
        high: i32,
        unit_key: Option<&'static str>,
        tip_key: &'static str,
        prop_key: &'static str,
    ) -> Handle {
        // A row as the NumericUpDown of the .NET version: the label, the box (the number on the
        // right, its ▼ ▲ buttons side by side) drawn by paint_background around a borderless edit
        // and its up-down control, and the range
        const BOX_X: i32 = 388;
        let lbl = self.add_label(
            page,
            label_key,
            192,
            ly + 4,
            BOX_X - 192 - 6,
            20,
            0,
            Some((tip_key, Some(prop_key))),
        );
        let edit = self.add_ctrl(
            page,
            "EDIT",
            "",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL | ES_NUMBER | ES_RIGHT,
            0,
            BOX_X + 6,
            ly + 4,
            NUMBER_BOX_W - 6 - NUMBER_SPIN_W - 6,
            18,
            edit_id,
            false,
            ControlTheme::Explorer,
            None,
            Some((tip_key, Some(prop_key))),
        );
        let updown = self.add_ctrl(
            page,
            "msctls_updown32",
            "",
            WS_CHILD | WS_VISIBLE | UDS_HORZ | UDS_SETBUDDYINT | UDS_ARROWKEYS | UDS_NOTHOUSANDS,
            0,
            BOX_X + NUMBER_BOX_W - NUMBER_SPIN_W - 2,
            ly + 2,
            NUMBER_SPIN_W,
            NUMBER_BOX_H - 4,
            0,
            false,
            ControlTheme::Plain,
            None,
            None,
        );
        send_msg(updown, UDM_SETBUDDY, edit as usize, 0);
        send_msg(updown, UDM_SETRANGE32, low as usize, high as isize);
        apply_updown_theme(updown, self.palette.dark);
        self.frames.push(FrameSlot {
            page,
            control: edit,
            lx: BOX_X + PAGE_DX,
            ly: ly + PAGE_DY,
            lw: NUMBER_BOX_W,
            lh: NUMBER_BOX_H,
        });

        let range_x = BOX_X + NUMBER_BOX_W + 8;
        let range_lbl = self.add_ctrl(
            page,
            "STATIC",
            "",
            WS_CHILD | WS_VISIBLE | SS_LEFT,
            0,
            range_x,
            ly + 4,
            PANEL_RIGHT - PAGE_DX - 4 - range_x,
            20,
            0,
            false,
            ControlTheme::Plain,
            Some(TextBinding::RangeLabel(low, high, unit_key)),
            None,
        );
        self.muted_controls.push(range_lbl);

        self.numbers.push(NumberBinding {
            edit_id,
            edit_hwnd: edit,
            updown_hwnd: updown,
            range_hwnd: range_lbl,
            label_hwnd: lbl,
            page,
            row_ly: ly + PAGE_DY,
            low,
            high,
            dirty: false,
        });
        lbl
    }

    fn build_controls(&mut self) {
        // Top Profile Bar
        self.profile_lbl = self.add_label(PAGE_ALL, "Properties", 12, 14, 72, 20, 0, None);
        self.profile_combo = self.add_combo(PAGE_ALL, 90, 10, 140, 220, ID_PROFILE_COMBO, None);
        self.profile_add_btn =
            self.add_button(PAGE_ALL, "Add", 238, 10, 64, 26, ID_PROFILE_ADD, None);
        self.profile_del_btn =
            self.add_button(PAGE_ALL, "Delete", 308, 10, 64, 26, ID_PROFILE_DELETE, None);
        self.profile_open_btn =
            self.add_button(PAGE_ALL, "Open Dir", 378, 10, 96, 26, ID_PROFILE_OPEN_DIR, None);
        self.profile_reload_btn =
            self.add_button(PAGE_ALL, "Reload", 480, 10, 74, 26, ID_PROFILE_RELOAD, None);

        // Left Nav ListBox
        self.nav_list = self.add_ctrl(
            PAGE_ALL,
            "LISTBOX",
            "",
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | LBS_NOTIFY
                | LBS_OWNERDRAWFIXED
                | LBS_HASSTRINGS
                | LBS_NOINTEGRALHEIGHT,
            0,
            MARGIN,
            PANEL_TOP,
            NAV_W,
            PANEL_BOTTOM - PANEL_TOP,
            ID_NAV_LIST,
            false,
            ControlTheme::Explorer,
            None,
            None,
        );
        send_msg(
            self.nav_list,
            LB_SETITEMHEIGHT,
            0,
            scale(NAV_ITEM_H, self.dpi) as isize,
        );

        // Page 0: General
        self.lang_lbl = self.add_label(
            0,
            "Language",
            192,
            59,
            82,
            20,
            0,
            Some(("The language of the menu and the dialogs", Some("uiLanguage"))),
        );
        self.lang_combo = self.add_combo(
            0,
            282,
            55,
            120,
            120,
            ID_LANG_COMBO,
            Some(("The language of the menu and the dialogs", Some("uiLanguage"))),
        );

        self.priority_lbl = self.add_label(
            0,
            "Priority",
            192,
            91,
            82,
            20,
            0,
            Some(("The priority of the process of W10Wheel", Some("processPriority"))),
        );
        self.priority_combo = self.add_combo(
            0,
            282,
            87,
            190,
            140,
            ID_PRIORITY_COMBO,
            Some(("The priority of the process of W10Wheel", Some("processPriority"))),
        );

        let is_admin = is_process_elevated();
        let next_y = if !is_admin {
            self.admin_note_lbl = self.add_note(
                0,
                Some("W10Wheel is not running as administrator"),
                NoteKind::Warning,
                192,
                132,
                400,
                22,
            );
            self.admin_restart_btn = self.add_button(
                0,
                "Restart as administrator",
                192,
                156,
                210,
                28,
                ID_ADMIN_RESTART_BTN,
                Some(("Windows asks for permission (User Account Control)", None)),
            );
            set_button_shield(self.admin_restart_btn, true);
            196
        } else {
            132
        };

        self.admin_check = self.add_checkbox(
            0,
            "Ask to restart as administrator at startup",
            192,
            next_y,
            400,
            ID_ADMIN_CHECK,
            "Asked when W10Wheel is not running as administrator",
            "adminCheck",
        );

        // Page 1: Trigger
        self.mouse_chk = self.add_checkbox(
            1,
            "Mouse",
            192,
            57,
            104,
            ID_MOUSE_CHK,
            "The mouse buttons that start scroll mode\nUnchecked: no mouse trigger (the keyboard trigger only)",
            "firstTrigger",
        );
        self.mouse_combo = self.add_combo(
            1,
            304,
            55,
            140,
            240,
            ID_MOUSE_COMBO,
            Some((
                "The mouse buttons that start scroll mode\nUnchecked: no mouse trigger (the keyboard trigger only)",
                Some("firstTrigger"),
            )),
        );
        self.send_middle_chk = self.add_checkbox(
            1,
            "Send MiddleClick",
            212,
            86,
            380,
            ID_SEND_MIDDLE_CHK,
            "The trigger pressed with Shift, Ctrl or Alt sends a middle click (Middle, X1, X2 only)",
            "sendMiddleClick",
        );
        self.dragged_lock_chk = self.add_checkbox(
            1,
            "Dragged Lock",
            212,
            112,
            380,
            ID_DRAGGED_LOCK_CHK,
            "After a drag with a Drag trigger, scroll mode stays locked",
            "draggedLock",
        );
        self.kb_chk = self.add_checkbox(
            1,
            "Keyboard",
            192,
            144,
            104,
            ID_KB_CHK,
            "The key that starts scroll mode\nThe keys sent by other software (autofill, macros, key remapping tools) are not triggers",
            "keyboardHook, targetVKCode",
        );
        self.kb_combo = self.add_combo(
            1,
            304,
            142,
            220,
            240,
            ID_KB_COMBO,
            Some((
                "The key that starts scroll mode\nThe keys sent by other software (autofill, macros, key remapping tools) are not triggers",
                Some("keyboardHook, targetVKCode"),
            )),
        );
        self.poll_timeout_lbl = self.add_number_row(
            1,
            "Simultaneous press time",
            180,
            ID_POLL_TIMEOUT_EDIT,
            50,
            500,
            Some("ms"),
            "How long the LR, Left and Right triggers wait for the other button",
            "pollTimeout",
        );
        self.scroll_locktime_lbl = self.add_number_row(
            1,
            "Scroll mode lock time",
            212,
            ID_SCROLL_LOCKTIME_EDIT,
            150,
            500,
            Some("ms"),
            "A trigger released within this time locks scroll mode",
            "scrollLocktime",
        );
        self.trigger_status_lbl = self.add_note(1, None, NoteKind::Warning, 192, 248, 400, 22);

        // Page 2: Scroll
        self.cursor_change_chk = self.add_checkbox(
            2,
            "Cursor Change",
            192,
            56,
            400,
            ID_CURSOR_CHANGE_CHK,
            "Changes the cursor in scroll mode",
            "cursorChange",
        );
        self.horizontal_chk = self.add_checkbox(
            2,
            "Horizontal Scroll",
            192,
            82,
            400,
            ID_HORIZONTAL_CHK,
            "Scrolls horizontally too\nDisable it if you do not use it",
            "horizontalScroll",
        );
        self.reverse_chk = self.add_checkbox(
            2,
            "Reverse Scroll (Flip)",
            192,
            108,
            400,
            ID_REVERSE_CHK,
            "Reverses the direction of the scroll",
            "reverseScroll",
        );
        self.swap_chk = self.add_checkbox(
            2,
            "Swap Scroll (V.H)",
            192,
            134,
            400,
            ID_SWAP_CHK,
            "Swaps the vertical and the horizontal scroll",
            "swapScroll",
        );
        self.v_thresh_lbl = self.add_number_row(
            2,
            "Vertical scroll threshold",
            170,
            ID_V_THRESH_EDIT,
            0,
            500,
            None,
            "A vertical move up to this does not scroll",
            "verticalThreshold",
        );
        self.h_thresh_lbl = self.add_number_row(
            2,
            "Horizontal scroll threshold",
            202,
            ID_H_THRESH_EDIT,
            0,
            500,
            None,
            "A horizontal move up to this does not scroll\nToo small, the vertical scroll becomes hard to use",
            "horizontalThreshold",
        );
        self.add_note(
            2,
            Some("The thresholds are not used while VH Adjuster is enabled"),
            NoteKind::Info,
            192,
            238,
            400,
            22,
        );

        // Page 3: Accel Table
        self.accel_enabled_chk = self.add_checkbox(
            3,
            "Enabled",
            192,
            56,
            400,
            ID_ACCEL_ENABLED_CHK,
            "Accelerates the scroll by the speed of the move, with the multipliers of the table",
            "accelTable",
        );
        for (idx, (preset, id, label)) in ACCEL_PRESETS.iter().enumerate() {
            let group_flag = if idx == 0 { WS_GROUP } else { 0 };
            let radio = self.add_ctrl(
                3,
                "BUTTON",
                label,
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTORADIOBUTTON | group_flag,
                0,
                212,
                82 + (idx as i32) * 24,
                320,
                22,
                *id,
                false,
                ControlTheme::ButtonCheckRadio,
                None,
                Some(("The multipliers of the table", Some("accelMultiplier"))),
            );
            self.accel_radios[idx] = (*preset, radio);
        }
        self.custom_accel_chk = self.add_checkbox(
            3,
            "Custom Table",
            192,
            208,
            400,
            ID_CUSTOM_ACCEL_CHK,
            "Uses customAccelThreshold and customAccelMultiplier of the properties file (only when they are valid)",
            "customAccelTable",
        );
        // Where the text of the check box starts (the state of the custom table)
        self.custom_status_lbl = self.add_note(3, None, NoteKind::Plain, 215, 234, 377, 34);
        self.add_note(
            3,
            Some("Not used while Real Wheel Mode is enabled"),
            NoteKind::Info,
            192,
            276,
            400,
            22,
        );

        // Page 4: Real Wheel Mode
        self.real_wheel_chk = self.add_checkbox(
            4,
            "Enabled",
            192,
            56,
            400,
            ID_REAL_WHEEL_CHK,
            "Scrolls as a real wheel does, a notch at a time\nThe accel table is not used",
            "realWheelMode",
        );
        self.wheel_delta_lbl = self.add_number_row(
            4,
            "Wheel delta",
            90,
            ID_WHEEL_DELTA_EDIT,
            10,
            500,
            None,
            "The wheel value of a notch (a usual mouse sends 120)",
            "wheelDelta",
        );
        self.v_wheel_move_lbl = self.add_number_row(
            4,
            "Vertical move per notch",
            122,
            ID_V_WHEEL_MOVE_EDIT,
            10,
            500,
            None,
            "The vertical move made into a notch",
            "vWheelMove",
        );
        self.h_wheel_move_lbl = self.add_number_row(
            4,
            "Horizontal move per notch",
            154,
            ID_H_WHEEL_MOVE_EDIT,
            10,
            500,
            None,
            "The horizontal move made into a notch",
            "hWheelMove",
        );
        self.quick_first_chk = self.add_checkbox(
            4,
            "Quick first response",
            192,
            190,
            400,
            ID_QUICK_FIRST_CHK,
            "Sends the first notch whatever the move (else after half of the move)",
            "quickFirst",
        );
        self.quick_turn_chk = self.add_checkbox(
            4,
            "Quick turn response",
            192,
            216,
            400,
            ID_QUICK_TURN_CHK,
            "Sends a notch at once when the move turns back, whatever the move",
            "quickTurn",
        );

        // Page 5: VH Adjuster
        self.vha_enabled_chk = self.add_checkbox(
            5,
            "Enabled",
            192,
            56,
            400,
            ID_VHA_ENABLED_CHK,
            "Scrolls in one direction at a time, vertical or horizontal",
            "vhAdjusterMode",
        );
        self.vha_fixed_radio = self.add_ctrl(
            5,
            "BUTTON",
            "Fixed",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTORADIOBUTTON | WS_GROUP,
            0,
            212,
            82,
            320,
            22,
            ID_VHA_FIXED_RADIO,
            false,
            ControlTheme::ButtonCheckRadio,
            Some(TextBinding::Literal("Fixed")),
            Some((
                "Fixed: the direction of the larger first move\nSwitching: also switched by a move above the switching threshold",
                Some("vhAdjusterMethod"),
            )),
        );
        self.vha_switching_radio = self.add_ctrl(
            5,
            "BUTTON",
            "Switching",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTORADIOBUTTON,
            0,
            212,
            106,
            320,
            22,
            ID_VHA_SWITCHING_RADIO,
            false,
            ControlTheme::ButtonCheckRadio,
            Some(TextBinding::Literal("Switching")),
            Some((
                "Fixed: the direction of the larger first move\nSwitching: also switched by a move above the switching threshold",
                Some("vhAdjusterMethod"),
            )),
        );
        self.first_prefer_v_chk = self.add_checkbox(
            5,
            "Prefer vertical first",
            192,
            138,
            400,
            ID_FIRST_PREFER_V_CHK,
            "The first direction prefers vertical",
            "firstPreferVertical",
        );
        self.first_min_lbl = self.add_number_row(
            5,
            "First direction threshold",
            172,
            ID_FIRST_MIN_EDIT,
            1,
            10,
            None,
            "The first direction is decided by a move above this (smaller moves are ignored)",
            "firstMinThreshold",
        );
        self.switching_lbl = self.add_number_row(
            5,
            "Switching threshold",
            204,
            ID_SWITCHING_EDIT,
            10,
            500,
            None,
            "A move above this switches the direction (Switching)",
            "switchingThreshold",
        );
        self.add_note(
            5,
            Some("Used when Horizontal Scroll is enabled"),
            NoteKind::Info,
            192,
            240,
            400,
            22,
        );

        // Page 6: Excluded Programs (the list in a box drawn by paint_background, as the numbers)
        self.excl_list = self.add_ctrl(
            6,
            "LISTBOX",
            "",
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WS_HSCROLL
                | LBS_NOTIFY
                | LBS_EXTENDEDSEL
                | LBS_OWNERDRAWFIXED
                | LBS_HASSTRINGS
                | LBS_NOINTEGRALHEIGHT,
            0,
            192,
            54,
            400,
            204,
            ID_EXCL_LIST,
            false,
            ControlTheme::Explorer,
            None,
            Some((
                "The programs (.exe) whose windows have scrolling disabled\nJudged by the window under the cursor when a trigger is pressed\n(UWP / Store apps are not supported)",
                None,
            )),
        );
        send_msg(
            self.excl_list,
            LB_SETITEMHEIGHT,
            0,
            scale(22, self.dpi) as isize,
        );
        self.frames.push(FrameSlot {
            page: 6,
            control: self.excl_list,
            lx: 192 + PAGE_DX - 2,
            ly: 54 + PAGE_DY - 2,
            lw: 400 + 4,
            lh: 204 + 4,
        });
        self.excl_add_btn =
            self.add_button(6, "Add...", 192, 266, 88, 28, ID_EXCL_ADD, None);
        self.excl_remove_btn =
            self.add_button(6, "Remove", 286, 266, 88, 28, ID_EXCL_REMOVE, None);
        self.add_note(
            6,
            Some("Scrolling is disabled in these programs"),
            NoteKind::Info,
            192,
            302,
            400,
            22,
        );

        // Bottom Bar
        self.reset_btn = self.add_button(
            PAGE_ALL,
            "Reset to defaults",
            MARGIN,
            BOTTOM_Y,
            120,
            BOTTOM_BTN_H,
            ID_RESET_BTN,
            None,
        );
        self.bottom_status_lbl = self.add_ctrl(
            PAGE_ALL,
            "STATIC",
            "",
            WS_CHILD | WS_VISIBLE | SS_LEFT,
            0,
            142,
            BOTTOM_Y + 5,
            360,
            20,
            0,
            false,
            ControlTheme::Plain,
            None,
            None,
        );
        self.close_btn = self.add_button(
            PAGE_ALL,
            "Close",
            CLIENT_W - MARGIN - 80,
            BOTTOM_Y,
            80,
            BOTTOM_BTN_H,
            ID_CLOSE_BTN,
            None,
        );
        // The lists of the mouse triggers and the trigger keys are filled in the language
        // (retext_all)
    }

    fn on_dpi_changed(&mut self, new_dpi: u32, suggested: &Rect) {
        self.dpi = new_dpi;
        self.normal_font = create_ui_font(new_dpi, UiFontKind::Normal);
        self.bold_font = create_ui_font(new_dpi, UiFontKind::Bold);
        self._icon_sm = create_app_icon(system_icon_size(new_dpi, true));
        self._icon_lg = create_app_icon(system_icon_size(new_dpi, false));
        if let Some(i) = &self._icon_sm {
            set_window_icon(self.hwnd, true, i.raw());
        }
        if let Some(i) = &self._icon_lg {
            set_window_icon(self.hwnd, false, i.raw());
        }
        self.tips.update_dpi(new_dpi);
        move_window(
            self.hwnd,
            suggested.left,
            suggested.top,
            suggested.right - suggested.left,
            suggested.bottom - suggested.top,
        );
        let nf = self.normal_font.as_ref().map_or(null_mut(), |f| f.raw());
        let bf = self.bold_font.as_ref().map_or(nf, |f| f.raw());
        for meta in &self.controls {
            set_font(meta.hwnd, if meta.bold { bf } else { nf });
            move_window(
                meta.hwnd,
                scale(meta.lx, new_dpi),
                scale(meta.ly, new_dpi),
                scale(meta.lw, new_dpi),
                scale(meta.lh, new_dpi),
            );
        }
        send_msg(
            self.nav_list,
            LB_SETITEMHEIGHT,
            0,
            scale(NAV_ITEM_H, new_dpi) as isize,
        );
        send_msg(
            self.excl_list,
            LB_SETITEMHEIGHT,
            0,
            scale(22, new_dpi) as isize,
        );
        for cb in [
            self.profile_combo,
            self.lang_combo,
            self.priority_combo,
            self.mouse_combo,
            self.kb_combo,
        ] {
            send_msg(
                cb,
                CB_SETITEMHEIGHT,
                usize::MAX,
                scale(22, new_dpi) as isize,
            );
            send_msg(cb, CB_SETITEMHEIGHT, 0, scale(24, new_dpi) as isize);
        }
        self.relayout_dynamic_controls();
        redraw_window_all(self.hwnd, false);
    }

    fn apply_theme(&mut self, dark: bool) {
        self.palette = ThemePalette::new(dark);
        apply_window_dark_mode(self.hwnd, dark);
        for meta in &self.controls {
            apply_control_theme(meta.hwnd, dark, meta.theme_kind);
        }
        for nb in &self.numbers {
            apply_updown_theme(nb.updown_hwnd, dark);
        }
        self.tips.apply_theme(dark);
        redraw_window_all(self.hwnd, true);
    }

    /// The tooltips of the grayed controls (e.g. Custom Table, Dragged Lock), as the .NET version
    /// shows them (SettingsForm.showTipsOfDisabled): a disabled control gets no mouse messages, so
    /// its tooltip never shows. This window gets them instead and shows the tip of the disabled
    /// control under the mouse, as a tooltip does: after its delay (not as soon as the mouse
    /// crosses one), for its time, and not again on the same control until the mouse has gone.
    fn on_mouse_move_for_disabled_tips(&mut self) {
        // WM_MOUSELEAVE when the mouse leaves this window, also onto an enabled control
        track_mouse_leave(self.hwnd);
        let target = self.tips.disabled_tool_at(cursor_pos());
        let current = self.disabled_tip_for;
        let same = match target {
            None => current.is_null(),
            // The controls of one setting have the same tip (e.g. the box of a number and its
            // arrows): it stays from one to the other
            Some(t) => {
                !current.is_null()
                    && (t == current || self.tips.tip_text(t) == self.tips.tip_text(current))
            }
        };
        if same {
            return;
        }
        self.hide_disabled_tip();
        if let Some(t) = target {
            self.disabled_tip_for = t;
            set_timer(self.hwnd, DISABLED_TIP_DELAY_TIMER, TIP_INITIAL_DELAY_MS);
        }
    }

    fn show_disabled_tip_after_delay(&mut self) {
        kill_timer(self.hwnd, DISABLED_TIP_DELAY_TIMER);
        let control = self.disabled_tip_for;
        // Still disabled: it may have been enabled while waiting (e.g. --sendReloadProp)
        if control.is_null() || unsafe { IsWindowEnabled(control) } != 0 {
            return;
        }
        // Below the mouse, as a tooltip shows
        let mut at = cursor_pos();
        at.y += scale(20, self.dpi);
        self.tips.show_tracking(control, at);
        set_timer(self.hwnd, DISABLED_TIP_AUTOPOP_TIMER, TIP_AUTOPOP_MS);
    }

    fn hide_disabled_tip(&mut self) {
        kill_timer(self.hwnd, DISABLED_TIP_DELAY_TIMER);
        kill_timer(self.hwnd, DISABLED_TIP_AUTOPOP_TIMER);
        self.tips.hide_tracking();
        self.disabled_tip_for = null_mut();
    }

    fn show_page(&mut self, page: u8) {
        // Another page (e.g. by the arrow keys in the list): the tip of this one is not left shown
        self.hide_disabled_tip();
        self.selected_page = page;
        for meta in &self.controls {
            if meta.page == PAGE_ALL || meta.page == page {
                show_window(meta.hwnd, SW_SHOW);
            } else {
                show_window(meta.hwnd, SW_HIDE);
            }
        }
        send_msg(self.nav_list, LB_SETCURSEL, page as usize, 0);
        redraw_window_all(self.hwnd, false);
    }

    fn retext_all(&mut self) {
        let lang = self.shared.borrow().store.config().ui_language;
        set_window_text(self.hwnd, &window_title(lang));

        for meta in &self.controls {
            if let Some(binding) = meta.text {
                match binding {
                    TextBinding::Literal(key) => set_window_text(meta.hwnd, tr(lang, key)),
                    TextBinding::RangeLabel(low, high, unit) => {
                        // As the .NET version: "(50 - 500 ms)"
                        let s = match unit {
                            Some(u) => format!("({low} - {high} {u})"),
                            None => format!("({low} - {high})"),
                        };
                        set_window_text(meta.hwnd, &s);
                    }
                }
            }
            if let Some((tip_key, prop_key)) = meta.tip {
                let text = format_tooltip(lang, tip_key, prop_key);
                self.tips.set_tip(meta.hwnd, &text);
            }
        }

        self.refreshing = true;
        // Nav list
        send_msg(self.nav_list, LB_RESETCONTENT, 0, 0);
        for title in PAGE_TITLES {
            let w = to_wide(tr(lang, title));
            send_msg(self.nav_list, LB_ADDSTRING, 0, w.as_ptr() as isize);
        }
        send_msg(self.nav_list, LB_SETCURSEL, self.selected_page as usize, 0);

        // Language combo
        send_msg(self.lang_combo, CB_RESETCONTENT, 0, 0);
        for key in ["English", "Japanese"] {
            let w = to_wide(tr(lang, key));
            send_msg(self.lang_combo, CB_ADDSTRING, 0, w.as_ptr() as isize);
        }

        // Priority combo
        send_msg(self.priority_combo, CB_RESETCONTENT, 0, 0);
        for key in ["High", "Above Normal", "Normal"] {
            let w = to_wide(tr(lang, key));
            send_msg(self.priority_combo, CB_ADDSTRING, 0, w.as_ptr() as isize);
        }

        // Mouse trigger combo: its selection kept (refresh_all sets it from the settings too)
        let mouse_sel = send_msg(self.mouse_combo, CB_GETCURSEL, 0, 0);
        send_msg(self.mouse_combo, CB_RESETCONTENT, 0, 0);
        for trigger in ACTIVE_MOUSE_TRIGGERS {
            let w = to_wide(mouse_trigger_label(lang, trigger));
            send_msg(self.mouse_combo, CB_ADDSTRING, 0, w.as_ptr() as isize);
        }
        if mouse_sel >= 0 {
            send_msg(self.mouse_combo, CB_SETCURSEL, mouse_sel as usize, 0);
        }

        // Trigger key combo: some names translated (e.g. "VK_CONVERT (変換)"), as the .NET version
        let kb_sel = send_msg(self.kb_combo, CB_GETCURSEL, 0, 0);
        send_msg(self.kb_combo, CB_RESETCONTENT, 0, 0);
        for (_, display) in KEYBOARD_KEYS {
            let w = to_wide(tr(lang, display));
            send_msg(self.kb_combo, CB_ADDSTRING, 0, w.as_ptr() as isize);
        }
        if kb_sel >= 0 {
            send_msg(self.kb_combo, CB_SETCURSEL, kb_sel as usize, 0);
        }
        self.refreshing = false;
    }

    fn refresh_all(&mut self) {
        self.refresh_with(true);
    }

    /// As refresh_all, without reading the list of the properties again (unless the current one
    /// is not in it), nor the excluded programs (their selection kept), nor dropping a number
    /// being typed: its text stays and it is stored later, as it would have been (the debounce
    /// timer goes on). Not in a box disabled now: its value is shown again (enable_number_row).
    fn refresh_values(&mut self) {
        self.refresh_with(false);
    }

    fn refresh_with(&mut self, read_lists: bool) {
        self.refreshing = true;
        self.disabled_labels.clear();
        let keep_pending = !read_lists;
        {
            let shared = self.shared.borrow();
            let cfg = shared.store.config();
            let lang = cfg.ui_language;

            // Profile Bar
            let listed = self
                .profile_names
                .iter()
                .any(|n| n == shared.store.selected_profile());
            if read_lists || !listed {
                self.profile_names.clear();
                self.profile_names.push(DEFAULT_PROFILE.to_owned());
                if let Some(dir) = shared.store.dir() {
                    for name in list_profiles(dir) {
                        self.profile_names.push(name);
                    }
                }
                send_msg(self.profile_combo, CB_RESETCONTENT, 0, 0);
                for (idx, name) in self.profile_names.iter().enumerate() {
                    let display = if idx == 0 {
                        tr(lang, "Default")
                    } else {
                        name.as_str()
                    };
                    let w = to_wide(display);
                    send_msg(self.profile_combo, CB_ADDSTRING, 0, w.as_ptr() as isize);
                }
            }
            let selected_idx = self
                .profile_names
                .iter()
                .position(|n| n == shared.store.selected_profile())
                .unwrap_or(0);
            send_msg(self.profile_combo, CB_SETCURSEL, selected_idx, 0);
            enable_window(
                self.profile_del_btn,
                shared.store.selected_profile() != DEFAULT_PROFILE,
            );
            enable_window(self.profile_open_btn, shared.store.dir().is_some());

            // Page 0: General
            let lang_idx = usize::from(cfg.ui_language == UiLanguage::Ja);
            send_msg(self.lang_combo, CB_SETCURSEL, lang_idx, 0);
            let pri_idx = match cfg.process_priority {
                ProcessPriority::High => 0,
                ProcessPriority::AboveNormal => 1,
                ProcessPriority::Normal => 2,
            };
            send_msg(self.priority_combo, CB_SETCURSEL, pri_idx, 0);
            if !self.admin_restart_btn.is_null() {
                enable_window(self.admin_restart_btn, !shared.restarting_admin);
            }
            set_check(self.admin_check, cfg.admin_check);

            // Page 1: Trigger
            let is_none_trigger = cfg.first_trigger.is_none();
            if !is_none_trigger {
                self.last_mouse_trigger = cfg.first_trigger;
            }
            set_check(self.mouse_chk, !is_none_trigger);
            enable_window(self.mouse_combo, !is_none_trigger);
            let mouse_idx = ACTIVE_MOUSE_TRIGGERS
                .iter()
                .position(|t| *t == self.last_mouse_trigger)
                .unwrap_or(7);
            send_msg(self.mouse_combo, CB_SETCURSEL, mouse_idx, 0);

            set_check(self.send_middle_chk, cfg.send_middle_click);
            enable_window(self.send_middle_chk, cfg.first_trigger.is_single());
            set_check(self.dragged_lock_chk, cfg.dragged_lock);
            enable_window(self.dragged_lock_chk, cfg.first_trigger.is_drag());

            let kb_active = cfg.keyboard_hook && cfg.target_vk_code != TriggerKey::None;
            set_check(self.kb_chk, kb_active);
            enable_window(self.kb_combo, kb_active);
            let kb_idx = KEYBOARD_KEYS
                .iter()
                .position(|(k, _)| *k == cfg.target_vk_code);
            match kb_idx {
                Some(idx) => {
                    send_msg(self.kb_combo, CB_SETCURSEL, idx, 0);
                }
                None => {
                    send_msg(self.kb_combo, CB_SETCURSEL, usize::MAX, 0);
                }
            }

            let dbl = cfg.first_trigger.is_double();
            enable_number_row(
                &mut self.disabled_labels,
                self.poll_timeout_lbl,
                &mut self.numbers,
                keep_pending,
                ID_POLL_TIMEOUT_EDIT,
                dbl,
                cfg.poll_timeout,
            );
            let lock_en = cfg.first_trigger.is_single() || dbl || kb_active;
            enable_number_row(
                &mut self.disabled_labels,
                self.scroll_locktime_lbl,
                &mut self.numbers,
                keep_pending,
                ID_SCROLL_LOCKTIME_EDIT,
                lock_en,
                cfg.scroll_locktime,
            );
            set_window_text(
                self.trigger_status_lbl,
                if is_none_trigger && !kb_active {
                    tr(lang, "Scrolling is disabled")
                } else {
                    " "
                },
            );

            // Page 2: Scroll
            let vha_active = cfg.horizontal_scroll && cfg.vh_adjuster_mode;
            set_check(self.cursor_change_chk, cfg.cursor_change);
            set_check(self.horizontal_chk, cfg.horizontal_scroll);
            set_check(self.reverse_chk, cfg.reverse_scroll);
            set_check(self.swap_chk, cfg.swap_scroll);
            enable_number_row(
                &mut self.disabled_labels,
                self.v_thresh_lbl,
                &mut self.numbers,
                keep_pending,
                ID_V_THRESH_EDIT,
                !vha_active,
                cfg.vertical_threshold,
            );
            enable_number_row(
                &mut self.disabled_labels,
                self.h_thresh_lbl,
                &mut self.numbers,
                keep_pending,
                ID_H_THRESH_EDIT,
                cfg.horizontal_scroll && !vha_active,
                cfg.horizontal_threshold,
            );

            // Page 3: Accel Table
            let accel_used = !cfg.real_wheel_mode;
            let custom_loaded = matches!(
                shared.store.custom_table(),
                w10wheel::config::CustomTableState::Loaded { .. }
            );
            let custom_used = cfg.custom_accel_table && custom_loaded;
            set_check(self.accel_enabled_chk, cfg.accel_table);
            enable_window(self.accel_enabled_chk, accel_used);
            let preset_en = accel_used && cfg.accel_table && !custom_used;
            for (preset, radio) in self.accel_radios {
                set_check(radio, cfg.accel_multiplier == preset);
                enable_window(radio, preset_en);
            }
            set_check(self.custom_accel_chk, cfg.custom_accel_table);
            enable_window(self.custom_accel_chk, accel_used && cfg.accel_table && custom_loaded);
            let (cw, ctext) = format_custom_table_status(lang, shared.store.custom_table());
            self.custom_status_is_warn = cw;
            set_window_text(self.custom_status_lbl, &ctext);

            // Page 4: Real Wheel Mode
            let rw = cfg.real_wheel_mode;
            set_check(self.real_wheel_chk, rw);
            enable_number_row(
                &mut self.disabled_labels,
                self.wheel_delta_lbl,
                &mut self.numbers,
                keep_pending,
                ID_WHEEL_DELTA_EDIT,
                rw,
                cfg.wheel_delta,
            );
            enable_number_row(
                &mut self.disabled_labels,
                self.v_wheel_move_lbl,
                &mut self.numbers,
                keep_pending,
                ID_V_WHEEL_MOVE_EDIT,
                rw,
                cfg.v_wheel_move,
            );
            enable_number_row(
                &mut self.disabled_labels,
                self.h_wheel_move_lbl,
                &mut self.numbers,
                keep_pending,
                ID_H_WHEEL_MOVE_EDIT,
                rw && cfg.horizontal_scroll,
                cfg.h_wheel_move,
            );
            set_check(self.quick_first_chk, cfg.quick_first);
            enable_window(self.quick_first_chk, rw);
            set_check(self.quick_turn_chk, cfg.quick_turn);
            enable_window(self.quick_turn_chk, rw);

            // Page 5: VH Adjuster
            set_check(self.vha_enabled_chk, cfg.vh_adjuster_mode);
            enable_window(self.vha_enabled_chk, cfg.horizontal_scroll);
            set_check(self.vha_fixed_radio, cfg.vh_adjuster_method == VhAdjusterMethod::Fixed);
            set_check(
                self.vha_switching_radio,
                cfg.vh_adjuster_method == VhAdjusterMethod::Switching,
            );
            enable_window(self.vha_fixed_radio, vha_active);
            enable_window(self.vha_switching_radio, vha_active);
            set_check(self.first_prefer_v_chk, cfg.first_prefer_vertical);
            enable_window(self.first_prefer_v_chk, vha_active);
            enable_number_row(
                &mut self.disabled_labels,
                self.first_min_lbl,
                &mut self.numbers,
                keep_pending,
                ID_FIRST_MIN_EDIT,
                vha_active,
                cfg.first_min_threshold,
            );
            enable_number_row(
                &mut self.disabled_labels,
                self.switching_lbl,
                &mut self.numbers,
                keep_pending,
                ID_SWITCHING_EDIT,
                vha_active && cfg.vh_adjuster_method == VhAdjusterMethod::Switching,
                cfg.switching_threshold,
            );

            // Page 6: Excluded Programs (not changed when nothing is loaded: the selection stays)
            if read_lists {
                send_msg(self.excl_list, LB_RESETCONTENT, 0, 0);
                for path in shared.store.exclusions().entries() {
                    let w = to_wide(path);
                    send_msg(self.excl_list, LB_ADDSTRING, 0, w.as_ptr() as isize);
                }
                enable_window(self.excl_remove_btn, false);
            }

            // Bottom status
            match shared.store.store_status() {
                Some(status) => {
                    self.bottom_status_is_warn = true;
                    set_window_text(self.bottom_status_lbl, tr(lang, store_status_key(status)));
                }
                None => {
                    self.bottom_status_is_warn = false;
                    set_window_text(
                        self.bottom_status_lbl,
                        tr(lang, "Changes are applied and saved at once"),
                    );
                }
            }
        }
        // The state of the custom table: a warning (with its icon) when it cannot be used, else
        // the explanation of the check box above it
        let custom_kind = if self.custom_status_is_warn {
            NoteKind::Warning
        } else {
            NoteKind::Plain
        };
        self.set_note_kind(self.custom_status_lbl, custom_kind);

        if !keep_pending {
            for nb in &mut self.numbers {
                nb.dirty = false;
            }
        }
        self.refreshing = false;
        self.relayout_dynamic_controls();
        if is_window_visible(self.hwnd) {
            redraw_window_all(self.hwnd, false);
        } else {
            invalidate(self.hwnd);
        }
    }

    fn ctrl_logical_height(&self, hwnd: Handle, fallback: i32) -> i32 {
        self.controls
            .iter()
            .find(|m| m.hwnd == hwnd)
            .map_or(fallback, |m| m.lh)
    }

    fn fit_combo_width<'a>(&self, font: Handle, items: impl IntoIterator<Item = &'a str>) -> i32 {
        let max_text = items
            .into_iter()
            .map(|s| measure_text_width(self.hwnd, font, s))
            .max()
            .unwrap_or(0);
        max_text + scale(38, self.dpi)
    }

    fn relayout_dynamic_controls(&mut self) {
        let nf = self.normal_font.as_ref().map_or(null_mut(), |f| f.raw());
        let dpi = self.dpi;
        let lang = self.shared.borrow().store.config().ui_language;
        let margin = scale(MARGIN, dpi);
        let right_edge = scale(CLIENT_W - MARGIN, dpi);

        // Top Profile Bar
        let prop_lbl_text = tr(lang, "Properties");
        let prop_lbl_w = measure_text_width(self.hwnd, nf, prop_lbl_text) + scale(6, dpi);
        move_window(
            self.profile_lbl,
            margin,
            scale(14, dpi),
            prop_lbl_w,
            scale(20, dpi),
        );

        let gap_sm = scale(6, dpi);
        let gap_md = scale(8, dpi);
        let top_y = scale(10, dpi);
        let top_btn_h = scale(28, dpi);

        let btn_specs = [
            (self.profile_add_btn, tr(lang, "Add")),
            (self.profile_del_btn, tr(lang, "Delete")),
            (self.profile_open_btn, tr(lang, "Open Dir")),
            (self.profile_reload_btn, tr(lang, "Reload")),
        ];
        let btn_widths: [i32; 4] = btn_specs.map(|(_, label)| {
            (measure_text_width(self.hwnd, nf, label) + scale(28, dpi)).max(scale(64, dpi))
        });
        let buttons_total = btn_widths.iter().sum::<i32>() + gap_sm * 3;
        let combo_x = margin + prop_lbl_w + gap_md;
        let max_profile_w = (right_edge - buttons_total - gap_md - combo_x).max(scale(90, dpi));

        let profile_content_w = self.fit_combo_width(
            nf,
            self.profile_names
                .iter()
                .enumerate()
                .map(|(i, n)| if i == 0 { tr(lang, "Default") } else { n.as_str() }),
        );
        let profile_box_w = profile_content_w.clamp(scale(96, dpi), max_profile_w.min(scale(180, dpi)));
        let profile_drop_h = scale(self.ctrl_logical_height(self.profile_combo, 220), dpi);
        move_window(
            self.profile_combo,
            combo_x,
            top_y,
            profile_box_w,
            profile_drop_h,
        );
        send_msg(
            self.profile_combo,
            CB_SETDROPPEDWIDTH,
            profile_content_w.max(profile_box_w) as usize,
            0,
        );

        let mut bx = combo_x + profile_box_w + gap_md;
        for ((btn, _), bw) in btn_specs.into_iter().zip(btn_widths) {
            move_window(btn, bx, top_y, bw, top_btn_h);
            bx += bw + gap_sm;
        }

        // The columns of the values of the pages (the combo boxes and the numbers line up)
        let cols = self.value_columns();
        self.layout_value_columns(&cols);

        // Page 0: Language, Priority, and Admin Restart Button
        let lang_content_w =
            self.fit_combo_width(nf, ["English", "Japanese"].into_iter().map(|k| tr(lang, k)));
        let lang_box_w = lang_content_w.clamp(scale(100, dpi), scale(280, dpi));
        move_window(
            self.lang_combo,
            scale(cols[0], dpi),
            scale(55 + PAGE_DY, dpi),
            lang_box_w,
            scale(self.ctrl_logical_height(self.lang_combo, 120), dpi),
        );
        send_msg(
            self.lang_combo,
            CB_SETDROPPEDWIDTH,
            lang_content_w.max(lang_box_w) as usize,
            0,
        );

        let pri_content_w = self.fit_combo_width(
            nf,
            ["High", "Above Normal", "Normal"]
                .into_iter()
                .map(|k| tr(lang, k)),
        );
        let pri_box_w = pri_content_w.clamp(scale(100, dpi), scale(280, dpi));
        move_window(
            self.priority_combo,
            scale(cols[0], dpi),
            scale(87 + PAGE_DY, dpi),
            pri_box_w,
            scale(self.ctrl_logical_height(self.priority_combo, 140), dpi),
        );
        send_msg(
            self.priority_combo,
            CB_SETDROPPEDWIDTH,
            pri_content_w.max(pri_box_w) as usize,
            0,
        );

        if !self.admin_restart_btn.is_null() {
            let restart_text = tr(lang, "Restart as administrator");
            let restart_w = (measure_text_width(self.hwnd, nf, restart_text) + scale(46, dpi))
                .clamp(scale(150, dpi), scale(300, dpi));
            move_window(
                self.admin_restart_btn,
                scale(192 + PAGE_DX, dpi),
                scale(156 + PAGE_DY, dpi),
                restart_w,
                scale(28, dpi),
            );
        }

        // Page 1: Mouse & Keyboard Trigger Combos
        // Wide enough for the names in both languages (the language can be switched without a
        // new layout)
        let mouse_content_w = self.fit_combo_width(
            nf,
            [UiLanguage::En, UiLanguage::Ja].into_iter().flat_map(|lang| {
                ACTIVE_MOUSE_TRIGGERS
                    .iter()
                    .map(move |&t| mouse_trigger_label(lang, t))
            }),
        );
        let mouse_box_w = mouse_content_w.clamp(scale(100, dpi), scale(PANEL_RIGHT - 12 - cols[1], dpi));
        move_window(
            self.mouse_combo,
            scale(cols[1], dpi),
            scale(55 + PAGE_DY, dpi),
            mouse_box_w,
            scale(self.ctrl_logical_height(self.mouse_combo, 240), dpi),
        );
        send_msg(
            self.mouse_combo,
            CB_SETDROPPEDWIDTH,
            mouse_content_w.max(mouse_box_w) as usize,
            0,
        );

        // Wide enough for the names in both languages (as the mouse triggers)
        let kb_content_w = self.fit_combo_width(
            nf,
            [UiLanguage::En, UiLanguage::Ja]
                .into_iter()
                .flat_map(|lang| KEYBOARD_KEYS.iter().map(move |&(_, disp)| tr(lang, disp))),
        );
        let kb_box_w = kb_content_w.clamp(scale(120, dpi), scale(PANEL_RIGHT - 12 - cols[1], dpi));
        move_window(
            self.kb_combo,
            scale(cols[1], dpi),
            scale(142 + PAGE_DY, dpi),
            kb_box_w,
            scale(self.ctrl_logical_height(self.kb_combo, 240), dpi),
        );
        send_msg(
            self.kb_combo,
            CB_SETDROPPEDWIDTH,
            kb_content_w.max(kb_box_w) as usize,
            0,
        );

        // Page 6: Add... / Remove buttons
        let excl_add_w = (measure_text_width(self.hwnd, nf, tr(lang, "Add...")) + scale(32, dpi))
            .max(scale(80, dpi));
        let excl_rem_w = (measure_text_width(self.hwnd, nf, tr(lang, "Remove")) + scale(32, dpi))
            .max(scale(80, dpi));
        move_window(
            self.excl_add_btn,
            scale(192 + PAGE_DX, dpi),
            scale(266 + PAGE_DY, dpi),
            excl_add_w,
            scale(28, dpi),
        );
        move_window(
            self.excl_remove_btn,
            scale(192 + PAGE_DX, dpi) + excl_add_w + gap_sm,
            scale(266 + PAGE_DY, dpi),
            excl_rem_w,
            scale(28, dpi),
        );

        // Bottom Bar
        let bot_y = scale(BOTTOM_Y, dpi);
        let bot_h = scale(BOTTOM_BTN_H, dpi);
        let reset_w = (measure_text_width(self.hwnd, nf, tr(lang, "Reset to defaults"))
            + scale(32, dpi))
        .max(scale(96, dpi));
        let close_w =
            (measure_text_width(self.hwnd, nf, tr(lang, "Close")) + scale(32, dpi))
                .max(scale(96, dpi));
        let close_x = right_edge - close_w;
        // The hint after the icon of a warning (when the changes cannot be saved)
        let icon_shift = if self.bottom_status_is_warn {
            scale(NOTE_ICON + NOTE_ICON_GAP, dpi)
        } else {
            0
        };
        let status_x = margin + reset_w + scale(14, dpi) + icon_shift;
        let status_w = (close_x - scale(10, dpi) - status_x).max(scale(80, dpi));

        move_window(self.reset_btn, margin, bot_y, reset_w, bot_h);
        move_window(
            self.bottom_status_lbl,
            status_x,
            bot_y + (bot_h - scale(20, dpi)) / 2 + scale(1, dpi),
            status_w,
            scale(20, dpi),
        );
        move_window(self.close_btn, close_x, bot_y, close_w, bot_h);

        self.layout_notes();
    }

    /// The x of the column of the values (the combo boxes and the boxes of the numbers) of each
    /// page, at 96 DPI: after the widest label or check box before a value on the page, as a
    /// column of a table (the .NET and Java versions), so that the values start at the same x
    fn value_columns(&self) -> [i32; 7] {
        let nf = self.normal_font.as_ref().map_or(null_mut(), |f| f.raw());
        let dpi = self.dpi as i32;
        let logical = |hwnd: Handle| {
            let px = measure_text_width(self.hwnd, nf, &get_window_text(hwnd));
            (px * 96 + dpi - 1) / dpi
        };
        // The box, its gap and the text of a check box (as drawn by the theme / the custom draw)
        const CHECK_EXTRA: i32 = 24;
        let mut widths = [0i32; 7];
        let mut widen = |page: u8, w: i32| {
            let slot = &mut widths[page as usize];
            *slot = (*slot).max(w);
        };
        widen(0, logical(self.lang_lbl));
        widen(0, logical(self.priority_lbl));
        widen(1, logical(self.mouse_chk) + CHECK_EXTRA);
        widen(1, logical(self.kb_chk) + CHECK_EXTRA);
        for nb in &self.numbers {
            widen(nb.page, logical(nb.label_hwnd));
        }
        widths.map(|w| VALUE_LEFT + w.max(VALUE_MIN_LABEL_W) + VALUE_GAP)
    }

    /// The labels, the check boxes and the boxes of the numbers in the columns of their pages
    fn layout_value_columns(&mut self, cols: &[i32; 7]) {
        let dpi = self.dpi;
        // The check boxes before a combo box: up to the column (their tooltips cover their texts)
        for chk in [self.mouse_chk, self.kb_chk] {
            if let Some(r) = client_rect_of(self.hwnd, chk) {
                let x = scale(VALUE_LEFT, dpi);
                move_window(chk, x, r.top, scale(cols[1] - VALUE_GAP / 2, dpi) - x, r.bottom - r.top);
            }
        }
        for lbl in [self.lang_lbl, self.priority_lbl] {
            if let Some(r) = client_rect_of(self.hwnd, lbl) {
                let x = scale(VALUE_LEFT, dpi);
                move_window(lbl, x, r.top, scale(cols[0] - VALUE_GAP / 2, dpi) - x, r.bottom - r.top);
            }
        }

        for nb in &self.numbers {
            let col = cols[nb.page as usize];
            let ly = nb.row_ly;
            move_window(
                nb.label_hwnd,
                scale(VALUE_LEFT, dpi),
                scale(ly + 4, dpi),
                scale(col - VALUE_GAP / 2 - VALUE_LEFT, dpi),
                scale(20, dpi),
            );
            move_window(
                nb.edit_hwnd,
                scale(col + 6, dpi),
                scale(ly + 4, dpi),
                scale(NUMBER_BOX_W - 6 - NUMBER_SPIN_W - 4, dpi),
                scale(18, dpi),
            );
            move_window(
                nb.updown_hwnd,
                scale(col + NUMBER_BOX_W - NUMBER_SPIN_W - 2, dpi),
                scale(ly + 2, dpi),
                scale(NUMBER_SPIN_W, dpi),
                scale(NUMBER_BOX_H - 4, dpi),
            );
            let range_x = col + NUMBER_BOX_W + 8;
            move_window(
                nb.range_hwnd,
                scale(range_x, dpi),
                scale(ly + 4, dpi),
                scale(PANEL_RIGHT - 4 - range_x, dpi),
                scale(20, dpi),
            );
        }
        for f in &mut self.frames {
            if let Some(nb) = self.numbers.iter().find(|n| n.edit_hwnd == f.control) {
                f.lx = cols[nb.page as usize];
                f.ly = nb.row_ly;
            }
        }
    }

    fn handle_command(&mut self, id: u16, code: u16) {
        if let Some(idx) = self.numbers.iter().position(|n| n.edit_id == id) {
            match code {
                EN_CHANGE => {
                    self.numbers[idx].dirty = true;
                    set_timer(self.hwnd, NUMERIC_DEBOUNCE_TIMER, NUMERIC_DEBOUNCE_MS);
                }
                EN_SETFOCUS => {
                    // The border of the box in the accent color
                    self.invalidate_frame_of(self.numbers[idx].edit_hwnd);
                }
                EN_KILLFOCUS => {
                    self.invalidate_frame_of(self.numbers[idx].edit_hwnd);
                    self.commit_number_index(idx, true);
                }
                _ => {}
            }
            return;
        }

        match id {
            ID_NAV_LIST if code == LBN_SELCHANGE => {
                let idx = send_msg(self.nav_list, LB_GETCURSEL, 0, 0);
                if (0..7).contains(&idx) {
                    self.commit_all_numbers();
                    self.show_page(idx as u8);
                }
            }
            ID_CLOSE_BTN | 2 /* IDCANCEL */ if code == BN_CLICKED => {
                self.commit_all_numbers();
                show_window(self.hwnd, SW_HIDE);
            }
            ID_PROFILE_COMBO if code == CBN_SELCHANGE => self.on_switch_profile(),
            ID_PROFILE_ADD if code == BN_CLICKED => self.on_add_profile(),
            ID_PROFILE_DELETE if code == BN_CLICKED => self.on_delete_profile(),
            ID_PROFILE_OPEN_DIR if code == BN_CLICKED => self.on_open_dir(),
            ID_PROFILE_RELOAD if code == BN_CLICKED => self.on_reload_button(),
            ID_RESET_BTN if code == BN_CLICKED => self.on_reset_defaults(),

            ID_LANG_COMBO if code == CBN_SELCHANGE => {
                let idx = send_msg(self.lang_combo, CB_GETCURSEL, 0, 0);
                let lang = if idx == 1 {
                    UiLanguage::Ja
                } else {
                    UiLanguage::En
                };
                self.mutate_config(|cfg| cfg.ui_language = lang, true);
            }
            ID_PRIORITY_COMBO if code == CBN_SELCHANGE => {
                let idx = send_msg(self.priority_combo, CB_GETCURSEL, 0, 0);
                let pri = match idx {
                    0 => ProcessPriority::High,
                    2 => ProcessPriority::Normal,
                    _ => ProcessPriority::AboveNormal,
                };
                self.mutate_config(
                    |cfg| {
                        cfg.process_priority = pri;
                        let _ = set_process_priority(pri);
                    },
                    false,
                );
            }
            ID_ADMIN_RESTART_BTN if code == BN_CLICKED => self.on_restart_as_admin(),
            ID_ADMIN_CHECK if code == BN_CLICKED => {
                let v = get_check(self.admin_check);
                self.mutate_config(|cfg| cfg.admin_check = v, false);
            }

            ID_MOUSE_CHK if code == BN_CLICKED => {
                let checked = get_check(self.mouse_chk);
                let next = if checked {
                    self.last_mouse_trigger
                } else {
                    Trigger::None
                };
                self.mutate_config(|cfg| cfg.first_trigger = next, false);
            }
            ID_MOUSE_COMBO if code == CBN_SELCHANGE => {
                let idx = send_msg(self.mouse_combo, CB_GETCURSEL, 0, 0);
                if let Some(&trigger) = ACTIVE_MOUSE_TRIGGERS.get(idx as usize) {
                    self.last_mouse_trigger = trigger;
                    self.mutate_config(|cfg| cfg.first_trigger = trigger, false);
                }
            }
            ID_SEND_MIDDLE_CHK if code == BN_CLICKED => {
                let v = get_check(self.send_middle_chk);
                self.mutate_config(|cfg| cfg.send_middle_click = v, false);
            }
            ID_DRAGGED_LOCK_CHK if code == BN_CLICKED => {
                let v = get_check(self.dragged_lock_chk);
                self.mutate_config(|cfg| cfg.dragged_lock = v, false);
            }
            ID_KB_CHK if code == BN_CLICKED => {
                let v = get_check(self.kb_chk);
                self.mutate_config(
                    |cfg| {
                        cfg.keyboard_hook = v;
                        if v && cfg.target_vk_code == TriggerKey::None {
                            cfg.target_vk_code = TriggerKey::VkNonconvert;
                        }
                    },
                    false,
                );
            }
            ID_KB_COMBO if code == CBN_SELCHANGE => {
                let idx = send_msg(self.kb_combo, CB_GETCURSEL, 0, 0);
                if let Some(&(vk, _)) = KEYBOARD_KEYS.get(idx as usize) {
                    self.mutate_config(|cfg| cfg.target_vk_code = vk, false);
                }
            }

            ID_CURSOR_CHANGE_CHK if code == BN_CLICKED => {
                let v = get_check(self.cursor_change_chk);
                self.mutate_config(|cfg| cfg.cursor_change = v, false);
            }
            ID_HORIZONTAL_CHK if code == BN_CLICKED => {
                let v = get_check(self.horizontal_chk);
                self.mutate_config(|cfg| cfg.horizontal_scroll = v, false);
            }
            ID_REVERSE_CHK if code == BN_CLICKED => {
                let v = get_check(self.reverse_chk);
                self.mutate_config(|cfg| cfg.reverse_scroll = v, false);
            }
            ID_SWAP_CHK if code == BN_CLICKED => {
                let v = get_check(self.swap_chk);
                self.mutate_config(|cfg| cfg.swap_scroll = v, false);
            }

            ID_ACCEL_ENABLED_CHK if code == BN_CLICKED => {
                let v = get_check(self.accel_enabled_chk);
                self.mutate_config(|cfg| cfg.accel_table = v, false);
            }
            ID_ACCEL_M5..=ID_ACCEL_M9 if code == BN_CLICKED => {
                if let Some((preset, _, _)) = ACCEL_PRESETS.iter().find(|(_, rid, _)| *rid == id) {
                    let p = *preset;
                    self.mutate_config(|cfg| cfg.accel_multiplier = p, false);
                }
            }
            ID_CUSTOM_ACCEL_CHK if code == BN_CLICKED => {
                let v = get_check(self.custom_accel_chk);
                self.mutate_config(|cfg| cfg.custom_accel_table = v, false);
            }

            ID_REAL_WHEEL_CHK if code == BN_CLICKED => {
                let v = get_check(self.real_wheel_chk);
                self.mutate_config(|cfg| cfg.real_wheel_mode = v, false);
            }
            ID_QUICK_FIRST_CHK if code == BN_CLICKED => {
                let v = get_check(self.quick_first_chk);
                self.mutate_config(|cfg| cfg.quick_first = v, false);
            }
            ID_QUICK_TURN_CHK if code == BN_CLICKED => {
                let v = get_check(self.quick_turn_chk);
                self.mutate_config(|cfg| cfg.quick_turn = v, false);
            }

            ID_VHA_ENABLED_CHK if code == BN_CLICKED => {
                let v = get_check(self.vha_enabled_chk);
                self.mutate_config(|cfg| cfg.vh_adjuster_mode = v, false);
            }
            ID_VHA_FIXED_RADIO if code == BN_CLICKED => {
                self.mutate_config(|cfg| cfg.vh_adjuster_method = VhAdjusterMethod::Fixed, false);
            }
            ID_VHA_SWITCHING_RADIO if code == BN_CLICKED => {
                self.mutate_config(
                    |cfg| cfg.vh_adjuster_method = VhAdjusterMethod::Switching,
                    false,
                );
            }
            ID_FIRST_PREFER_V_CHK if code == BN_CLICKED => {
                let v = get_check(self.first_prefer_v_chk);
                self.mutate_config(|cfg| cfg.first_prefer_vertical = v, false);
            }

            ID_EXCL_LIST if code == LBN_SETFOCUS || code == LBN_KILLFOCUS => {
                self.invalidate_frame_of(self.excl_list);
            }
            ID_EXCL_LIST if code == LBN_SELCHANGE => {
                let sel_count = send_msg(self.excl_list, LB_GETSELCOUNT, 0, 0);
                enable_window(self.excl_remove_btn, sel_count > 0);
            }
            ID_EXCL_ADD if code == BN_CLICKED => self.on_add_exclusions(),
            ID_EXCL_REMOVE if code == BN_CLICKED => self.on_remove_exclusions(),
            _ => {}
        }
    }

    fn mutate_config(
        &mut self,
        edit: impl FnOnce(&mut w10wheel::config::Config),
        retext: bool,
    ) {
        let tray_host = {
            let mut s = self.shared.borrow_mut();
            edit(s.store.config_mut());
            let next_cfg = EngineConfig::from_store(&s.store);
            let _ = s.control.set_config(next_cfg);
            let _ = s.store.store();
            s.tray_host
        };
        if retext {
            self.retext_all();
        }
        self.refresh_all();
        notify_tray_sync(tray_host);
    }

    fn commit_valid_dirty_numbers(&mut self) {
        let mut changed = false;
        for idx in 0..self.numbers.len() {
            if self.numbers[idx].dirty && self.commit_number_index(idx, false) {
                changed = true;
            }
        }
        if changed {
            self.push_config_and_save();
            self.refresh_all();
        }
    }

    fn commit_all_numbers(&mut self) {
        kill_timer(self.hwnd, NUMERIC_DEBOUNCE_TIMER);
        let mut changed = false;
        for idx in 0..self.numbers.len() {
            if self.numbers[idx].dirty && self.commit_number_index(idx, false) {
                changed = true;
            }
        }
        if changed {
            self.push_config_and_save();
            self.refresh_all();
        }
    }

    fn drop_pending_numbers(&mut self) {
        kill_timer(self.hwnd, NUMERIC_DEBOUNCE_TIMER);
        for nb in &mut self.numbers {
            nb.dirty = false;
        }
    }

    fn commit_number_index(&mut self, idx: usize, standalone: bool) -> bool {
        let nb = &mut self.numbers[idx];
        let text = get_window_text(nb.edit_hwnd);
        let Ok(parsed) = text.trim().parse::<i32>() else {
            if standalone {
                nb.dirty = false;
                self.refresh_all();
            }
            return false;
        };
        if !standalone && !(nb.low..=nb.high).contains(&parsed) {
            return false;
        }
        let clamped = parsed.clamp(nb.low, nb.high);
        nb.dirty = false;
        let edit_id = nb.edit_id;

        let changed = {
            let mut s = self.shared.borrow_mut();
            let cfg = s.store.config_mut();
            let target = match edit_id {
                ID_POLL_TIMEOUT_EDIT => &mut cfg.poll_timeout,
                ID_SCROLL_LOCKTIME_EDIT => &mut cfg.scroll_locktime,
                ID_V_THRESH_EDIT => &mut cfg.vertical_threshold,
                ID_H_THRESH_EDIT => &mut cfg.horizontal_threshold,
                ID_WHEEL_DELTA_EDIT => &mut cfg.wheel_delta,
                ID_V_WHEEL_MOVE_EDIT => &mut cfg.v_wheel_move,
                ID_H_WHEEL_MOVE_EDIT => &mut cfg.h_wheel_move,
                ID_FIRST_MIN_EDIT => &mut cfg.first_min_threshold,
                ID_SWITCHING_EDIT => &mut cfg.switching_threshold,
                _ => return false,
            };
            if *target == clamped {
                false
            } else {
                *target = clamped;
                true
            }
        };
        if standalone {
            if changed {
                self.push_config_and_save();
            }
            self.refresh_all();
        }
        changed
    }

    fn push_config_and_save(&self) {
        let mut s = self.shared.borrow_mut();
        let next_cfg = EngineConfig::from_store(&s.store);
        let _ = s.control.set_config(next_cfg);
        let _ = s.store.store();
    }

    /// Before leaving the selected properties (switching, restarting as administrator), after they
    /// have been stored: true if nothing is left unsaved, else whether to go on and lose it, asked
    /// with why it could not be saved (the error on a line of its own, then what is lost), as .NET
    /// does (Ctx.confirmUnstoredLoss)
    fn confirm_unsaved_loss(
        &self,
        lang: UiLanguage,
        dark: bool,
        loss: &'static str,
        action: &'static str,
    ) -> bool {
        let (unsaved, why) = {
            let s = self.shared.borrow();
            (
                s.store.has_unsaved_changes(),
                s.store.unsaved_reason().map(str::to_owned),
            )
        };
        if !unsaved {
            return true;
        }
        let text = match why {
            Some(why) => format!("{why}\n{}", tr(lang, loss)),
            None => tr(lang, loss).to_owned(),
        };
        confirm_warning(
            self.hwnd,
            lang,
            dark,
            "W10Wheel",
            tr(lang, "The changes could not be saved"),
            &text,
            tr(lang, action),
        )
    }

    fn on_switch_profile(&mut self) {
        self.commit_all_numbers();
        let sel = send_msg(self.profile_combo, CB_GETCURSEL, 0, 0);
        let Some(target) = self.profile_names.get(sel as usize).cloned() else {
            return;
        };
        let (same, lang, dark, tray_host) = {
            let mut s = self.shared.borrow_mut();
            let same = target == s.store.selected_profile();
            let _ = s.store.store();
            (same, s.store.config().ui_language, s.dark, s.tray_host)
        };
        if same {
            return;
        }
        if !self.confirm_unsaved_loss(
            lang,
            dark,
            "Switching loses the changes that could not be saved.",
            "Switch",
        ) {
            self.refresh_all();
            return;
        }

        let res = {
            let mut s = self.shared.borrow_mut();
            let r = s.store.switch_profile(&target);
            if r.is_ok() {
                s.apply_load();
            }
            r
        };
        if let Err(err) = res {
            let heading = format!("{}: {}", tr(lang, "The properties file could not be read"), target);
            show_error_dialog(self.hwnd, lang, dark, "W10Wheel", &heading, &err.to_string());
        }
        self.retext_all();
        self.refresh_all();
        notify_tray_sync(tray_host);
    }

    fn on_add_profile(&mut self) {
        self.commit_all_numbers();
        let (dir_opt, lang, dark, tray_host) = {
            let s = self.shared.borrow();
            (
                s.store.dir().map(std::path::Path::to_path_buf),
                s.store.config().ui_language,
                s.dark,
                s.tray_host,
            )
        };
        let Some(dir) = dir_opt else {
            return;
        };
        let Some(name) = ask_profile_name(self.hwnd, lang, dark, &dir) else {
            return;
        };
        let res = {
            let mut s = self.shared.borrow_mut();
            let r = s.store.add_profile(&name);
            if r.is_ok() {
                s.load_engine_config();
            }
            r
        };
        if let Err(err) = res {
            show_error_dialog(
                self.hwnd,
                lang,
                dark,
                "W10Wheel",
                tr(lang, "Add Properties"),
                &err.to_string(),
            );
        }
        self.refresh_all();
        notify_tray_sync(tray_host);
    }

    fn on_delete_profile(&mut self) {
        let (selected, lang, dark, tray_host) = {
            let s = self.shared.borrow();
            (
                s.store.selected_profile().to_owned(),
                s.store.config().ui_language,
                s.dark,
                s.tray_host,
            )
        };
        if selected == DEFAULT_PROFILE {
            return;
        }
        // As the .NET version: the question, then which properties
        let detail = format!("{}: {selected}", tr(lang, "Properties"));
        if !confirm_warning(
            self.hwnd,
            lang,
            dark,
            "W10Wheel",
            tr(lang, "Delete these properties?"),
            &detail,
            tr(lang, "Delete"),
        ) {
            return;
        }
        let res = {
            let mut s = self.shared.borrow_mut();
            let r = s.store.delete_profile(&selected);
            if r.is_ok() {
                s.apply_load();
            }
            r
        };
        if let Err(err) = res {
            show_error_dialog(
                self.hwnd,
                lang,
                dark,
                "W10Wheel",
                tr(lang, "Delete Properties"),
                &err.to_string(),
            );
        }
        self.retext_all();
        self.refresh_all();
        notify_tray_sync(tray_host);
    }

    fn on_open_dir(&self) {
        if let Some(dir) = self.shared.borrow().store.dir() {
            let _ = open_shell_target(&dir.to_string_lossy());
        }
    }

    fn on_reload_button(&mut self) {
        self.drop_pending_numbers();
        let (res, name, lang, dark, tray_host) = {
            let mut s = self.shared.borrow_mut();
            let name = s.store.selected_profile().to_owned();
            let res = s.store.reload();
            if res.is_ok() {
                s.apply_load();
            }
            (
                res,
                name,
                s.store.config().ui_language,
                s.dark,
                s.tray_host,
            )
        };
        if let Err(err) = res {
            let heading = format!("{}: {}", tr(lang, "The properties file could not be read"), name);
            show_error_dialog(self.hwnd, lang, dark, "W10Wheel", &heading, &err.to_string());
        }
        self.retext_all();
        self.refresh_all();
        notify_tray_sync(tray_host);
    }

    fn on_reset_defaults(&mut self) {
        let (lang, dark, selected) = {
            let s = self.shared.borrow();
            (s.store.config().ui_language, s.dark, s.store.selected_profile().to_owned())
        };
        // As the .NET version (Ctx.resetSettings): that the language is kept, and which
        // properties (only Default is translated)
        let shown = if selected == DEFAULT_PROFILE {
            tr(lang, "Default").to_owned()
        } else {
            selected
        };
        let detail = format!(
            "{}\n{}: {shown}",
            tr(lang, "The language is kept."),
            tr(lang, "Properties")
        );
        if !confirm_warning(
            self.hwnd,
            lang,
            dark,
            "W10Wheel",
            tr(lang, "Reset all the settings to their defaults?"),
            &detail,
            tr(lang, "Reset to defaults"),
        ) {
            return;
        }
        self.drop_pending_numbers();
        let tray_host = {
            let mut s = self.shared.borrow_mut();
            let res = s.store.reset_to_defaults();
            s.apply_load();
            if let Err(err) = res {
                let l = s.store.config().ui_language;
                let d = s.dark;
                let th = s.tray_host;
                drop(s);
                show_error_dialog(
                    self.hwnd,
                    l,
                    d,
                    "W10Wheel",
                    tr(l, "The changes could not be saved"),
                    &err.to_string(),
                );
                th
            } else {
                s.tray_host
            }
        };
        self.retext_all();
        self.refresh_all();
        notify_tray_sync(tray_host);
    }

    fn on_restart_as_admin(&mut self) {
        self.commit_all_numbers();
        let (lang, dark, selected, tray_host, err_slot) = {
            let mut s = self.shared.borrow_mut();
            if s.restarting_admin {
                return;
            }
            let _ = s.store.store();
            (
                s.store.config().ui_language,
                s.dark,
                s.store.selected_profile().to_owned(),
                s.tray_host,
                Arc::clone(&s.admin_error),
            )
        };
        if !self.confirm_unsaved_loss(
            lang,
            dark,
            "Restarting loses the changes that could not be saved.",
            "Restart as administrator",
        ) {
            return;
        }
        self.shared.borrow_mut().restarting_admin = true;
        self.refresh_all();

        let tray_ptr = tray_host as usize;
        std::thread::spawn(move || {
            let profile_arg = (selected != DEFAULT_PROFILE).then_some(selected.as_str());
            let wparam = match restart_as_admin(profile_arg, true) {
                AdminStartResult::Started => ADMIN_RESTART_STARTED,
                AdminStartResult::Canceled => ADMIN_RESTART_CANCELED,
                AdminStartResult::Failed(err) => {
                    *err_slot.lock().unwrap() = Some(err.to_string());
                    ADMIN_RESTART_FAILED
                }
            };
            let _ = w10wheel::post_window_message(tray_ptr, ADMIN_RESTART_MESSAGE, wparam, 0);
        });
    }

    fn on_add_exclusions(&mut self) {
        let (lang, dark) = {
            let s = self.shared.borrow();
            (s.store.config().ui_language, s.dark)
        };
        // The programs only, as the .NET version: "Programs (*.exe)" is the only filter
        let filter_label = format!("{} (*.exe)", tr(lang, "Programs"));
        let Some((valid, rejected)) = pick_excluded_programs(
            self.hwnd,
            tr(lang, "Select programs to exclude"),
            &filter_label,
        ) else {
            return;
        };
        if !rejected.is_empty() {
            let detail = format_non_exe_warning(lang, &rejected);
            show_warning_dialog(self.hwnd, lang, dark, "W10Wheel", tr(lang, "Excluded Programs"), &detail);
        }
        if valid.is_empty() {
            return;
        }
        {
            let mut s = self.shared.borrow_mut();
            let mut added_any = false;
            for path in valid {
                if s.store.exclusions_mut().add(&path) {
                    added_any = true;
                }
            }
            if added_any {
                let next = EngineConfig::from_store(&s.store);
                let _ = s.control.set_config(next);
                let _ = s.store.store();
            }
        }
        self.refresh_all();
    }

    fn on_remove_exclusions(&mut self) {
        let count = send_msg(self.excl_list, LB_GETCOUNT, 0, 0);
        let sel_count = send_msg(self.excl_list, LB_GETSELCOUNT, 0, 0);
        if count <= 0 || sel_count <= 0 {
            return;
        }
        let mut indices = vec![0i32; sel_count as usize];
        let got = send_msg(
            self.excl_list,
            LB_GETSELITEMS,
            indices.len(),
            indices.as_mut_ptr() as isize,
        );
        if got <= 0 {
            return;
        }
        indices.truncate(got as usize);
        {
            let mut s = self.shared.borrow_mut();
            let to_remove: Vec<String> = indices
                .iter()
                .filter_map(|&idx| s.store.exclusions().entries().get(idx as usize).cloned())
                .collect();
            let mut removed_any = false;
            for path in to_remove {
                if s.store.exclusions_mut().remove(&path) {
                    removed_any = true;
                }
            }
            if removed_any {
                let next = EngineConfig::from_store(&s.store);
                let _ = s.control.set_config(next);
                let _ = s.store.store();
            }
        }
        self.refresh_all();
    }
}

fn set_check(hwnd: Handle, checked: bool) {
    send_msg(
        hwnd,
        BM_SETCHECK,
        if checked { BST_CHECKED } else { BST_UNCHECKED },
        0,
    );
}

fn get_check(hwnd: Handle) -> bool {
    send_msg(hwnd, BM_GETCHECK, 0, 0) as usize == BST_CHECKED
}

fn enable_number_row(
    disabled_labels: &mut Vec<Handle>,
    lbl: Handle,
    numbers: &mut [NumberBinding],
    keep_pending: bool,
    edit_id: u16,
    enabled: bool,
    value: i32,
) {
    if !enabled && !lbl.is_null() {
        disabled_labels.push(lbl);
    }
    if let Some(nb) = numbers.iter_mut().find(|n| n.edit_id == edit_id) {
        if !enabled {
            disabled_labels.push(nb.range_hwnd);
        }
        enable_window(nb.edit_hwnd, enabled);
        enable_window(nb.updown_hwnd, enabled);
        // A number being typed (not stored yet) is kept (see refresh_values), unless its box is
        // disabled: its text could not be corrected any more (its focus lost while refreshing,
        // EN_KILLFOCUS not handled), and one out of range would never be stored nor dropped
        if keep_pending && enabled && nb.dirty {
            return;
        }
        nb.dirty = false;
        set_window_text(nb.edit_hwnd, &value.to_string());
        send_msg(nb.updown_hwnd, UDM_SETPOS32, 0, value as isize);
    }
}

fn notify_tray_sync(tray_host: Handle) {
    if !tray_host.is_null() {
        let _ = w10wheel::post_window_message(tray_host as usize, TRAY_SYNC_MESSAGE, 0, 0);
    }
}
