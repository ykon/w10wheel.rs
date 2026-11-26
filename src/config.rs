use std::collections::BTreeMap;
use std::ops::RangeInclusive;

/// Property keys shared with Java (Scala) and .NET (F#) W10Wheel v2.10.0.
pub mod keys {
    // String settings
    pub const FIRST_TRIGGER: &str = "firstTrigger";
    pub const PROCESS_PRIORITY: &str = "processPriority";
    pub const ACCEL_MULTIPLIER: &str = "accelMultiplier";
    pub const TARGET_VK_CODE: &str = "targetVKCode";
    pub const VH_ADJUSTER_METHOD: &str = "vhAdjusterMethod";
    pub const UI_LANGUAGE: &str = "uiLanguage";

    // Boolean settings (persisted)
    pub const REAL_WHEEL_MODE: &str = "realWheelMode";
    pub const CURSOR_CHANGE: &str = "cursorChange";
    pub const HORIZONTAL_SCROLL: &str = "horizontalScroll";
    pub const REVERSE_SCROLL: &str = "reverseScroll";
    pub const QUICK_FIRST: &str = "quickFirst";
    pub const QUICK_TURN: &str = "quickTurn";
    pub const ACCEL_TABLE: &str = "accelTable";
    pub const CUSTOM_ACCEL_TABLE: &str = "customAccelTable";
    pub const DRAGGED_LOCK: &str = "draggedLock";
    pub const SWAP_SCROLL: &str = "swapScroll";
    pub const SEND_MIDDLE_CLICK: &str = "sendMiddleClick";
    pub const KEYBOARD_HOOK: &str = "keyboardHook";
    pub const VH_ADJUSTER_MODE: &str = "vhAdjusterMode";
    pub const FIRST_PREFER_VERTICAL: &str = "firstPreferVertical";
    pub const ADMIN_CHECK: &str = "adminCheck";

    // Integer settings
    pub const POLL_TIMEOUT: &str = "pollTimeout";
    pub const SCROLL_LOCKTIME: &str = "scrollLocktime";
    pub const VERTICAL_THRESHOLD: &str = "verticalThreshold";
    pub const HORIZONTAL_THRESHOLD: &str = "horizontalThreshold";
    pub const WHEEL_DELTA: &str = "wheelDelta";
    pub const V_WHEEL_MOVE: &str = "vWheelMove";
    pub const H_WHEEL_MOVE: &str = "hWheelMove";
    pub const FIRST_MIN_THRESHOLD: &str = "firstMinThreshold";
    pub const SWITCHING_THRESHOLD: &str = "switchingThreshold";

    // Read-only custom acceleration table arrays
    pub const CUSTOM_ACCEL_THRESHOLD: &str = "customAccelThreshold";
    pub const CUSTOM_ACCEL_MULTIPLIER: &str = "customAccelMultiplier";
}

pub const POLL_TIMEOUT_RANGE: RangeInclusive<i32> = 50..=500;
pub const SCROLL_LOCKTIME_RANGE: RangeInclusive<i32> = 150..=500;
pub const VERTICAL_THRESHOLD_RANGE: RangeInclusive<i32> = 0..=500;
pub const HORIZONTAL_THRESHOLD_RANGE: RangeInclusive<i32> = 0..=500;
pub const WHEEL_DELTA_RANGE: RangeInclusive<i32> = 10..=500;
pub const V_WHEEL_MOVE_RANGE: RangeInclusive<i32> = 10..=500;
pub const H_WHEEL_MOVE_RANGE: RangeInclusive<i32> = 10..=500;
pub const FIRST_MIN_THRESHOLD_RANGE: RangeInclusive<i32> = 1..=10;
pub const SWITCHING_THRESHOLD_RANGE: RangeInclusive<i32> = 10..=500;

/// Kensington MouseWorks default acceleration thresholds.
pub const DEFAULT_ACCEL_THRESHOLD: [i32; 12] = [1, 2, 3, 5, 7, 10, 14, 20, 30, 43, 63, 91];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trigger {
    Lr,
    Left,
    Right,
    Middle,
    X1,
    X2,
    LeftDrag,
    #[default]
    RightDrag,
    MiddleDrag,
    X1Drag,
    X2Drag,
    None,
}

impl Trigger {
    pub fn as_property_value(self) -> &'static str {
        match self {
            Self::Lr => "LRTrigger",
            Self::Left => "LeftTrigger",
            Self::Right => "RightTrigger",
            Self::Middle => "MiddleTrigger",
            Self::X1 => "X1Trigger",
            Self::X2 => "X2Trigger",
            Self::LeftDrag => "LeftDragTrigger",
            Self::RightDrag => "RightDragTrigger",
            Self::MiddleDrag => "MiddleDragTrigger",
            Self::X1Drag => "X1DragTrigger",
            Self::X2Drag => "X2DragTrigger",
            Self::None => "NoneTrigger",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "LR" | "LRTrigger" => Some(Self::Lr),
            "Left" | "LeftTrigger" => Some(Self::Left),
            "Right" | "RightTrigger" => Some(Self::Right),
            "Middle" | "MiddleTrigger" => Some(Self::Middle),
            "X1" | "X1Trigger" => Some(Self::X1),
            "X2" | "X2Trigger" => Some(Self::X2),
            "LeftDrag" | "LeftDragTrigger" => Some(Self::LeftDrag),
            "RightDrag" | "RightDragTrigger" => Some(Self::RightDrag),
            "MiddleDrag" | "MiddleDragTrigger" => Some(Self::MiddleDrag),
            "X1Drag" | "X1DragTrigger" => Some(Self::X1Drag),
            "X2Drag" | "X2DragTrigger" => Some(Self::X2Drag),
            "None" | "NoneTrigger" => Some(Self::None),
            _ => Option::None,
        }
    }

    pub fn is_single(self) -> bool {
        matches!(self, Self::Middle | Self::X1 | Self::X2)
    }

    pub fn is_double(self) -> bool {
        matches!(self, Self::Lr | Self::Left | Self::Right)
    }

    pub fn is_drag(self) -> bool {
        matches!(
            self,
            Self::LeftDrag | Self::RightDrag | Self::MiddleDrag | Self::X1Drag | Self::X2Drag
        )
    }

    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessPriority {
    High,
    #[default]
    AboveNormal,
    Normal,
}

impl ProcessPriority {
    pub fn as_property_value(self) -> &'static str {
        match self {
            Self::High => "High",
            Self::AboveNormal => "AboveNormal",
            Self::Normal => "Normal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "High" => Some(Self::High),
            "AboveNormal" | "Above Normal" => Some(Self::AboveNormal),
            "Normal" => Some(Self::Normal),
            _ => Option::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccelPreset {
    #[default]
    M5,
    M6,
    M7,
    M8,
    M9,
}

impl AccelPreset {
    const M5_TABLE: [f64; 12] = [1.0, 1.3, 1.7, 2.0, 2.4, 2.7, 3.1, 3.4, 3.8, 4.1, 4.5, 4.8];
    const M6_TABLE: [f64; 12] = [1.2, 1.6, 2.0, 2.4, 2.8, 3.3, 3.7, 4.1, 4.5, 4.9, 5.4, 5.8];
    const M7_TABLE: [f64; 12] = [1.4, 1.8, 2.3, 2.8, 3.3, 3.8, 4.3, 4.8, 5.3, 5.8, 6.3, 6.7];
    const M8_TABLE: [f64; 12] = [1.6, 2.1, 2.7, 3.2, 3.8, 4.4, 4.9, 5.5, 6.0, 6.6, 7.2, 7.7];
    const M9_TABLE: [f64; 12] = [1.8, 2.4, 3.0, 3.6, 4.3, 4.9, 5.5, 6.2, 6.8, 7.4, 8.1, 8.7];

    pub fn as_property_value(self) -> &'static str {
        match self {
            Self::M5 => "M5",
            Self::M6 => "M6",
            Self::M7 => "M7",
            Self::M8 => "M8",
            Self::M9 => "M9",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "M5" => Some(Self::M5),
            "M6" => Some(Self::M6),
            "M7" => Some(Self::M7),
            "M8" => Some(Self::M8),
            "M9" => Some(Self::M9),
            _ => Option::None,
        }
    }

    pub fn multipliers(self) -> &'static [f64; 12] {
        match self {
            Self::M5 => &Self::M5_TABLE,
            Self::M6 => &Self::M6_TABLE,
            Self::M7 => &Self::M7_TABLE,
            Self::M8 => &Self::M8_TABLE,
            Self::M9 => &Self::M9_TABLE,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TriggerKey {
    None,
    VkTab,
    VkPause,
    VkCapital,
    VkConvert,
    #[default]
    VkNonconvert,
    VkPrior,
    VkNext,
    VkEnd,
    VkHome,
    VkSnapshot,
    VkInsert,
    VkDelete,
    VkLwin,
    VkRwin,
    VkApps,
    VkNumlock,
    VkScroll,
    VkLshift,
    VkRshift,
    VkLcontrol,
    VkRcontrol,
    VkLmenu,
    VkRmenu,
}

impl TriggerKey {
    pub fn as_property_value(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::VkTab => "VK_TAB",
            Self::VkPause => "VK_PAUSE",
            Self::VkCapital => "VK_CAPITAL",
            Self::VkConvert => "VK_CONVERT",
            Self::VkNonconvert => "VK_NONCONVERT",
            Self::VkPrior => "VK_PRIOR",
            Self::VkNext => "VK_NEXT",
            Self::VkEnd => "VK_END",
            Self::VkHome => "VK_HOME",
            Self::VkSnapshot => "VK_SNAPSHOT",
            Self::VkInsert => "VK_INSERT",
            Self::VkDelete => "VK_DELETE",
            Self::VkLwin => "VK_LWIN",
            Self::VkRwin => "VK_RWIN",
            Self::VkApps => "VK_APPS",
            Self::VkNumlock => "VK_NUMLOCK",
            Self::VkScroll => "VK_SCROLL",
            Self::VkLshift => "VK_LSHIFT",
            Self::VkRshift => "VK_RSHIFT",
            Self::VkLcontrol => "VK_LCONTROL",
            Self::VkRcontrol => "VK_RCONTROL",
            Self::VkLmenu => "VK_LMENU",
            Self::VkRmenu => "VK_RMENU",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "None" => Some(Self::None),
            "VK_TAB" => Some(Self::VkTab),
            "VK_PAUSE" => Some(Self::VkPause),
            "VK_CAPITAL" => Some(Self::VkCapital),
            "VK_CONVERT" => Some(Self::VkConvert),
            "VK_NONCONVERT" => Some(Self::VkNonconvert),
            "VK_PRIOR" => Some(Self::VkPrior),
            "VK_NEXT" => Some(Self::VkNext),
            "VK_END" => Some(Self::VkEnd),
            "VK_HOME" => Some(Self::VkHome),
            "VK_SNAPSHOT" => Some(Self::VkSnapshot),
            "VK_INSERT" => Some(Self::VkInsert),
            "VK_DELETE" => Some(Self::VkDelete),
            "VK_LWIN" => Some(Self::VkLwin),
            "VK_RWIN" => Some(Self::VkRwin),
            "VK_APPS" => Some(Self::VkApps),
            "VK_NUMLOCK" => Some(Self::VkNumlock),
            "VK_SCROLL" => Some(Self::VkScroll),
            "VK_LSHIFT" => Some(Self::VkLshift),
            "VK_RSHIFT" => Some(Self::VkRshift),
            "VK_LCONTROL" => Some(Self::VkLcontrol),
            "VK_RCONTROL" => Some(Self::VkRcontrol),
            "VK_LMENU" => Some(Self::VkLmenu),
            "VK_RMENU" => Some(Self::VkRmenu),
            _ => Option::None,
        }
    }

    pub fn vk_code(self) -> u32 {
        match self {
            Self::None => 0x00,
            Self::VkTab => 0x09,
            Self::VkPause => 0x13,
            Self::VkCapital => 0x14,
            Self::VkConvert => 0x1C,
            Self::VkNonconvert => 0x1D,
            Self::VkPrior => 0x21,
            Self::VkNext => 0x22,
            Self::VkEnd => 0x23,
            Self::VkHome => 0x24,
            Self::VkSnapshot => 0x2C,
            Self::VkInsert => 0x2D,
            Self::VkDelete => 0x2E,
            Self::VkLwin => 0x5B,
            Self::VkRwin => 0x5C,
            Self::VkApps => 0x5D,
            Self::VkNumlock => 0x90,
            Self::VkScroll => 0x91,
            Self::VkLshift => 0xA0,
            Self::VkRshift => 0xA1,
            Self::VkLcontrol => 0xA2,
            Self::VkRcontrol => 0xA3,
            Self::VkLmenu => 0xA4,
            Self::VkRmenu => 0xA5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VhAdjusterMethod {
    Fixed,
    #[default]
    Switching,
}

impl VhAdjusterMethod {
    pub fn as_property_value(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed",
            Self::Switching => "Switching",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Fixed" => Some(Self::Fixed),
            "Switching" => Some(Self::Switching),
            _ => Option::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiLanguage {
    #[default]
    En,
    Ja,
}

impl UiLanguage {
    pub fn as_property_value(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ja => "ja",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "en" => Some(Self::En),
            "ja" => Some(Self::Ja),
            _ => Option::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomTableError {
    MissingPair,
    ParseError,
    LengthMismatch,
    InvalidThresholds,
    InvalidMultipliers,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum CustomTableState {
    #[default]
    NotWritten,
    Invalid(CustomTableError),
    Loaded {
        thresholds: Vec<i32>,
        multipliers: Vec<f64>,
    },
}

/// Complete persisted configuration matching Java and .NET W10Wheel v2.10.0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub first_trigger: Trigger,
    pub process_priority: ProcessPriority,
    pub accel_multiplier: AccelPreset,
    pub target_vk_code: TriggerKey,
    pub vh_adjuster_method: VhAdjusterMethod,
    pub ui_language: UiLanguage,

    pub real_wheel_mode: bool,
    pub cursor_change: bool,
    pub horizontal_scroll: bool,
    pub reverse_scroll: bool,
    pub quick_first: bool,
    pub quick_turn: bool,
    pub accel_table: bool,
    pub custom_accel_table: bool,
    pub dragged_lock: bool,
    pub swap_scroll: bool,
    pub send_middle_click: bool,
    pub keyboard_hook: bool,
    pub vh_adjuster_mode: bool,
    pub first_prefer_vertical: bool,
    pub admin_check: bool,

    pub poll_timeout: i32,
    pub scroll_locktime: i32,
    pub vertical_threshold: i32,
    pub horizontal_threshold: i32,
    pub wheel_delta: i32,
    pub v_wheel_move: i32,
    pub h_wheel_move: i32,
    pub first_min_threshold: i32,
    pub switching_threshold: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self::default_for_language(UiLanguage::En)
    }
}

impl Config {
    /// Defaults matching Java (`Context.scala`) and .NET (`Ctx.fs`) runtime defaults.
    pub fn default_for_language(system_lang: UiLanguage) -> Self {
        Self {
            first_trigger: Trigger::RightDrag,
            process_priority: ProcessPriority::AboveNormal,
            accel_multiplier: AccelPreset::M5,
            target_vk_code: TriggerKey::VkNonconvert,
            vh_adjuster_method: VhAdjusterMethod::Switching,
            ui_language: system_lang,

            real_wheel_mode: false,
            cursor_change: true,
            horizontal_scroll: true,
            reverse_scroll: false,
            quick_first: false,
            quick_turn: false,
            accel_table: true,
            custom_accel_table: false,
            dragged_lock: false,
            swap_scroll: false,
            send_middle_click: false,
            keyboard_hook: false,
            vh_adjuster_mode: false,
            first_prefer_vertical: true,
            admin_check: true,

            poll_timeout: 200,
            scroll_locktime: 200,
            vertical_threshold: 0,
            horizontal_threshold: 75,
            wheel_delta: 120,
            v_wheel_move: 60,
            h_wheel_move: 60,
            first_min_threshold: 5,
            switching_threshold: 50,
        }
    }

    /// Parses properties into `Config` and `CustomTableState`, falling back to defaults per key.
    pub fn from_property_map(
        map: &BTreeMap<String, String>,
        system_lang: UiLanguage,
    ) -> (Self, CustomTableState) {
        let mut config = Self::default_for_language(system_lang);

        if let Some(v) = map.get(keys::FIRST_TRIGGER).and_then(|s| Trigger::parse(s)) {
            config.first_trigger = v;
        }
        if let Some(v) = map
            .get(keys::PROCESS_PRIORITY)
            .and_then(|s| ProcessPriority::parse(s))
        {
            config.process_priority = v;
        }
        if let Some(v) = map
            .get(keys::ACCEL_MULTIPLIER)
            .and_then(|s| AccelPreset::parse(s))
        {
            config.accel_multiplier = v;
        }
        if let Some(v) = map
            .get(keys::TARGET_VK_CODE)
            .and_then(|s| TriggerKey::parse(s))
        {
            config.target_vk_code = v;
        }
        if let Some(v) = map
            .get(keys::VH_ADJUSTER_METHOD)
            .and_then(|s| VhAdjusterMethod::parse(s))
        {
            config.vh_adjuster_method = v;
        }
        if let Some(v) = map
            .get(keys::UI_LANGUAGE)
            .and_then(|s| UiLanguage::parse(s))
        {
            config.ui_language = v;
        }

        apply_bool(map, keys::REAL_WHEEL_MODE, &mut config.real_wheel_mode);
        apply_bool(map, keys::CURSOR_CHANGE, &mut config.cursor_change);
        apply_bool(map, keys::HORIZONTAL_SCROLL, &mut config.horizontal_scroll);
        apply_bool(map, keys::REVERSE_SCROLL, &mut config.reverse_scroll);
        apply_bool(map, keys::QUICK_FIRST, &mut config.quick_first);
        apply_bool(map, keys::QUICK_TURN, &mut config.quick_turn);
        apply_bool(map, keys::ACCEL_TABLE, &mut config.accel_table);
        apply_bool(map, keys::CUSTOM_ACCEL_TABLE, &mut config.custom_accel_table);
        apply_bool(map, keys::DRAGGED_LOCK, &mut config.dragged_lock);
        apply_bool(map, keys::SWAP_SCROLL, &mut config.swap_scroll);
        apply_bool(map, keys::SEND_MIDDLE_CLICK, &mut config.send_middle_click);
        apply_bool(map, keys::KEYBOARD_HOOK, &mut config.keyboard_hook);
        apply_bool(map, keys::VH_ADJUSTER_MODE, &mut config.vh_adjuster_mode);
        apply_bool(
            map,
            keys::FIRST_PREFER_VERTICAL,
            &mut config.first_prefer_vertical,
        );
        apply_bool(map, keys::ADMIN_CHECK, &mut config.admin_check);

        apply_int(
            map,
            keys::POLL_TIMEOUT,
            POLL_TIMEOUT_RANGE,
            &mut config.poll_timeout,
        );
        apply_int(
            map,
            keys::SCROLL_LOCKTIME,
            SCROLL_LOCKTIME_RANGE,
            &mut config.scroll_locktime,
        );
        apply_int(
            map,
            keys::VERTICAL_THRESHOLD,
            VERTICAL_THRESHOLD_RANGE,
            &mut config.vertical_threshold,
        );
        apply_int(
            map,
            keys::HORIZONTAL_THRESHOLD,
            HORIZONTAL_THRESHOLD_RANGE,
            &mut config.horizontal_threshold,
        );
        apply_int(
            map,
            keys::WHEEL_DELTA,
            WHEEL_DELTA_RANGE,
            &mut config.wheel_delta,
        );
        apply_int(
            map,
            keys::V_WHEEL_MOVE,
            V_WHEEL_MOVE_RANGE,
            &mut config.v_wheel_move,
        );
        apply_int(
            map,
            keys::H_WHEEL_MOVE,
            H_WHEEL_MOVE_RANGE,
            &mut config.h_wheel_move,
        );
        apply_int(
            map,
            keys::FIRST_MIN_THRESHOLD,
            FIRST_MIN_THRESHOLD_RANGE,
            &mut config.first_min_threshold,
        );
        apply_int(
            map,
            keys::SWITCHING_THRESHOLD,
            SWITCHING_THRESHOLD_RANGE,
            &mut config.switching_threshold,
        );

        let custom_table = parse_custom_table(map);
        (config, custom_table)
    }

    /// Serializes all 30 persisted settings into sorted property pairs.
    pub fn to_property_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert(
            keys::FIRST_TRIGGER.into(),
            self.first_trigger.as_property_value().into(),
        );
        map.insert(
            keys::PROCESS_PRIORITY.into(),
            self.process_priority.as_property_value().into(),
        );
        map.insert(
            keys::ACCEL_MULTIPLIER.into(),
            self.accel_multiplier.as_property_value().into(),
        );
        map.insert(
            keys::TARGET_VK_CODE.into(),
            self.target_vk_code.as_property_value().into(),
        );
        map.insert(
            keys::VH_ADJUSTER_METHOD.into(),
            self.vh_adjuster_method.as_property_value().into(),
        );
        map.insert(
            keys::UI_LANGUAGE.into(),
            self.ui_language.as_property_value().into(),
        );

        for (key, value) in [
            (keys::REAL_WHEEL_MODE, self.real_wheel_mode),
            (keys::CURSOR_CHANGE, self.cursor_change),
            (keys::HORIZONTAL_SCROLL, self.horizontal_scroll),
            (keys::REVERSE_SCROLL, self.reverse_scroll),
            (keys::QUICK_FIRST, self.quick_first),
            (keys::QUICK_TURN, self.quick_turn),
            (keys::ACCEL_TABLE, self.accel_table),
            (keys::CUSTOM_ACCEL_TABLE, self.custom_accel_table),
            (keys::DRAGGED_LOCK, self.dragged_lock),
            (keys::SWAP_SCROLL, self.swap_scroll),
            (keys::SEND_MIDDLE_CLICK, self.send_middle_click),
            (keys::KEYBOARD_HOOK, self.keyboard_hook),
            (keys::VH_ADJUSTER_MODE, self.vh_adjuster_mode),
            (keys::FIRST_PREFER_VERTICAL, self.first_prefer_vertical),
            (keys::ADMIN_CHECK, self.admin_check),
        ] {
            map.insert(key.into(), value.to_string());
        }

        for (key, value) in [
            (keys::POLL_TIMEOUT, self.poll_timeout),
            (keys::SCROLL_LOCKTIME, self.scroll_locktime),
            (keys::VERTICAL_THRESHOLD, self.vertical_threshold),
            (keys::HORIZONTAL_THRESHOLD, self.horizontal_threshold),
            (keys::WHEEL_DELTA, self.wheel_delta),
            (keys::V_WHEEL_MOVE, self.v_wheel_move),
            (keys::H_WHEEL_MOVE, self.h_wheel_move),
            (keys::FIRST_MIN_THRESHOLD, self.first_min_threshold),
            (keys::SWITCHING_THRESHOLD, self.switching_threshold),
        ] {
            map.insert(key.into(), value.to_string());
        }

        map
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        Option::None
    }
}

fn apply_bool(map: &BTreeMap<String, String>, key: &str, target: &mut bool) {
    if let Some(value) = map.get(key).and_then(|s| parse_bool(s)) {
        *target = value;
    }
}

fn apply_int(
    map: &BTreeMap<String, String>,
    key: &str,
    range: RangeInclusive<i32>,
    target: &mut i32,
) {
    if let Some(value) = map
        .get(key)
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|n| range.contains(n))
    {
        *target = value;
    }
}

fn parse_custom_table(map: &BTreeMap<String, String>) -> CustomTableState {
    let threshold_raw = map.get(keys::CUSTOM_ACCEL_THRESHOLD);
    let multiplier_raw = map.get(keys::CUSTOM_ACCEL_MULTIPLIER);
    match (threshold_raw, multiplier_raw) {
        (Option::None, Option::None) => CustomTableState::NotWritten,
        (Some(_), Option::None) | (Option::None, Some(_)) => {
            CustomTableState::Invalid(CustomTableError::MissingPair)
        }
        (Some(t_str), Some(m_str)) => {
            let Ok(thresholds) = parse_csv(t_str, |s| s.parse::<i32>().ok()) else {
                return CustomTableState::Invalid(CustomTableError::ParseError);
            };
            let Ok(multipliers) = parse_csv(m_str, |s| s.parse::<f64>().ok()) else {
                return CustomTableState::Invalid(CustomTableError::ParseError);
            };
            if thresholds.is_empty() && multipliers.is_empty() {
                return CustomTableState::NotWritten;
            }
            if thresholds.len() != multipliers.len() {
                return CustomTableState::Invalid(CustomTableError::LengthMismatch);
            }
            let valid_thresholds = thresholds.iter().all(|&n| n >= 0)
                && thresholds.windows(2).all(|pair| pair[0] < pair[1]);
            if !valid_thresholds {
                return CustomTableState::Invalid(CustomTableError::InvalidThresholds);
            }
            if !multipliers.iter().all(|&d| d.is_finite() && d >= 0.0) {
                return CustomTableState::Invalid(CustomTableError::InvalidMultipliers);
            }
            CustomTableState::Loaded {
                thresholds,
                multipliers,
            }
        }
    }
}

fn parse_csv<T>(raw: &str, parse_item: fn(&str) -> Option<T>) -> Result<Vec<T>, ()> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| parse_item(s).ok_or(()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_java_and_dotnet_runtime_values() {
        let cfg = Config::default_for_language(UiLanguage::Ja);
        assert_eq!(cfg.first_trigger, Trigger::RightDrag);
        assert_eq!(cfg.process_priority, ProcessPriority::AboveNormal);
        assert_eq!(cfg.accel_multiplier, AccelPreset::M5);
        assert_eq!(cfg.target_vk_code, TriggerKey::VkNonconvert);
        assert_eq!(cfg.vh_adjuster_method, VhAdjusterMethod::Switching);
        assert_eq!(cfg.ui_language, UiLanguage::Ja);

        assert!(!cfg.real_wheel_mode);
        assert!(cfg.cursor_change);
        assert!(cfg.horizontal_scroll);
        assert!(!cfg.reverse_scroll);
        assert!(!cfg.quick_first);
        assert!(!cfg.quick_turn);
        assert!(cfg.accel_table);
        assert!(!cfg.custom_accel_table);
        assert!(!cfg.dragged_lock);
        assert!(!cfg.swap_scroll);
        assert!(!cfg.send_middle_click);
        assert!(!cfg.keyboard_hook);
        assert!(!cfg.vh_adjuster_mode);
        assert!(cfg.first_prefer_vertical);
        assert!(cfg.admin_check);

        assert_eq!(cfg.poll_timeout, 200);
        assert_eq!(cfg.scroll_locktime, 200);
        assert_eq!(cfg.vertical_threshold, 0);
        assert_eq!(cfg.horizontal_threshold, 75);
        assert_eq!(cfg.wheel_delta, 120);
        assert_eq!(cfg.v_wheel_move, 60);
        assert_eq!(cfg.h_wheel_move, 60);
        assert_eq!(cfg.first_min_threshold, 5);
        assert_eq!(cfg.switching_threshold, 50);

        let serialized = cfg.to_property_map();
        assert_eq!(serialized.len(), 30);
        assert!(!serialized.contains_key("passMode"));
        assert!(!serialized.contains_key(keys::CUSTOM_ACCEL_THRESHOLD));
        assert!(!serialized.contains_key(keys::CUSTOM_ACCEL_MULTIPLIER));
        assert_eq!(
            serialized.get(keys::FIRST_TRIGGER).map(String::as_str),
            Some("RightDragTrigger")
        );
    }

    #[test]
    fn parses_short_and_full_triggers_priorities_and_keys() {
        for (short, full, expected) in [
            ("LR", "LRTrigger", Trigger::Lr),
            ("Left", "LeftTrigger", Trigger::Left),
            ("Right", "RightTrigger", Trigger::Right),
            ("Middle", "MiddleTrigger", Trigger::Middle),
            ("X1", "X1Trigger", Trigger::X1),
            ("X2", "X2Trigger", Trigger::X2),
            ("LeftDrag", "LeftDragTrigger", Trigger::LeftDrag),
            ("RightDrag", "RightDragTrigger", Trigger::RightDrag),
            ("MiddleDrag", "MiddleDragTrigger", Trigger::MiddleDrag),
            ("X1Drag", "X1DragTrigger", Trigger::X1Drag),
            ("X2Drag", "X2DragTrigger", Trigger::X2Drag),
            ("None", "NoneTrigger", Trigger::None),
        ] {
            assert_eq!(Trigger::parse(short), Some(expected));
            assert_eq!(Trigger::parse(full), Some(expected));
            assert_eq!(expected.as_property_value(), full);
        }
        assert_eq!(Trigger::parse("invalid"), Option::None);

        assert_eq!(
            ProcessPriority::parse("Above Normal"),
            Some(ProcessPriority::AboveNormal)
        );
        assert_eq!(
            ProcessPriority::parse("AboveNormal"),
            Some(ProcessPriority::AboveNormal)
        );
        assert_eq!(ProcessPriority::parse("High"), Some(ProcessPriority::High));
        assert_eq!(
            ProcessPriority::parse("Normal"),
            Some(ProcessPriority::Normal)
        );
        assert_eq!(ProcessPriority::parse("Low"), Option::None);

        assert_eq!(TriggerKey::VkNonconvert.vk_code(), 0x1D);
        assert_eq!(TriggerKey::parse("None"), Some(TriggerKey::None));
        assert_eq!(TriggerKey::parse("VK_RMENU"), Some(TriggerKey::VkRmenu));

        assert_eq!(DEFAULT_ACCEL_THRESHOLD.len(), 12);
        assert_eq!(AccelPreset::M5.multipliers()[0], 1.0);
        assert_eq!(AccelPreset::M9.multipliers()[11], 8.7);
    }

    #[test]
    fn from_property_map_enforces_ranges_and_case_insensitive_booleans() {
        let mut map = BTreeMap::new();
        map.insert(keys::FIRST_TRIGGER.into(), "LR".into());
        map.insert(keys::PROCESS_PRIORITY.into(), "Above Normal".into());
        map.insert(keys::ACCEL_MULTIPLIER.into(), "M8".into());
        map.insert(keys::TARGET_VK_CODE.into(), "VK_LWIN".into());
        map.insert(keys::VH_ADJUSTER_METHOD.into(), "Fixed".into());
        map.insert(keys::UI_LANGUAGE.into(), "ja".into());

        // .NET writes "True"/"False"
        map.insert(keys::ACCEL_TABLE.into(), "False".into());
        map.insert(keys::REVERSE_SCROLL.into(), "True".into());
        map.insert(keys::ADMIN_CHECK.into(), "FALSE".into());
        map.insert(keys::SWAP_SCROLL.into(), "yes".into()); // invalid -> default false

        // Boundary values & out-of-range fallback
        map.insert(keys::POLL_TIMEOUT.into(), "50".into());
        map.insert(keys::SCROLL_LOCKTIME.into(), "500".into());
        map.insert(keys::VERTICAL_THRESHOLD.into(), "500".into());
        map.insert(keys::HORIZONTAL_THRESHOLD.into(), "501".into()); // out of bounds -> 75
        map.insert(keys::WHEEL_DELTA.into(), "9".into()); // out of bounds -> 120
        map.insert(keys::V_WHEEL_MOVE.into(), "10".into());
        map.insert(keys::H_WHEEL_MOVE.into(), "500".into());
        map.insert(keys::FIRST_MIN_THRESHOLD.into(), "10".into());
        map.insert(keys::SWITCHING_THRESHOLD.into(), "9".into()); // out of bounds -> 50

        let (cfg, table) = Config::from_property_map(&map, UiLanguage::En);
        assert_eq!(cfg.first_trigger, Trigger::Lr);
        assert_eq!(cfg.process_priority, ProcessPriority::AboveNormal);
        assert_eq!(cfg.accel_multiplier, AccelPreset::M8);
        assert_eq!(cfg.target_vk_code, TriggerKey::VkLwin);
        assert_eq!(cfg.vh_adjuster_method, VhAdjusterMethod::Fixed);
        assert_eq!(cfg.ui_language, UiLanguage::Ja);

        assert!(!cfg.accel_table);
        assert!(cfg.reverse_scroll);
        assert!(!cfg.admin_check);
        assert!(!cfg.swap_scroll);

        assert_eq!(cfg.poll_timeout, 50);
        assert_eq!(cfg.scroll_locktime, 500);
        assert_eq!(cfg.vertical_threshold, 500);
        assert_eq!(cfg.horizontal_threshold, 75);
        assert_eq!(cfg.wheel_delta, 120);
        assert_eq!(cfg.v_wheel_move, 10);
        assert_eq!(cfg.h_wheel_move, 500);
        assert_eq!(cfg.first_min_threshold, 10);
        assert_eq!(cfg.switching_threshold, 50);
        assert_eq!(table, CustomTableState::NotWritten);
    }

    #[test]
    fn custom_accel_table_validation_covers_all_cases() {
        let parse = |t: Option<&str>, m: Option<&str>| {
            let mut map = BTreeMap::new();
            if let Some(v) = t {
                map.insert(keys::CUSTOM_ACCEL_THRESHOLD.into(), v.into());
            }
            if let Some(v) = m {
                map.insert(keys::CUSTOM_ACCEL_MULTIPLIER.into(), v.into());
            }
            Config::from_property_map(&map, UiLanguage::En).1
        };

        assert_eq!(parse(Option::None, Option::None), CustomTableState::NotWritten);
        assert_eq!(parse(Some(","), Some(", ")), CustomTableState::NotWritten);
        assert_eq!(
            parse(Some("1, 2"), Option::None),
            CustomTableState::Invalid(CustomTableError::MissingPair)
        );
        assert_eq!(
            parse(Option::None, Some("1.0")),
            CustomTableState::Invalid(CustomTableError::MissingPair)
        );
        assert_eq!(
            parse(Some("1, abc"), Some("1.0, 2.0")),
            CustomTableState::Invalid(CustomTableError::ParseError)
        );
        assert_eq!(
            parse(Some("1, 2, 3"), Some("1.0, 2.0")),
            CustomTableState::Invalid(CustomTableError::LengthMismatch)
        );
        assert_eq!(
            parse(Some("1, 2, 2"), Some("1.0, 2.0, 3.0")),
            CustomTableState::Invalid(CustomTableError::InvalidThresholds)
        );
        assert_eq!(
            parse(Some("-1, 2"), Some("1.0, 2.0")),
            CustomTableState::Invalid(CustomTableError::InvalidThresholds)
        );
        assert_eq!(
            parse(Some("1, 5"), Some("1.0, NaN")),
            CustomTableState::Invalid(CustomTableError::InvalidMultipliers)
        );
        assert_eq!(
            parse(Some("1, 5"), Some("1.0, Infinity")),
            CustomTableState::Invalid(CustomTableError::InvalidMultipliers)
        );
        assert_eq!(
            parse(Some("1, 5"), Some("1.0, -0.5")),
            CustomTableState::Invalid(CustomTableError::InvalidMultipliers)
        );
        assert_eq!(
            parse(Some("0, 5, 10"), Some("0.5, 1.5, 3")),
            CustomTableState::Loaded {
                thresholds: vec![0, 5, 10],
                multipliers: vec![0.5, 1.5, 3.0],
            }
        );
    }
}
