use crate::config::{
    AccelPreset, Config, CustomTableState, DEFAULT_ACCEL_THRESHOLD, VhAdjusterMethod,
};

const MAX_NOTCHES_PER_PACKET: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ScrollAxis {
    Vertical = 1,
    Horizontal = 2,
}

impl ScrollAxis {
    pub fn from_wparam(value: usize) -> Option<Self> {
        match value {
            1 => Some(Self::Vertical),
            2 => Some(Self::Horizontal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelEvent {
    Vertical(i32),
    Horizontal(i32),
}

/// Complete scroll engine configuration matching Java (`Windows.scala`) and .NET (`Windows.fs`).
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollConfig {
    pub real_wheel_mode: bool,
    pub cursor_change: bool,
    pub horizontal_scroll: bool,
    pub reverse_scroll: bool,
    pub swap_scroll: bool,
    pub vertical_threshold: i32,
    pub horizontal_threshold: i32,
    pub accel_table: bool,
    pub accel_multiplier: AccelPreset,
    pub custom_accel_table: bool,
    pub custom_table: CustomTableState,
    pub wheel_delta: i32,
    pub v_wheel_move: i32,
    pub h_wheel_move: i32,
    pub quick_first: bool,
    pub quick_turn: bool,
    pub vh_adjuster_mode: bool,
    pub vh_adjuster_method: VhAdjusterMethod,
    pub first_prefer_vertical: bool,
    pub first_min_threshold: i32,
    pub switching_threshold: i32,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self::from_config(&Config::default(), &CustomTableState::NotWritten)
    }
}

impl ScrollConfig {
    pub fn from_config(config: &Config, custom_table: &CustomTableState) -> Self {
        Self {
            real_wheel_mode: config.real_wheel_mode,
            cursor_change: config.cursor_change,
            horizontal_scroll: config.horizontal_scroll,
            reverse_scroll: config.reverse_scroll,
            swap_scroll: config.swap_scroll,
            vertical_threshold: config.vertical_threshold,
            horizontal_threshold: config.horizontal_threshold,
            accel_table: config.accel_table,
            accel_multiplier: config.accel_multiplier,
            custom_accel_table: config.custom_accel_table,
            custom_table: custom_table.clone(),
            wheel_delta: config.wheel_delta,
            v_wheel_move: config.v_wheel_move,
            h_wheel_move: config.h_wheel_move,
            quick_first: config.quick_first,
            quick_turn: config.quick_turn,
            vh_adjuster_mode: config.vh_adjuster_mode,
            vh_adjuster_method: config.vh_adjuster_method,
            first_prefer_vertical: config.first_prefer_vertical,
            first_min_threshold: config.first_min_threshold,
            switching_threshold: config.switching_threshold,
        }
    }

    pub fn is_vh_adjuster_active(&self) -> bool {
        self.horizontal_scroll && self.vh_adjuster_mode
    }

    pub fn active_accel_table(&self) -> (&[i32], &[f64]) {
        if self.custom_accel_table
            && let CustomTableState::Loaded {
                thresholds,
                multipliers,
            } = &self.custom_table
            && !thresholds.is_empty()
            && thresholds.len() == multipliers.len()
        {
            return (thresholds.as_slice(), multipliers.as_slice());
        }
        (&DEFAULT_ACCEL_THRESHOLD, self.accel_multiplier.multipliers())
    }

    fn is_using_custom_accel(&self) -> bool {
        self.custom_accel_table
            && matches!(
                &self.custom_table,
                CustomTableState::Loaded {
                    thresholds,
                    multipliers,
                } if !thresholds.is_empty() && thresholds.len() == multipliers.len()
            )
    }

    pub fn mode_summary(&self) -> String {
        let base = if self.real_wheel_mode {
            format!(
                "RealWheel (delta={}, V={}, H={})",
                self.wheel_delta, self.v_wheel_move, self.h_wheel_move
            )
        } else if self.accel_table {
            if self.is_using_custom_accel() {
                "Direct (加速: Custom)".to_owned()
            } else {
                format!("Direct (加速: {})", self.accel_multiplier.as_property_value())
            }
        } else {
            "Direct (加速なし)".to_owned()
        };

        if self.is_vh_adjuster_active() {
            format!(
                "{base} + VHAdjuster({})",
                self.vh_adjuster_method.as_property_value()
            )
        } else {
            base
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveDirection {
    Zero,
    Plus,
    Minus,
}

impl MoveDirection {
    fn is_turn(self, movement: i32) -> bool {
        match self {
            Self::Zero => false,
            Self::Plus => movement < 0,
            Self::Minus => movement > 0,
        }
    }
}

/// Stateful per-scroll session translating relative mouse motion into wheel events.
#[derive(Debug)]
pub struct ScrollSession {
    vw_count: i64,
    hw_count: i64,
    v_last_move: MoveDirection,
    h_last_move: MoveDirection,
    fixed_vhd: Option<ScrollAxis>,
    latest_vhd: Option<ScrollAxis>,
    cursor_axis: Option<ScrollAxis>,
}

impl ScrollSession {
    pub fn new(config: &ScrollConfig) -> Self {
        Self {
            vw_count: initial_wheel_count(config.v_wheel_move, config.quick_first),
            hw_count: initial_wheel_count(config.h_wheel_move, config.quick_first),
            v_last_move: MoveDirection::Zero,
            h_last_move: MoveDirection::Zero,
            fixed_vhd: None,
            latest_vhd: None,
            cursor_axis: None,
        }
    }

    /// Scroll mode started by a trigger other than drag (or the keyboard): the vertical cursor at
    /// once, also with the VH Adjuster (it changes the cursor once a direction is decided), as
    /// Java and .NET do (Scroll.Start).
    pub fn on_immediate_scroll_start(&mut self, config: &ScrollConfig) {
        if config.cursor_change {
            self.cursor_axis = Some(ScrollAxis::Vertical);
        }
    }

    pub fn on_drag_cursor_move(&mut self, config: &ScrollConfig) {
        if config.cursor_change && !config.is_vh_adjuster_active() && self.cursor_axis.is_none() {
            self.cursor_axis = Some(ScrollAxis::Vertical);
        }
    }

    pub fn cursor_axis(&self, config: &ScrollConfig) -> Option<ScrollAxis> {
        config.cursor_change.then_some(self.cursor_axis).flatten()
    }

    pub fn process_raw_motion(&mut self, x: i32, y: i32, config: &ScrollConfig) -> Vec<WheelEvent> {
        if x == 0 && y == 0 {
            return Vec::new();
        }
        let (dx, dy) = if config.swap_scroll { (y, x) } else { (x, y) };
        let mut events = Vec::new();

        if config.is_vh_adjuster_active() {
            self.process_vh_adjuster(dx, dy, config, &mut events);
        } else {
            self.process_standard(dx, dy, config, &mut events);
        }
        events
    }

    fn process_vh_adjuster(
        &mut self,
        dx: i32,
        dy: i32,
        config: &ScrollConfig,
        events: &mut Vec<WheelEvent>,
    ) {
        let adx = i64::from(dx).abs();
        let ady = i64::from(dy).abs();

        let cur_vhd = match self.fixed_vhd {
            None => {
                let min_thr = i64::from(config.first_min_threshold);
                if adx > min_thr || ady > min_thr {
                    let y_weighted = if config.first_prefer_vertical {
                        ady * 2
                    } else {
                        ady
                    };
                    let chosen = if y_weighted >= adx {
                        ScrollAxis::Vertical
                    } else {
                        ScrollAxis::Horizontal
                    };
                    self.fixed_vhd = Some(chosen);
                    Some(chosen)
                } else {
                    None
                }
            }
            Some(fixed) => match config.vh_adjuster_method {
                VhAdjusterMethod::Fixed => Some(fixed),
                VhAdjusterMethod::Switching => {
                    let sthr = i64::from(config.switching_threshold);
                    if ady > sthr {
                        Some(ScrollAxis::Vertical)
                    } else if adx > sthr {
                        Some(ScrollAxis::Horizontal)
                    } else {
                        None
                    }
                }
            },
        };

        if let Some(dir) = cur_vhd
            && Some(dir) != self.latest_vhd
        {
            self.latest_vhd = Some(dir);
            if config.cursor_change {
                self.cursor_axis = Some(dir);
            }
        }

        match self.latest_vhd {
            Some(ScrollAxis::Vertical) if dy != 0 => self.emit_vertical(dy, config, events),
            Some(ScrollAxis::Horizontal) if dx != 0 => self.emit_horizontal(dx, config, events),
            _ => {}
        }
    }

    fn process_standard(
        &mut self,
        dx: i32,
        dy: i32,
        config: &ScrollConfig,
        events: &mut Vec<WheelEvent>,
    ) {
        if i64::from(dy).abs() > i64::from(config.vertical_threshold) {
            self.emit_vertical(dy, config, events);
        }
        // The cursor is not changed here: without the VH Adjuster it stays the vertical one,
        // also when scrolling horizontally, as in Java and .NET (set when scroll mode starts, or
        // at the first move of a drag trigger: on_immediate_scroll_start, on_drag_cursor_move).
        if config.horizontal_scroll && i64::from(dx).abs() > i64::from(config.horizontal_threshold) {
            self.emit_horizontal(dx, config, events);
        }
    }

    fn emit_vertical(&mut self, d: i32, config: &ScrollConfig, events: &mut Vec<WheelEvent>) {
        if d == 0 {
            return;
        }
        if config.real_wheel_mode {
            let base = if d > 0 {
                -config.wheel_delta
            } else {
                config.wheel_delta
            };
            let delta = if config.reverse_scroll { -base } else { base };
            emit_real_wheel(
                d,
                &mut self.vw_count,
                &mut self.v_last_move,
                config.v_wheel_move,
                config.quick_turn,
                delta,
                events,
                WheelEvent::Vertical,
            );
        } else {
            let raw = if config.accel_table {
                apply_accel(d, config)
            } else {
                d
            };
            let delta = if config.reverse_scroll {
                raw
            } else {
                raw.saturating_neg()
            };
            if delta != 0 {
                events.push(WheelEvent::Vertical(delta));
            }
        }
    }

    fn emit_horizontal(&mut self, d: i32, config: &ScrollConfig, events: &mut Vec<WheelEvent>) {
        if d == 0 {
            return;
        }
        if config.real_wheel_mode {
            let base = if d > 0 {
                config.wheel_delta
            } else {
                -config.wheel_delta
            };
            let delta = if config.reverse_scroll { -base } else { base };
            emit_real_wheel(
                d,
                &mut self.hw_count,
                &mut self.h_last_move,
                config.h_wheel_move,
                config.quick_turn,
                delta,
                events,
                WheelEvent::Horizontal,
            );
        } else {
            let raw = if config.accel_table {
                apply_accel(d, config)
            } else {
                d
            };
            let delta = if config.reverse_scroll {
                raw.saturating_neg()
            } else {
                raw
            };
            if delta != 0 {
                events.push(WheelEvent::Horizontal(delta));
            }
        }
    }
}

fn initial_wheel_count(wheel_move: i32, quick_first: bool) -> i64 {
    if quick_first {
        i64::from(wheel_move)
    } else {
        i64::from(wheel_move / 2)
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_real_wheel(
    d: i32,
    count: &mut i64,
    last_move: &mut MoveDirection,
    wheel_move: i32,
    quick_turn: bool,
    delta: i32,
    events: &mut Vec<WheelEvent>,
    make_event: fn(i32) -> WheelEvent,
) {
    if delta == 0 {
        return;
    }
    if last_move.is_turn(d) {
        *count = if quick_turn {
            i64::from(wheel_move)
        } else {
            i64::from(wheel_move / 2)
        };
    }
    *count = count.saturating_add(i64::from(d).abs());

    let step = i64::from(wheel_move);
    let mut emitted = 0usize;
    while step > 0 && *count >= step && emitted < MAX_NOTCHES_PER_PACKET {
        events.push(make_event(delta));
        *count -= step;
        emitted += 1;
    }
    if step > 0 && *count >= step {
        *count %= step;
    }

    *last_move = if d > 0 {
        MoveDirection::Plus
    } else {
        MoveDirection::Minus
    };
}

pub(crate) fn get_nearest_index(d: i32, thresholds: &[i32]) -> usize {
    let ad = i64::from(d).abs();
    for (i, &thr) in thresholds.iter().enumerate() {
        let n = i64::from(thr);
        if n == ad {
            return i;
        }
        if n > ad {
            if i == 0 {
                return 0;
            }
            let prev = i64::from(thresholds[i - 1]);
            // Strict `<` breaks exact midpoint ties in favor of the lower threshold index.
            return if (n - ad) < (prev - ad).abs() {
                i
            } else {
                i - 1
            };
        }
    }
    thresholds.len().saturating_sub(1)
}

fn apply_accel(d: i32, config: &ScrollConfig) -> i32 {
    let (thresholds, multipliers) = config.active_accel_table();
    let idx = get_nearest_index(d, thresholds);
    ((d as f64) * multipliers[idx]) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_mode_handles_thresholds_reverse_swap_and_horizontal_toggle() {
        let mut cfg = ScrollConfig {
            accel_table: false,
            vertical_threshold: 0,
            horizontal_threshold: 75,
            ..ScrollConfig::default()
        };
        let mut session = ScrollSession::new(&cfg);

        // Strict `>` threshold check: 75 does not emit horizontal, 76 does.
        assert_eq!(
            session.process_raw_motion(75, 10, &cfg),
            vec![WheelEvent::Vertical(-10)]
        );
        assert_eq!(
            session.process_raw_motion(76, -10, &cfg),
            vec![WheelEvent::Vertical(10), WheelEvent::Horizontal(76)]
        );

        // Reverse flips both axes.
        cfg.reverse_scroll = true;
        assert_eq!(
            session.process_raw_motion(80, 10, &cfg),
            vec![WheelEvent::Vertical(10), WheelEvent::Horizontal(-80)]
        );

        // Swap exchanges x and y BEFORE thresholds and emitters.
        cfg.reverse_scroll = false;
        cfg.swap_scroll = true;
        cfg.vertical_threshold = 2;
        cfg.horizontal_threshold = 5;
        // Raw (x=6, y=3) -> swapped (dx=3, dy=6). |dy|=6 > 2 -> V(-6), |dx|=3 <= 5 -> no H.
        assert_eq!(
            session.process_raw_motion(6, 3, &cfg),
            vec![WheelEvent::Vertical(-6)]
        );
        // Raw (x=10, y=6) -> swapped (dx=6, dy=10). Both emit.
        assert_eq!(
            session.process_raw_motion(10, 6, &cfg),
            vec![WheelEvent::Vertical(-10), WheelEvent::Horizontal(6)]
        );

        // Horizontal scroll disabled suppresses horizontal even above threshold.
        cfg.swap_scroll = false;
        cfg.horizontal_scroll = false;
        assert_eq!(
            session.process_raw_motion(200, 10, &cfg),
            vec![WheelEvent::Vertical(-10)]
        );

        // Without the VH Adjuster the cursor stays vertical, also for a horizontal scroll.
        cfg.horizontal_scroll = true;
        session.on_immediate_scroll_start(&cfg);
        assert_eq!(
            session.process_raw_motion(200, 0, &cfg),
            vec![WheelEvent::Horizontal(200)]
        );
        assert_eq!(session.cursor_axis(&cfg), Some(ScrollAxis::Vertical));
    }

    #[test]
    fn vh_adjuster_starts_with_vertical_cursor_then_follows_the_direction() {
        let cfg = ScrollConfig {
            accel_table: false,
            vh_adjuster_mode: true,
            vh_adjuster_method: VhAdjusterMethod::Switching,
            first_prefer_vertical: false,
            first_min_threshold: 5,
            switching_threshold: 50,
            cursor_change: true,
            ..ScrollConfig::default()
        };
        let mut s = ScrollSession::new(&cfg);
        s.on_immediate_scroll_start(&cfg);
        assert_eq!(s.cursor_axis(&cfg), Some(ScrollAxis::Vertical));
        assert_eq!(s.process_raw_motion(20, 1, &cfg), vec![WheelEvent::Horizontal(20)]);
        assert_eq!(s.cursor_axis(&cfg), Some(ScrollAxis::Horizontal));

        // Drag triggers: no cursor change before a direction is decided (as .NET dragStart).
        let mut drag = ScrollSession::new(&cfg);
        drag.on_drag_cursor_move(&cfg);
        assert_eq!(drag.cursor_axis(&cfg), None);
    }

    #[test]
    fn accel_table_nearest_index_presets_and_custom_tables() {
        // Default thresholds: [1, 2, 3, 5, 7, 10, 14, 20, 30, 43, 63, 91]
        assert_eq!(get_nearest_index(0, &DEFAULT_ACCEL_THRESHOLD), 0);
        assert_eq!(get_nearest_index(1, &DEFAULT_ACCEL_THRESHOLD), 0);
        // Midpoint between 3 (idx 2) and 5 (idx 3) is 4 -> strict `<` picks lower index (2).
        assert_eq!(get_nearest_index(4, &DEFAULT_ACCEL_THRESHOLD), 2);
        // 17 between 14 (idx 6, dist 3) and 20 (idx 7, dist 3) -> tie picks lower index (6).
        assert_eq!(get_nearest_index(17, &DEFAULT_ACCEL_THRESHOLD), 6);
        // 18 between 14 (dist 4) and 20 (dist 2) -> picks 7.
        assert_eq!(get_nearest_index(18, &DEFAULT_ACCEL_THRESHOLD), 7);
        assert_eq!(get_nearest_index(200, &DEFAULT_ACCEL_THRESHOLD), 11);

        // Custom table starting above 1: smaller values clamp to index 0.
        let custom_thr = [5, 10];
        assert_eq!(get_nearest_index(2, &custom_thr), 0);

        let mut cfg = ScrollConfig {
            accel_table: true,
            accel_multiplier: AccelPreset::M6,
            horizontal_threshold: 0,
            ..ScrollConfig::default()
        };
        let mut session = ScrollSession::new(&cfg);
        // d = 10 -> idx 5 (M6 = 3.3) -> 33.0 -> -33
        assert_eq!(
            session.process_raw_motion(0, 10, &cfg),
            vec![WheelEvent::Vertical(-33)]
        );

        // Custom table active + fractional < 1.0 truncating to 0 suppresses event.
        cfg.custom_accel_table = true;
        cfg.custom_table = CustomTableState::Loaded {
            thresholds: vec![1, 5],
            multipliers: vec![0.4, 2.5],
        };
        // d = 2 -> idx 0 (mul 0.4) -> 0.8 as i32 == 0 -> suppressed.
        assert_eq!(session.process_raw_motion(0, 2, &cfg), Vec::new());
        // d = 5 -> idx 1 (mul 2.5) -> 12.5 as i32 == 12 -> -12.
        assert_eq!(
            session.process_raw_motion(0, 5, &cfg),
            vec![WheelEvent::Vertical(-12)]
        );

        // Invalid custom table falls back to preset M6 even if custom_accel_table=true.
        cfg.custom_table =
            CustomTableState::Invalid(crate::config::CustomTableError::LengthMismatch);
        assert_eq!(
            session.process_raw_motion(0, 10, &cfg),
            vec![WheelEvent::Vertical(-33)]
        );
    }

    #[test]
    fn real_wheel_mode_accumulation_quick_flags_multi_notch_and_reverse() {
        let mut cfg = ScrollConfig {
            real_wheel_mode: true,
            accel_table: true, // Ignored in RealWheel mode.
            wheel_delta: 120,
            v_wheel_move: 60,
            h_wheel_move: 40,
            vertical_threshold: 0,
            horizontal_threshold: 0,
            quick_first: false,
            quick_turn: false,
            ..ScrollConfig::default()
        };
        let mut session = ScrollSession::new(&cfg);

        // Initial count is 30 (60 / 2). +29 -> 59 (no notch); +1 -> 60 -> 1 notch (-120), rem 0.
        assert_eq!(session.process_raw_motion(0, 29, &cfg), Vec::new());
        assert_eq!(
            session.process_raw_motion(0, 1, &cfg),
            vec![WheelEvent::Vertical(-120)]
        );

        // Large single packet emits multiple separate notches: rem 0 + 130 -> 2 notches, rem 10.
        assert_eq!(
            session.process_raw_motion(0, 130, &cfg),
            vec![WheelEvent::Vertical(-120), WheelEvent::Vertical(-120)]
        );

        // Direction reversal with quick_turn=false resets count to half (30).
        // -29 -> 59 (no notch), -1 -> 60 -> 1 upward notch (+120).
        assert_eq!(session.process_raw_motion(0, -29, &cfg), Vec::new());
        assert_eq!(
            session.process_raw_motion(0, -1, &cfg),
            vec![WheelEvent::Vertical(120)]
        );

        // Horizontal uses h_wheel_move=40 (half=20) and positive delta for +dx.
        assert_eq!(session.process_raw_motion(19, 0, &cfg), Vec::new());
        assert_eq!(
            session.process_raw_motion(1, 0, &cfg),
            vec![WheelEvent::Horizontal(120)]
        );

        // quick_first=true starts count at full wheel_move; quick_turn=true resets to full on turn.
        cfg.quick_first = true;
        cfg.quick_turn = true;
        cfg.reverse_scroll = true;
        let mut quick = ScrollSession::new(&cfg);
        // Any 1px move triggers first notch immediately; reverse flips signs.
        assert_eq!(
            quick.process_raw_motion(1, 1, &cfg),
            vec![WheelEvent::Vertical(120), WheelEvent::Horizontal(-120)]
        );
        // Immediate turn on both axes triggers another notch at 1px.
        assert_eq!(
            quick.process_raw_motion(-1, -1, &cfg),
            vec![WheelEvent::Vertical(-120), WheelEvent::Horizontal(120)]
        );
    }

    #[test]
    fn vh_adjuster_locks_switches_ignores_std_thresholds_and_requires_horizontal() {
        let mut cfg = ScrollConfig {
            accel_table: false,
            vh_adjuster_mode: true,
            vh_adjuster_method: VhAdjusterMethod::Switching,
            first_prefer_vertical: true,
            first_min_threshold: 5,
            switching_threshold: 50,
            vertical_threshold: 100, // Ignored while VHAdjuster is active!
            horizontal_threshold: 100,
            cursor_change: true,
            ..ScrollConfig::default()
        };

        // 1. Small move <= first_min_threshold (5) stays undecided.
        let mut s = ScrollSession::new(&cfg);
        assert_eq!(s.process_raw_motion(5, 5, &cfg), Vec::new());
        assert_eq!(s.cursor_axis(&cfg), None);

        // 2. first_prefer_vertical=true: adx=8, ady=6 -> 6*2=12 >= 8 -> Vertical.
        //    Std vertical_threshold=100 is ignored, so dy=6 emits!
        assert_eq!(
            s.process_raw_motion(8, 6, &cfg),
            vec![WheelEvent::Vertical(-6)]
        );
        assert_eq!(s.cursor_axis(&cfg), Some(ScrollAxis::Vertical));

        // 3. Move below switching_threshold (50) stays Vertical even if dx=50.
        assert_eq!(
            s.process_raw_motion(50, 3, &cfg),
            vec![WheelEvent::Vertical(-3)]
        );
        assert_eq!(s.cursor_axis(&cfg), Some(ScrollAxis::Vertical));

        // 4. dx=51 (> 50) while dy=10 (<= 50) switches to Horizontal.
        assert_eq!(
            s.process_raw_motion(51, 10, &cfg),
            vec![WheelEvent::Horizontal(51)]
        );
        assert_eq!(s.cursor_axis(&cfg), Some(ScrollAxis::Horizontal));

        // 5. Dual-axis exceed (>50 on both): vertical wins because ady > sthr is checked first.
        assert_eq!(
            s.process_raw_motion(60, 55, &cfg),
            vec![WheelEvent::Vertical(-55)]
        );
        assert_eq!(s.cursor_axis(&cfg), Some(ScrollAxis::Vertical));

        // 6. Fixed method never switches after first direction, and first_prefer_vertical=false
        //    picks Horizontal when adx=8 > ady=6.
        cfg.vh_adjuster_method = VhAdjusterMethod::Fixed;
        cfg.first_prefer_vertical = false;
        let mut fixed = ScrollSession::new(&cfg);
        assert_eq!(
            fixed.process_raw_motion(8, 6, &cfg),
            vec![WheelEvent::Horizontal(8)]
        );
        assert_eq!(
            fixed.process_raw_motion(10, 200, &cfg),
            vec![WheelEvent::Horizontal(10)]
        );
        assert_eq!(fixed.cursor_axis(&cfg), Some(ScrollAxis::Horizontal));

        // 7. When horizontal_scroll=false, VHAdjuster is inactive and std thresholds apply.
        cfg.horizontal_scroll = false;
        let mut std_only = ScrollSession::new(&cfg);
        assert_eq!(std_only.process_raw_motion(200, 50, &cfg), Vec::new());
        assert_eq!(
            std_only.process_raw_motion(200, 101, &cfg),
            vec![WheelEvent::Vertical(-101)]
        );
    }

    #[test]
    fn vh_adjuster_combines_cleanly_with_real_wheel_mode() {
        let cfg = ScrollConfig {
            real_wheel_mode: true,
            wheel_delta: 120,
            v_wheel_move: 20,
            h_wheel_move: 20,
            quick_first: false,
            vh_adjuster_mode: true,
            vh_adjuster_method: VhAdjusterMethod::Switching,
            first_min_threshold: 5,
            switching_threshold: 15,
            ..ScrollConfig::default()
        };
        let mut s = ScrollSession::new(&cfg);
        // ady=10 (> 5) picks Vertical and accumulates 10 + 10 = 20 -> 1 vertical notch.
        assert_eq!(
            s.process_raw_motion(0, 10, &cfg),
            vec![WheelEvent::Vertical(-120)]
        );
        // adx=16 (> 15) switches to Horizontal and accumulates 10 + 16 = 26 -> 1 horizontal notch.
        assert_eq!(
            s.process_raw_motion(16, 0, &cfg),
            vec![WheelEvent::Horizontal(120)]
        );
    }
}
