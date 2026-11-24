use crate::config::{Config, CustomTableState, Trigger, TriggerKey};
use crate::exclusions::ExclusionMatcher;
use crate::properties::ConfigStore;
use crate::scroll::{ScrollAxis, ScrollConfig, ScrollSession, WheelEvent};
use crate::sender::{KIND_DOWN, KIND_PLAIN, KIND_RELEASE, MouseButton, PRESS_ID_MASK, Point, ResendTags, ResentInfo};

const KEY_COUNT: usize = 256;
const VK_PAUSE: u32 = 0x13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TriggerConfig {
    pub first_trigger: Trigger,
    pub poll_timeout_ms: u32,
    pub scroll_locktime_ms: u32,
    pub dragged_lock: bool,
    pub send_middle_click: bool,
    pub keyboard_hook: bool,
    pub target_vk_code: TriggerKey,
}

impl TriggerConfig {
    pub fn from_config(config: &Config) -> Self {
        Self {
            first_trigger: config.first_trigger,
            poll_timeout_ms: config.poll_timeout.clamp(50, 500) as u32,
            scroll_locktime_ms: config.scroll_locktime.clamp(150, 500) as u32,
            dragged_lock: config.dragged_lock,
            send_middle_click: config.send_middle_click,
            keyboard_hook: config.keyboard_hook,
            target_vk_code: config.target_vk_code,
        }
    }

    pub fn keyboard_enabled(&self) -> bool {
        self.keyboard_hook && self.target_vk_code != TriggerKey::None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineConfig {
    pub scroll: ScrollConfig,
    pub trigger: TriggerConfig,
    pub exclusions: ExclusionMatcher,
}

impl EngineConfig {
    pub fn from_config(config: &Config, custom_table: &CustomTableState) -> Self {
        Self {
            scroll: ScrollConfig::from_config(config, custom_table),
            trigger: TriggerConfig::from_config(config),
            exclusions: ExclusionMatcher::default(),
        }
    }

    pub fn with_exclusions(mut self, exclusions: ExclusionMatcher) -> Self {
        self.exclusions = exclusions;
        self
    }

    pub fn from_store(store: &ConfigStore) -> Self {
        Self::from_config(store.config(), store.custom_table())
            .with_exclusions(store.exclusions().to_matcher())
    }

    pub fn trigger_summary(&self) -> String {
        let mouse = self.trigger.first_trigger.as_property_value();
        if self.trigger.keyboard_enabled() {
            format!("{mouse} + {}", self.trigger.target_vk_code.as_property_value())
        } else {
            mouse.to_owned()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookDecision {
    Pass,
    Suppress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookMouseKind {
    Move,
    Down(MouseButton),
    Up(MouseButton),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HookMouseEvent {
    pub kind: HookMouseKind,
    pub point: Point,
    pub time: u32,
    pub injected: bool,
    pub extra_info: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HookKeyEvent {
    pub vk_code: u32,
    pub action: KeyAction,
    pub time: u32,
}

pub trait TriggerHost {
    /// Registers Raw Input for scroll mode: false if Windows refused (scroll mode does not start).
    fn start_raw_input(&mut self) -> bool;
    fn stop_raw_input(&mut self);
    fn set_poll_timer(&mut self, timeout_ms: u32);
    fn kill_poll_timer(&mut self);
    fn update_cursor(&mut self, axis: Option<ScrollAxis>);
    fn notify_scroll_activity(&mut self);
    fn send_wheel(&mut self, event: WheelEvent, generation: u64);
    fn send_click(&mut self, button: MouseButton, point: Point, extra_info: usize);
    fn send_diff_point_click(
        &mut self,
        button: MouseButton,
        down_point: Point,
        up_point: Point,
        extra_info: usize,
    );
    fn send_down(&mut self, button: MouseButton, point: Point, extra_info: usize);
    fn send_up(&mut self, button: MouseButton, point: Point, extra_info: usize);
    fn set_sender_paused(&mut self, paused: bool);
    fn set_sender_generation(&mut self, generation: u64);
    fn cursor_pos(&self) -> Point;
    fn is_escape_down(&self) -> bool;
    fn is_modifier_down(&self) -> bool;
    fn is_excluded(&self, point: Point, exclusions: &ExclusionMatcher) -> bool;
}

#[derive(Default)]
struct ButtonTracker {
    last_press_id: u32,
    press_id: [u32; MouseButton::COUNT],
    down_suppressed: [bool; MouseButton::COUNT],
    down_passed_id: [u32; MouseButton::COUNT],
    up_passed: [bool; MouseButton::COUNT],
    release_queued_id: [u32; MouseButton::COUNT],
}

impl ButtonTracker {
    fn drop_resent(&self, button: MouseButton, is_down: bool, kind: usize, press_id: u32) -> bool {
        let i = button.index();
        match (is_down, kind) {
            (true, KIND_DOWN) => {
                (press_id != self.press_id[i] || self.up_passed[i])
                    && press_id != self.release_queued_id[i]
            }
            (false, KIND_RELEASE) => press_id != self.down_passed_id[i],
            _ => false,
        }
    }

    fn on_before_physical(&mut self, button: MouseButton, is_down: bool) {
        if !is_down {
            return;
        }
        let i = button.index();
        self.last_press_id = (self.last_press_id % PRESS_ID_MASK) + 1;
        self.press_id[i] = self.last_press_id;
        self.up_passed[i] = false;
        self.down_suppressed[i] = false;
        self.release_queued_id[i] = 0;
    }

    fn current_press_id(&self, button: MouseButton) -> u32 {
        self.press_id[button.index()]
    }

    fn would_drop_lone_up(&self, button: MouseButton) -> bool {
        let i = button.index();
        self.down_suppressed[i] && self.down_passed_id[i] != self.press_id[i]
    }

    fn drop_lone_up(&mut self, button: MouseButton) -> bool {
        if self.would_drop_lone_up(button) {
            self.up_passed[button.index()] = true;
            true
        } else {
            false
        }
    }

    fn mark_release_queued(&mut self, button: MouseButton, press_id: u32) {
        self.release_queued_id[button.index()] = press_id;
    }

    fn on_after_mouse(
        &mut self,
        button: MouseButton,
        is_down: bool,
        injected: bool,
        kind: usize,
        press_id: u32,
        passed: bool,
    ) {
        let i = button.index();
        if !injected && is_down && !passed {
            self.down_suppressed[i] = true;
            return;
        }
        if passed {
            if is_down && kind == KIND_DOWN && injected {
                self.down_passed_id[i] = press_id;
            } else if is_down && !injected {
                self.down_passed_id[i] = self.press_id[i];
            } else if !is_down && (!injected || kind == KIND_RELEASE) {
                self.down_passed_id[i] = 0;
                self.up_passed[i] = true;
                self.down_suppressed[i] = false;
                self.release_queued_id[i] = 0;
            }
        }
    }
}

#[derive(Default)]
struct LastFlags {
    suppressed_down: [bool; MouseButton::COUNT],
    resent_down: [bool; MouseButton::COUNT],
}

impl LastFlags {
    fn reset(&mut self) {
        self.suppressed_down = [false; MouseButton::COUNT];
        self.resent_down = [false; MouseButton::COUNT];
    }

    fn take_resent_down(&mut self, button: MouseButton) -> bool {
        let i = button.index();
        let had = self.resent_down[i];
        self.resent_down[i] = false;
        had
    }
}

struct KeyFlags {
    kd_suppressed: [bool; KEY_COUNT],
    kd_passed: [bool; KEY_COUNT],
    kd_lone_up: [bool; KEY_COUNT],
    last_key_event: Option<(u32, KeyAction)>,
}

impl Default for KeyFlags {
    fn default() -> Self {
        Self {
            kd_suppressed: [false; KEY_COUNT],
            kd_passed: [false; KEY_COUNT],
            kd_lone_up: [false; KEY_COUNT],
            last_key_event: None,
        }
    }
}

impl KeyFlags {
    fn suppressed_to_lone_up(&mut self) {
        for vk in 0..KEY_COUNT {
            if self.kd_suppressed[vk] {
                self.kd_suppressed[vk] = false;
                self.kd_lone_up[vk] = true;
            }
        }
        self.last_key_event = None;
    }

    /// Windows had removed the old keyboard hook: a key may have been released and pressed again
    /// with no hook to see it (its down reached the application). The records that would suppress
    /// its repeats or its up are forgotten; the passed keys are kept (they pass anyway).
    fn forget_suppressed(&mut self) {
        self.kd_suppressed = [false; KEY_COUNT];
        self.kd_lone_up = [false; KEY_COUNT];
        self.last_key_event = None;
    }

    /// The keyboard hook is removed: the ups will not be seen
    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug)]
struct LrWait {
    button: MouseButton,
    point: Point,
    press_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollOwner {
    MouseButton(MouseButton),
    MouseDouble,
    Keyboard,
}

struct ActiveScroll {
    owner: ScrollOwner,
    is_drag_trigger: bool,
    drag_start: Point,
    released: bool,
    started_at: u32,
    second_trigger_up: bool,
    dragged: bool,
    scrolled: bool,
    session: ScrollSession,
}

impl ActiveScroll {
    fn new_immediate(owner: ScrollOwner, started_at: u32, config: &ScrollConfig) -> Self {
        let mut session = ScrollSession::new(config);
        session.on_immediate_scroll_start(config);
        Self {
            owner,
            is_drag_trigger: false,
            drag_start: Point::default(),
            released: false,
            started_at,
            second_trigger_up: false,
            dragged: false,
            scrolled: false,
            session,
        }
    }

    fn new_drag(button: MouseButton, drag_start: Point, started_at: u32, config: &ScrollConfig) -> Self {
        Self {
            owner: ScrollOwner::MouseButton(button),
            is_drag_trigger: true,
            drag_start,
            released: false,
            started_at,
            second_trigger_up: false,
            dragged: false,
            scrolled: false,
            session: ScrollSession::new(config),
        }
    }

    fn dragged_or_scrolled(&self) -> bool {
        self.dragged || self.scrolled
    }
}

pub struct TriggerEngine {
    config: EngineConfig,
    tags: ResendTags,
    paused: bool,
    generation: u64,
    tracker: ButtonTracker,
    last_flags: LastFlags,
    key_flags: KeyFlags,
    last_mouse_event: Option<(MouseButton, bool)>,
    lr_wait: Option<LrWait>,
    scroll: Option<ActiveScroll>,
    active_cursor_axis: Option<ScrollAxis>,
}

impl TriggerEngine {
    pub fn new(config: EngineConfig, tags: ResendTags) -> Self {
        Self {
            config,
            tags,
            paused: false,
            generation: 0,
            tracker: ButtonTracker::default(),
            last_flags: LastFlags::default(),
            key_flags: KeyFlags::default(),
            last_mouse_event: None,
            lr_wait: None,
            scroll: None,
            active_cursor_axis: None,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_scrolling(&self) -> bool {
        self.scroll.is_some()
    }

    pub fn keyboard_hook_enabled(&self) -> bool {
        self.config.trigger.keyboard_enabled()
    }

    pub fn handle_mouse(
        &mut self,
        event: HookMouseEvent,
        host: &mut impl TriggerHost,
    ) -> HookDecision {
        let pass = match event.kind {
            HookMouseKind::Move => self.handle_mouse_move(event, host),
            HookMouseKind::Down(btn) => self.handle_mouse_button(btn, true, event, host),
            HookMouseKind::Up(btn) => self.handle_mouse_button(btn, false, event, host),
        };
        if pass {
            HookDecision::Pass
        } else {
            HookDecision::Suppress
        }
    }

    fn handle_mouse_move(&mut self, event: HookMouseEvent, host: &mut impl TriggerHost) -> bool {
        if self.paused {
            return true;
        }
        if let Some(wait) = self.lr_wait {
            if !event.injected && event.point != wait.point {
                self.lr_wait = None;
                host.kill_poll_timer();
                self.last_flags.resent_down[wait.button.index()] = true;
                host.send_down(
                    wait.button,
                    wait.point,
                    self.tags.encode(KIND_DOWN, wait.press_id),
                );
                return false;
            }
            return true;
        }
        if let Some(scroll) = &mut self.scroll {
            if !scroll.released
                && scroll.is_drag_trigger
                && !event.injected
                && event.point != scroll.drag_start
            {
                scroll.dragged = true;
                scroll.session.on_drag_cursor_move(&self.config.scroll);
                self.sync_cursor(host);
            }
            return false;
        }
        true
    }

    fn handle_mouse_button(
        &mut self,
        btn: MouseButton,
        is_down: bool,
        event: HookMouseEvent,
        host: &mut impl TriggerHost,
    ) -> bool {
        if event.injected {
            return match self.tags.decode(event.extra_info) {
                Some(ResentInfo::Click) => true,
                Some(ResentInfo::Button { kind, press_id }) => {
                    if self.tracker.drop_resent(btn, is_down, kind, press_id) {
                        false
                    } else {
                        self.tracker
                            .on_after_mouse(btn, is_down, true, kind, press_id, true);
                        true
                    }
                }
                None => true,
            };
        }

        self.tracker.on_before_physical(btn, is_down);

        if !is_down && host.is_escape_down() {
            self.init_state(host);
            return self.finish_physical_mouse(btn, false, event.point, true, host);
        }

        if self.paused {
            return self.finish_physical_mouse(btn, is_down, event.point, true, host);
        }

        let passed = if is_down {
            self.on_physical_down(btn, event.point, event.time, host)
        } else {
            self.on_physical_up(btn, event.point, event.time, host)
        };
        self.finish_physical_mouse(btn, is_down, event.point, passed, host)
    }

    fn finish_physical_mouse(
        &mut self,
        btn: MouseButton,
        is_down: bool,
        point: Point,
        mut passed: bool,
        host: &mut impl TriggerHost,
    ) -> bool {
        if !is_down && passed && self.tracker.drop_lone_up(btn) {
            passed = false;
            if self.last_flags.take_resent_down(btn) {
                let press_id = self.tracker.current_press_id(btn);
                self.tracker.mark_release_queued(btn, press_id);
                host.send_up(btn, point, self.tags.encode(KIND_RELEASE, press_id));
            }
        } else if !is_down {
            let _ = self.last_flags.take_resent_down(btn);
        }
        self.tracker.on_after_mouse(btn, is_down, false, 0, 0, passed);
        passed
    }

    fn on_physical_down(
        &mut self,
        btn: MouseButton,
        point: Point,
        time: u32,
        host: &mut impl TriggerHost,
    ) -> bool {
        if let Some(wait) = self.lr_wait {
            if btn == wait.button {
                self.lr_wait = None;
                host.kill_poll_timer();
            } else if self.is_second_double_button(btn, wait.button) {
                self.lr_wait = None;
                host.kill_poll_timer();
                self.last_flags.suppressed_down[wait.button.index()] = true;
                self.last_flags.suppressed_down[btn.index()] = true;
                self.last_mouse_event = Some((btn, true));
                if self.is_released_scroll() || self.is_keyboard_scroll() {
                    self.exit_scroll(host);
                } else if !self.start_scroll(
                    ActiveScroll::new_immediate(ScrollOwner::MouseDouble, time, &self.config.scroll),
                    host,
                ) {
                    // Scroll mode could not start (Raw Input refused): both downs are resent in
                    // order, as presses of their own, so that no click is lost and no move is
                    // suppressed without a scroll. Their ups are handled as those of resent downs.
                    let press_id = self.tracker.current_press_id(btn);
                    for (button, at, id) in
                        [(wait.button, wait.point, wait.press_id), (btn, point, press_id)]
                    {
                        self.last_flags.suppressed_down[button.index()] = false;
                        self.last_flags.resent_down[button.index()] = true;
                        host.send_down(button, at, self.tags.encode(KIND_DOWN, id));
                    }
                }
                return false;
            } else {
                self.lr_wait = None;
                host.kill_poll_timer();
                self.last_flags.resent_down[wait.button.index()] = true;
                host.send_down(
                    wait.button,
                    wait.point,
                    self.tags.encode(KIND_DOWN, wait.press_id),
                );
            }
        }

        let suppressed = self.last_flags.suppressed_down[btn.index()];
        let single_owner = self.is_single_scroll_owner(btn);
        if self.last_mouse_event == Some((btn, true)) || suppressed || single_owner {
            // A down again whose earlier press held scroll mode (its up was missed): the press of
            // the single or drag trigger, or one of the pair of the LR, Left and Right triggers
            // whose down is still suppressed. Not a button of the pair pressed again after its up
            // was seen (the other button still holds the scroll): that press passes below.
            let held = single_owner
                || (self.config.trigger.first_trigger.is_double() && btn.is_lr() && suppressed);
            if held
                && self.is_pressed_mouse_scroll()
                && let Some(scroll) = &mut self.scroll
            {
                scroll.released = true;
            }
            self.last_flags.suppressed_down[btn.index()] = false;
            self.last_flags.resent_down[btn.index()] = false;
        }
        self.last_mouse_event = Some((btn, true));

        if self.is_released_scroll() {
            self.exit_scroll(host);
            self.last_flags.suppressed_down[btn.index()] = true;
            return false;
        }

        let trigger = self.config.trigger.first_trigger;
        if trigger.is_none() {
            return true;
        }

        if trigger.is_double() {
            if self.is_first_double_button(btn) {
                if self.is_mouse_scroll() {
                    // The pressed scroll mode of the pair (a released one has ended above): a
                    // button of it pressed again after its up passes, and so does its up, as in
                    // .NET (passPressedScrollMode). The scroll goes on with the other button held.
                    return true;
                }
                if !self.is_keyboard_scroll() && host.is_excluded(point, &self.config.exclusions) {
                    return true;
                }
                let press_id = self.tracker.current_press_id(btn);
                self.lr_wait = Some(LrWait {
                    button: btn,
                    point,
                    press_id,
                });
                host.set_poll_timer(self.config.trigger.poll_timeout_ms);
                return false;
            }
            return true;
        }

        if let Some(trigger_btn) = MouseButton::single_from_trigger(trigger) {
            if btn != trigger_btn {
                return true;
            }
            if self.is_keyboard_scroll() {
                self.exit_scroll(host);
                self.last_flags.suppressed_down[btn.index()] = true;
                return false;
            }
            if host.is_excluded(point, &self.config.exclusions) {
                return true;
            }
            if trigger_btn == MouseButton::Middle
                && self.config.trigger.send_middle_click
                && host.is_modifier_down()
            {
                host.send_click(MouseButton::Middle, point, self.tags.click_tag());
                self.last_flags.suppressed_down[btn.index()] = true;
                return false;
            }
            let scroll = ActiveScroll::new_immediate(
                ScrollOwner::MouseButton(btn),
                time,
                &self.config.scroll,
            );
            if !self.start_scroll(scroll, host) {
                // Scroll mode could not start: the button works as if it were not the trigger
                return true;
            }
            self.last_flags.suppressed_down[btn.index()] = true;
            return false;
        }

        if let Some(trigger_btn) = MouseButton::drag_from_trigger(trigger) {
            if btn != trigger_btn {
                return true;
            }
            if self.is_keyboard_scroll() {
                self.exit_scroll(host);
                self.last_flags.suppressed_down[btn.index()] = true;
                return false;
            }
            if host.is_excluded(point, &self.config.exclusions) {
                return true;
            }
            let scroll = ActiveScroll::new_drag(btn, point, time, &self.config.scroll);
            if !self.start_scroll(scroll, host) {
                return true;
            }
            self.last_flags.suppressed_down[btn.index()] = true;
            return false;
        }

        true
    }

    fn on_physical_up(
        &mut self,
        btn: MouseButton,
        point: Point,
        time: u32,
        host: &mut impl TriggerHost,
    ) -> bool {
        if let Some(wait) = self.lr_wait {
            if btn == wait.button {
                self.lr_wait = None;
                host.kill_poll_timer();
                self.last_mouse_event = Some((btn, false));
                if point == wait.point {
                    host.send_click(btn, point, self.tags.click_tag());
                } else {
                    host.send_diff_point_click(
                        btn,
                        wait.point,
                        point,
                        self.tags.encode(KIND_PLAIN, 0),
                    );
                }
                return false;
            }
            if self.tracker.would_drop_lone_up(btn) {
                return true;
            }
            self.lr_wait = None;
            host.kill_poll_timer();
            self.last_flags.resent_down[wait.button.index()] = true;
            host.send_down(
                wait.button,
                wait.point,
                self.tags.encode(KIND_DOWN, wait.press_id),
            );
            self.last_mouse_event = Some((btn, false));
            host.send_up(btn, point, self.tags.encode(KIND_PLAIN, 0));
            return false;
        }

        if self.last_flags.suppressed_down[btn.index()] {
            self.last_flags.suppressed_down[btn.index()] = false;
            self.last_mouse_event = Some((btn, false));
            self.handle_suppressed_up_in_scroll(btn, point, time, host);
            return false;
        }

        if self.last_flags.resent_down[btn.index()] {
            self.last_mouse_event = Some((btn, false));
            return true;
        }

        if self.last_mouse_event == Some((btn, false)) {
            return true;
        }
        self.last_mouse_event = Some((btn, false));
        true
    }

    fn handle_suppressed_up_in_scroll(
        &mut self,
        btn: MouseButton,
        point: Point,
        time: u32,
        host: &mut impl TriggerHost,
    ) {
        let Some(scroll) = &mut self.scroll else {
            return;
        };
        if scroll.released {
            return;
        }
        match scroll.owner {
            ScrollOwner::MouseDouble if btn.is_lr() => {
                if !scroll.second_trigger_up {
                    scroll.second_trigger_up = true;
                } else if time.wrapping_sub(scroll.started_at) <= self.config.trigger.scroll_locktime_ms
                {
                    scroll.released = true;
                } else {
                    self.exit_scroll(host);
                }
            }
            ScrollOwner::MouseButton(owner_btn) if owner_btn == btn => {
                if scroll.is_drag_trigger {
                    let dragged = scroll.dragged_or_scrolled();
                    if dragged && self.config.trigger.dragged_lock {
                        scroll.released = true;
                    } else {
                        self.exit_scroll(host);
                        if !dragged {
                            host.send_click(btn, point, self.tags.click_tag());
                        }
                    }
                } else if time.wrapping_sub(scroll.started_at) <= self.config.trigger.scroll_locktime_ms
                {
                    scroll.released = true;
                } else {
                    self.exit_scroll(host);
                }
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, event: HookKeyEvent, host: &mut impl TriggerHost) -> HookDecision {
        let pass = self.handle_key_inner(event, host);
        if pass {
            HookDecision::Pass
        } else {
            HookDecision::Suppress
        }
    }

    fn handle_key_inner(&mut self, event: HookKeyEvent, host: &mut impl TriggerHost) -> bool {
        let Ok(vk) = usize::try_from(event.vk_code) else {
            return true;
        };
        if vk >= KEY_COUNT {
            return true;
        }

        if self.paused {
            return match event.action {
                KeyAction::Down => {
                    self.key_flags.kd_suppressed[vk] = false;
                    self.key_flags.kd_lone_up[vk] = false;
                    self.key_flags.kd_passed[vk] = true;
                    true
                }
                KeyAction::Up => {
                    if self.key_flags.kd_lone_up[vk] {
                        self.key_flags.kd_lone_up[vk] = false;
                        false
                    } else {
                        self.key_flags.kd_passed[vk] = false;
                        true
                    }
                }
            };
        }

        let is_trigger_key = self.config.trigger.keyboard_enabled()
            && event.vk_code == self.config.trigger.target_vk_code.vk_code();

        match event.action {
            KeyAction::Down => {
                if event.vk_code == VK_PAUSE && is_trigger_key && self.key_flags.kd_suppressed[vk] {
                    self.key_flags.kd_suppressed[vk] = false;
                    self.key_flags.last_key_event = Some((event.vk_code, KeyAction::Up));
                    if self.is_released_scroll() || self.is_pressed_keyboard_scroll() {
                        self.exit_scroll(host);
                    }
                    return false;
                }
                if event.vk_code == VK_PAUSE && is_trigger_key && self.key_flags.kd_passed[vk] {
                    self.key_flags.kd_passed[vk] = false;
                }

                if self.key_flags.kd_suppressed[vk] {
                    return false;
                }
                if self.key_flags.kd_passed[vk] {
                    return true;
                }

                self.key_flags.kd_lone_up[vk] = false;
                if is_trigger_key {
                    if self.is_released_scroll() || self.is_mouse_scroll() {
                        self.key_flags.last_key_event = Some((event.vk_code, KeyAction::Down));
                        self.exit_scroll(host);
                        self.key_flags.kd_suppressed[vk] = true;
                        return false;
                    }
                    if host.is_excluded(host.cursor_pos(), &self.config.exclusions) {
                        if event.vk_code != VK_PAUSE {
                            self.key_flags.kd_passed[vk] = true;
                        }
                        return true;
                    }
                    let scroll = ActiveScroll::new_immediate(
                        ScrollOwner::Keyboard,
                        event.time,
                        &self.config.scroll,
                    );
                    if !self.start_scroll(scroll, host) {
                        // Scroll mode could not start: the key passes as if it were not the
                        // trigger (its repeats and its up too)
                        if event.vk_code != VK_PAUSE {
                            self.key_flags.kd_passed[vk] = true;
                        }
                        return true;
                    }
                    self.key_flags.last_key_event = Some((event.vk_code, KeyAction::Down));
                    self.key_flags.kd_suppressed[vk] = true;
                    return false;
                }

                if self.is_released_scroll() {
                    self.exit_scroll(host);
                    self.key_flags.kd_suppressed[vk] = true;
                    return false;
                }

                self.key_flags.kd_passed[vk] = true;
                true
            }
            KeyAction::Up => {
                if self.key_flags.kd_suppressed[vk] {
                    self.key_flags.kd_suppressed[vk] = false;
                    if is_trigger_key {
                        self.key_flags.last_key_event = Some((event.vk_code, KeyAction::Up));
                        if self.is_pressed_keyboard_scroll() {
                            let started_at = self.scroll.as_ref().map_or(event.time, |s| s.started_at);
                            if event.time.wrapping_sub(started_at)
                                <= self.config.trigger.scroll_locktime_ms
                            {
                                if let Some(scroll) = &mut self.scroll {
                                    scroll.released = true;
                                }
                            } else {
                                self.exit_scroll(host);
                            }
                        }
                    }
                    return false;
                }

                if self.key_flags.kd_lone_up[vk] {
                    self.key_flags.kd_lone_up[vk] = false;
                    if is_trigger_key {
                        self.key_flags.last_key_event = Some((event.vk_code, KeyAction::Up));
                    }
                    return false;
                }

                if self.key_flags.kd_passed[vk] {
                    self.key_flags.kd_passed[vk] = false;
                    return true;
                }

                true
            }
        }
    }

    pub fn on_poll_timeout(&mut self, host: &mut impl TriggerHost) {
        host.kill_poll_timer();
        let Some(wait) = self.lr_wait.take() else {
            return;
        };
        if self.paused {
            return;
        }
        self.last_flags.resent_down[wait.button.index()] = true;
        host.send_down(
            wait.button,
            wait.point,
            self.tags.encode(KIND_DOWN, wait.press_id),
        );
    }

    pub fn on_raw_motion(&mut self, dx: i32, dy: i32, time: u32, host: &mut impl TriggerHost) {
        if self.paused || (dx == 0 && dy == 0) {
            return;
        }
        let Some(scroll) = &mut self.scroll else {
            return;
        };
        if (time.wrapping_sub(scroll.started_at) as i32) < 0 {
            return;
        }
        let events = scroll.session.process_raw_motion(dx, dy, &self.config.scroll);
        if !events.is_empty() {
            scroll.scrolled = true;
            scroll.session.on_drag_cursor_move(&self.config.scroll);
        }
        self.sync_cursor(host);
        for wheel_event in events {
            host.send_wheel(wheel_event, self.generation);
            host.notify_scroll_activity();
        }
    }

    pub fn set_paused(&mut self, paused: bool, host: &mut impl TriggerHost) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        host.set_sender_paused(paused);
        if paused {
            self.init_state(host);
        }
    }

    pub fn set_config(&mut self, config: EngineConfig, host: &mut impl TriggerHost) {
        let trigger_changed = self.config.trigger != config.trigger;
        self.config = config;
        if trigger_changed {
            self.init_state(host);
        } else if self.scroll.is_some() && !self.config.scroll.cursor_change {
            self.update_cursor_axis(None, host);
        }
    }

    pub fn init_state(&mut self, host: &mut impl TriggerHost) {
        if self.lr_wait.take().is_some() {
            host.kill_poll_timer();
        }
        self.exit_scroll(host);
        self.last_flags.reset();
        self.key_flags.suppressed_to_lone_up();
        self.last_mouse_event = None;
    }

    /// The keyboard hook has been installed (again, on every load of the properties): ups may have
    /// been missed while the old one was removed. The scroll mode the trigger key started would
    /// not end: it is ended. As .NET (KEventHandler.hookInstalled): if Windows had removed the old
    /// hook (old_hook_lost, e.g. for a timeout), the suppressed keys are forgotten; else they are
    /// suppressed for their up alone (the trigger key held included).
    pub fn on_keyboard_hook_installed(&mut self, old_hook_lost: bool, host: &mut impl TriggerHost) {
        if old_hook_lost {
            self.key_flags.forget_suppressed();
        } else {
            self.key_flags.suppressed_to_lone_up();
        }
        if self.is_keyboard_scroll() {
            self.exit_scroll(host);
        }
    }

    /// The keyboard hook has been removed (keyboard trigger off, or it could not be installed):
    /// its records are reset and the scroll mode of the trigger key is ended.
    pub fn on_keyboard_hook_removed(&mut self, host: &mut impl TriggerHost) {
        self.key_flags.reset();
        if self.is_keyboard_scroll() {
            self.exit_scroll(host);
        }
    }

    /// The properties have been loaded (Reload, switching, adding, deleting or resetting them):
    /// the scroll settings apply from the next scroll, so a scroll in progress is ended, as .NET
    /// does (Ctx.applyProperties). The trigger's up is still suppressed (its flags are kept).
    pub fn end_scroll_after_load(&mut self, host: &mut impl TriggerHost) {
        if self.scroll.is_some() {
            self.exit_scroll(host);
        }
    }

    pub fn shutdown(&mut self, host: &mut impl TriggerHost) {
        self.init_state(host);
    }

    /// false if Raw Input could not be registered: scroll mode is not started then (the moves
    /// would be suppressed with no scroll, and the mouse would seem to stop).
    fn start_scroll(&mut self, scroll: ActiveScroll, host: &mut impl TriggerHost) -> bool {
        if !host.start_raw_input() {
            return false;
        }
        self.scroll = Some(scroll);
        self.sync_cursor(host);
        true
    }

    fn exit_scroll(&mut self, host: &mut impl TriggerHost) {
        let was_scrolling = self.scroll.take().is_some();
        self.bump_generation(host);
        if was_scrolling {
            host.stop_raw_input();
        }
        self.update_cursor_axis(None, host);
    }

    fn bump_generation(&mut self, host: &mut impl TriggerHost) {
        self.generation = self.generation.wrapping_add(1);
        host.set_sender_generation(self.generation);
    }

    fn sync_cursor(&mut self, host: &mut impl TriggerHost) {
        let desired = self
            .scroll
            .as_ref()
            .and_then(|scroll| scroll.session.cursor_axis(&self.config.scroll));
        self.update_cursor_axis(desired, host);
    }

    fn update_cursor_axis(&mut self, desired: Option<ScrollAxis>, host: &mut impl TriggerHost) {
        if self.active_cursor_axis != desired {
            self.active_cursor_axis = desired;
            host.update_cursor(desired);
        }
    }

    fn is_released_scroll(&self) -> bool {
        self.scroll.as_ref().is_some_and(|s| s.released)
    }

    fn is_keyboard_scroll(&self) -> bool {
        self.scroll
            .as_ref()
            .is_some_and(|s| s.owner == ScrollOwner::Keyboard)
    }

    fn is_pressed_keyboard_scroll(&self) -> bool {
        self.scroll
            .as_ref()
            .is_some_and(|s| !s.released && s.owner == ScrollOwner::Keyboard)
    }

    fn is_mouse_scroll(&self) -> bool {
        self.scroll
            .as_ref()
            .is_some_and(|s| s.owner != ScrollOwner::Keyboard)
    }

    /// The scroll mode was started by this button as the single or drag trigger
    fn is_single_scroll_owner(&self, btn: MouseButton) -> bool {
        self.scroll
            .as_ref()
            .is_some_and(|s| s.owner == ScrollOwner::MouseButton(btn))
    }

    fn is_pressed_mouse_scroll(&self) -> bool {
        self.scroll
            .as_ref()
            .is_some_and(|s| !s.released && s.owner != ScrollOwner::Keyboard)
    }

    fn is_first_double_button(&self, btn: MouseButton) -> bool {
        match self.config.trigger.first_trigger {
            Trigger::Lr => btn.is_lr(),
            Trigger::Left => btn == MouseButton::Left,
            Trigger::Right => btn == MouseButton::Right,
            _ => false,
        }
    }

    fn is_second_double_button(&self, btn: MouseButton, first_btn: MouseButton) -> bool {
        match self.config.trigger.first_trigger {
            Trigger::Lr => first_btn.other_lr() == Some(btn),
            Trigger::Left => first_btn == MouseButton::Left && btn == MouseButton::Right,
            Trigger::Right => first_btn == MouseButton::Right && btn == MouseButton::Left,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum HostCall {
        StartRaw,
        StopRaw,
        SetTimer(u32),
        KillTimer,
        Cursor(Option<ScrollAxis>),
        NotifyActivity,
        Wheel(WheelEvent),
        Click(MouseButton, Point, usize),
        DiffClick(MouseButton, Point, Point, usize),
        Down(MouseButton, Point, usize),
        Up(MouseButton, Point, usize),
        SenderPaused(bool),
    }

    struct MockHost {
        calls: Vec<HostCall>,
        cursor: Point,
        escape_down: bool,
        modifier_down: bool,
        exclude_all: bool,
        excluded_points: Vec<Point>,
        raw_input_fails: bool,
    }

    impl MockHost {
        fn new() -> Self {
            Self {
                calls: Vec::new(),
                cursor: Point { x: 500, y: 400 },
                escape_down: false,
                modifier_down: false,
                exclude_all: false,
                excluded_points: Vec::new(),
                raw_input_fails: false,
            }
        }

        fn take_calls(&mut self) -> Vec<HostCall> {
            std::mem::take(&mut self.calls)
        }
    }

    impl TriggerHost for MockHost {
        fn start_raw_input(&mut self) -> bool {
            if self.raw_input_fails {
                return false;
            }
            self.calls.push(HostCall::StartRaw);
            true
        }
        fn stop_raw_input(&mut self) {
            self.calls.push(HostCall::StopRaw);
        }
        fn set_poll_timer(&mut self, timeout_ms: u32) {
            self.calls.push(HostCall::SetTimer(timeout_ms));
        }
        fn kill_poll_timer(&mut self) {
            self.calls.push(HostCall::KillTimer);
        }
        fn update_cursor(&mut self, axis: Option<ScrollAxis>) {
            self.calls.push(HostCall::Cursor(axis));
        }
        fn notify_scroll_activity(&mut self) {
            self.calls.push(HostCall::NotifyActivity);
        }
        fn send_wheel(&mut self, event: WheelEvent, _generation: u64) {
            self.calls.push(HostCall::Wheel(event));
        }
        fn send_click(&mut self, button: MouseButton, point: Point, extra_info: usize) {
            self.calls.push(HostCall::Click(button, point, extra_info));
        }
        fn send_diff_point_click(
            &mut self,
            button: MouseButton,
            down_point: Point,
            up_point: Point,
            extra_info: usize,
        ) {
            self.calls
                .push(HostCall::DiffClick(button, down_point, up_point, extra_info));
        }
        fn send_down(&mut self, button: MouseButton, point: Point, extra_info: usize) {
            self.calls.push(HostCall::Down(button, point, extra_info));
        }
        fn send_up(&mut self, button: MouseButton, point: Point, extra_info: usize) {
            self.calls.push(HostCall::Up(button, point, extra_info));
        }
        fn set_sender_paused(&mut self, paused: bool) {
            self.calls.push(HostCall::SenderPaused(paused));
        }
        fn set_sender_generation(&mut self, _generation: u64) {}
        fn cursor_pos(&self) -> Point {
            self.cursor
        }
        fn is_escape_down(&self) -> bool {
            self.escape_down
        }
        fn is_modifier_down(&self) -> bool {
            self.modifier_down
        }
        fn is_excluded(&self, point: Point, exclusions: &ExclusionMatcher) -> bool {
            if exclusions.is_empty() {
                return false;
            }
            self.exclude_all || self.excluded_points.contains(&point)
        }
    }

    fn base_engine(trigger: Trigger) -> (TriggerEngine, ResendTags, MockHost) {
        let mut cfg = Config::default();
        cfg.first_trigger = trigger;
        cfg.accel_table = false;
        cfg.vertical_threshold = 5;
        cfg.horizontal_threshold = 5;
        cfg.cursor_change = true;
        cfg.vh_adjuster_mode = false;
        let engine_cfg = EngineConfig::from_config(&cfg, &CustomTableState::default());
        let tags = ResendTags::next_unique();
        (TriggerEngine::new(engine_cfg, tags), tags, MockHost::new())
    }

    fn phys_mouse(kind: HookMouseKind, x: i32, y: i32, time: u32) -> HookMouseEvent {
        HookMouseEvent {
            kind,
            point: Point { x, y },
            time,
            injected: false,
            extra_info: 0,
        }
    }

    fn inj_mouse(kind: HookMouseKind, x: i32, y: i32, time: u32, extra_info: usize) -> HookMouseEvent {
        HookMouseEvent {
            kind,
            point: Point { x, y },
            time,
            injected: true,
            extra_info,
        }
    }

    #[test]
    fn lr_double_trigger_and_scroll_locktime_released_mode() {
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);

        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 120), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
        assert!(host.take_calls().contains(&HostCall::StartRaw));

        // First button up keeps scroll active; second up within scrollLocktime(200ms) switches to Released mode.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 150), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 10, 10, 220), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());

        // Scrolling continues while Released.
        engine.on_raw_motion(0, 20, 230, &mut host);
        assert!(host.take_calls().iter().any(|c| matches!(c, HostCall::Wheel(_))));

        // Clicking Left exits Released mode and suppresses both Down and Up.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 300), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 340), &mut host),
            HookDecision::Suppress
        );
        assert!(host.take_calls().contains(&HostCall::StopRaw));
    }

    #[test]
    fn lr_wait_timeout_resends_down_and_handles_fast_physical_up_race() {
        let (mut engine, tags, mut host) = base_engine(Trigger::Lr);

        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 50, 60, 100), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(host.take_calls(), vec![HostCall::SetTimer(200)]);

        engine.on_poll_timeout(&mut host);
        let calls = host.take_calls();
        let down_extra = match calls.as_slice() {
            [HostCall::KillTimer, HostCall::Down(MouseButton::Left, Point { x: 50, y: 60 }, extra)] => *extra,
            other => panic!("unexpected timeout calls: {other:?}"),
        };

        // Simulate physical Up arriving BEFORE resent KIND_DOWN reaches the hook.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 50, 60, 310), &mut host),
            HookDecision::Suppress
        );
        let rel_calls = host.take_calls();
        let rel_extra = match rel_calls.as_slice() {
            [HostCall::Up(MouseButton::Left, Point { x: 50, y: 60 }, extra)] => *extra,
            other => panic!("unexpected release calls: {other:?}"),
        };
        assert_eq!(
            tags.decode(rel_extra),
            Some(ResentInfo::Button {
                kind: KIND_RELEASE,
                press_id: 1
            })
        );

        // Both KIND_DOWN(1) and KIND_RELEASE(1) must now be allowed to pass.
        assert_eq!(
            engine.handle_mouse(
                inj_mouse(HookMouseKind::Down(MouseButton::Left), 50, 60, 311, down_extra),
                &mut host
            ),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(
                inj_mouse(HookMouseKind::Up(MouseButton::Left), 50, 60, 321, rel_extra),
                &mut host
            ),
            HookDecision::Pass
        );
    }

    #[test]
    fn left_only_double_trigger_ignores_right_first() {
        let (mut engine, _tags, mut host) = base_engine(Trigger::Left);

        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 100), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 10, 10, 110), &mut host),
            HookDecision::Pass
        );
        assert!(!engine.is_scrolling());

        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 200), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 220), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
    }

    #[test]
    fn drag_trigger_dragged_lock_vs_click_resend() {
        let mut cfg = Config::default();
        cfg.first_trigger = Trigger::RightDrag;
        cfg.dragged_lock = true;
        cfg.accel_table = false;
        cfg.vertical_threshold = 5;
        cfg.horizontal_threshold = 5;
        cfg.cursor_change = true;
        cfg.vh_adjuster_mode = false;
        let tags = ResendTags::next_unique();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            tags,
        );
        let mut host = MockHost::new();

        // Click without moving -> exits and resends Right click.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 100), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 10, 10, 150), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        assert!(host.take_calls().contains(&HostCall::Click(
            MouseButton::Right,
            Point { x: 10, y: 10 },
            tags.click_tag()
        )));

        // Any physical cursor move prevents click replay even without wheel output.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 200), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Move, 11, 10, 210), &mut host),
            HookDecision::Suppress
        );
        engine.on_raw_motion(0, 15, 220, &mut host);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 11, 25, 250), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
    }

    #[test]
    fn middle_trigger_modifier_click_and_escape_reset() {
        let mut cfg = Config::default();
        cfg.first_trigger = Trigger::Middle;
        cfg.send_middle_click = true;
        cfg.accel_table = false;
        cfg.vertical_threshold = 5;
        cfg.cursor_change = true;
        cfg.vh_adjuster_mode = false;
        let tags = ResendTags::next_unique();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            tags,
        );
        let mut host = MockHost::new();
        host.modifier_down = true;

        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Middle), 30, 40, 100), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Middle), 30, 40, 130), &mut host),
            HookDecision::Suppress
        );
        assert!(host.take_calls().contains(&HostCall::Click(
            MouseButton::Middle,
            Point { x: 30, y: 40 },
            tags.click_tag()
        )));

        // Escape + Up recovers cleanly from active scroll without popping stray Up.
        host.modifier_down = false;
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Middle), 30, 40, 200), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
        host.escape_down = true;
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Middle), 30, 40, 500), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        assert!(host.take_calls().contains(&HostCall::StopRaw));
    }

    #[test]
    fn keyboard_trigger_pause_key_and_mouse_mutual_exit() {
        let mut cfg = Config::default();
        cfg.first_trigger = Trigger::Middle;
        cfg.keyboard_hook = true;
        cfg.target_vk_code = TriggerKey::VkPause;
        let tags = ResendTags::next_unique();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            tags,
        );
        let mut host = MockHost::new();

        // VK_PAUSE Down starts keyboard scroll; second VK_PAUSE Down exits even without KeyUp.
        assert_eq!(
            engine.handle_key(
                HookKeyEvent {
                    vk_code: VK_PAUSE,
                    action: KeyAction::Down,
                    time: 100
                },
                &mut host
            ),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_key(
                HookKeyEvent {
                    vk_code: VK_PAUSE,
                    action: KeyAction::Down,
                    time: 400
                },
                &mut host
            ),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());

        // Start mouse Middle scroll, then press trigger key to cancel mouse scroll cleanly.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Middle), 0, 0, 500), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_key(
                HookKeyEvent {
                    vk_code: VK_PAUSE,
                    action: KeyAction::Down,
                    time: 600
                },
                &mut host
            ),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
    }

    #[test]
    fn excluded_windows_pass_all_triggers_but_released_scroll_still_exits() {
        use crate::exclusions::ExclusionList;

        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let excluded_pt = Point { x: 100, y: 100 };
        let normal_pt = Point { x: 20, y: 20 };

        // 1. RightDrag passes on excluded point without starting scroll or resending click.
        let mut cfg = Config::default();
        cfg.first_trigger = Trigger::RightDrag;
        cfg.dragged_lock = true;
        cfg.accel_table = false;
        cfg.vertical_threshold = 5;
        let tags = ResendTags::next_unique();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default())
                .with_exclusions(matcher.clone()),
            tags,
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);

        assert_eq!(
            engine.handle_mouse(
                phys_mouse(HookMouseKind::Down(MouseButton::Right), 100, 100, 10),
                &mut host
            ),
            HookDecision::Pass
        );
        assert!(!engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(
                phys_mouse(HookMouseKind::Up(MouseButton::Right), 100, 100, 20),
                &mut host
            ),
            HookDecision::Pass
        );
        assert!(host.take_calls().is_empty());

        // Start Drag scroll on normal point -> enter Released mode -> clicking excluded point STILL exits Released mode.
        assert_eq!(
            engine.handle_mouse(
                phys_mouse(HookMouseKind::Down(MouseButton::Right), 20, 20, 30),
                &mut host
            ),
            HookDecision::Suppress
        );
        engine.on_raw_motion(0, 15, 40, &mut host);
        assert_eq!(
            engine.handle_mouse(
                phys_mouse(HookMouseKind::Up(MouseButton::Right), 20, 20, 50),
                &mut host
            ),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(
                phys_mouse(HookMouseKind::Down(MouseButton::Left), 100, 100, 60),
                &mut host
            ),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(
                phys_mouse(HookMouseKind::Up(MouseButton::Left), 100, 100, 70),
                &mut host
            ),
            HookDecision::Suppress
        );

        // 2. Middle trigger with sendMiddleClick passes directly on excluded window.
        cfg.first_trigger = Trigger::Middle;
        cfg.send_middle_click = true;
        let mut mid_engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default())
                .with_exclusions(matcher.clone()),
            tags,
        );
        host.modifier_down = true;
        host.take_calls();
        assert_eq!(
            mid_engine.handle_mouse(
                phys_mouse(HookMouseKind::Down(MouseButton::Middle), 100, 100, 100),
                &mut host
            ),
            HookDecision::Pass
        );
        assert!(host.take_calls().is_empty());

        // 3. LR trigger does not start wait timer on excluded window.
        cfg.first_trigger = Trigger::Lr;
        let mut lr_engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default())
                .with_exclusions(matcher.clone()),
            tags,
        );
        assert_eq!(
            lr_engine.handle_mouse(
                phys_mouse(HookMouseKind::Down(MouseButton::Left), 100, 100, 100),
                &mut host
            ),
            HookDecision::Pass
        );
        assert!(host.take_calls().is_empty());

        // 4. Keyboard trigger (including VK_PAUSE without KeyUp) passes when cursor is over excluded window,
        //    then works normally when cursor moves back to a non-excluded window.
        cfg.first_trigger = Trigger::None;
        cfg.keyboard_hook = true;
        cfg.target_vk_code = TriggerKey::VkPause;
        let mut kb_engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            tags,
        );
        host.cursor = excluded_pt;
        assert_eq!(
            kb_engine.handle_key(
                HookKeyEvent {
                    vk_code: VK_PAUSE,
                    action: KeyAction::Down,
                    time: 200
                },
                &mut host
            ),
            HookDecision::Pass
        );
        assert!(!kb_engine.is_scrolling());

        host.cursor = normal_pt;
        assert_eq!(
            kb_engine.handle_key(
                HookKeyEvent {
                    vk_code: VK_PAUSE,
                    action: KeyAction::Down,
                    time: 300
                },
                &mut host
            ),
            HookDecision::Suppress
        );
        assert!(kb_engine.is_scrolling());
    }

    fn key(vk: u32, action: KeyAction, time: u32) -> HookKeyEvent {
        HookKeyEvent {
            vk_code: vk,
            action,
            time,
        }
    }

    #[test]
    fn raw_input_refused_lets_every_trigger_work_as_a_plain_button() {
        // Drag and single triggers: down and up pass, no scroll mode.
        for trigger in [Trigger::RightDrag, Trigger::Middle] {
            let (mut engine, _tags, mut host) = base_engine(trigger);
            host.raw_input_fails = true;
            let btn = if trigger == Trigger::Middle {
                MouseButton::Middle
            } else {
                MouseButton::Right
            };
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Down(btn), 10, 10, 100), &mut host),
                HookDecision::Pass
            );
            assert!(!engine.is_scrolling());
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Move, 30, 30, 110), &mut host),
                HookDecision::Pass
            );
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Up(btn), 30, 30, 120), &mut host),
                HookDecision::Pass
            );
            assert!(host.take_calls().is_empty());
        }

        // LR trigger: both downs are resent in order as presses of their own.
        let (mut engine, tags, mut host) = base_engine(Trigger::Lr);
        host.raw_input_fails = true;
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 1, 2, 100), &mut host);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 1, 2, 120), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        let calls = host.take_calls();
        let downs: Vec<_> = calls
            .iter()
            .filter_map(|c| match c {
                HostCall::Down(b, _, extra) => Some((*b, tags.decode(*extra))),
                _ => None,
            })
            .collect();
        assert_eq!(
            downs,
            vec![
                (MouseButton::Left, Some(ResentInfo::Button { kind: KIND_DOWN, press_id: 1 })),
                (MouseButton::Right, Some(ResentInfo::Button { kind: KIND_DOWN, press_id: 2 })),
            ]
        );
        // The resent downs reach the hook, then the physical ups pass (releasing them).
        for (btn, id) in [(MouseButton::Left, 1), (MouseButton::Right, 2)] {
            assert_eq!(
                engine.handle_mouse(
                    inj_mouse(HookMouseKind::Down(btn), 1, 2, 121, tags.encode(KIND_DOWN, id)),
                    &mut host
                ),
                HookDecision::Pass
            );
        }
        for btn in [MouseButton::Left, MouseButton::Right] {
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Up(btn), 1, 2, 150), &mut host),
                HookDecision::Pass
            );
        }

        // Keyboard trigger: the key passes, and so do its repeats and its up.
        let mut cfg = Config::default();
        cfg.first_trigger = Trigger::None;
        cfg.keyboard_hook = true;
        cfg.target_vk_code = TriggerKey::VkLmenu;
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        let vk = TriggerKey::VkLmenu.vk_code();
        for (action, time) in [(KeyAction::Down, 10), (KeyAction::Down, 40), (KeyAction::Up, 60)] {
            assert_eq!(kb.handle_key(key(vk, action, time), &mut host), HookDecision::Pass);
        }
        assert!(!kb.is_scrolling());
    }

    #[test]
    fn lr_pair_button_pressed_again_after_its_up_passes_and_scroll_goes_on() {
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 120), &mut host);
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 400), &mut host),
            HookDecision::Suppress
        );

        // Left clicked again while Right holds the scroll: the click passes, scrolling goes on.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 500), &mut host),
            HookDecision::Pass
        );
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 520), &mut host),
            HookDecision::Pass
        );
        assert!(engine.is_scrolling());

        // Right up (after scrollLocktime) ends it, suppressed as its down was.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 10, 10, 900), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());

        // A missed up is still detected: Right down again while its earlier down is suppressed.
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 1000), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 1020), &mut host);
        assert!(engine.is_scrolling());
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 1300), &mut host);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 1400), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
    }

    #[test]
    fn trigger_key_released_while_paused_is_suppressed_as_a_lone_up() {
        let mut cfg = Config::default();
        cfg.keyboard_hook = true;
        cfg.target_vk_code = TriggerKey::VkLmenu;
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        let vk = TriggerKey::VkLmenu.vk_code();

        assert_eq!(engine.handle_key(key(vk, KeyAction::Down, 10), &mut host), HookDecision::Suppress);
        assert!(engine.is_scrolling());
        engine.set_paused(true, &mut host);
        assert!(!engine.is_scrolling());
        // Its up alone would reach the application (Alt would activate the menu bar).
        assert_eq!(engine.handle_key(key(vk, KeyAction::Up, 500), &mut host), HookDecision::Suppress);

        // Pressed in Pause and held after it: its repeats and up pass, no scroll starts.
        assert_eq!(engine.handle_key(key(vk, KeyAction::Down, 600), &mut host), HookDecision::Pass);
        engine.set_paused(false, &mut host);
        assert_eq!(engine.handle_key(key(vk, KeyAction::Down, 650), &mut host), HookDecision::Pass);
        assert!(!engine.is_scrolling());
        assert_eq!(engine.handle_key(key(vk, KeyAction::Up, 700), &mut host), HookDecision::Pass);
        assert_eq!(engine.handle_key(key(vk, KeyAction::Down, 800), &mut host), HookDecision::Suppress);
        assert!(engine.is_scrolling());
    }

    #[test]
    fn load_ends_scroll_and_hook_reinstall_recovers_keys() {
        // A load during a drag scroll ends it; the trigger's up stays suppressed, no click.
        let (mut engine, _tags, mut host) = base_engine(Trigger::RightDrag);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 100), &mut host);
        assert!(engine.is_scrolling());
        engine.end_scroll_after_load(&mut host);
        assert!(!engine.is_scrolling());
        assert!(host.take_calls().contains(&HostCall::StopRaw));
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 10, 10, 200), &mut host),
            HookDecision::Suppress
        );
        assert!(!host.take_calls().iter().any(|c| matches!(c, HostCall::Click(..))));

        let mut cfg = Config::default();
        cfg.keyboard_hook = true;
        cfg.target_vk_code = TriggerKey::VkLmenu;
        let vk = TriggerKey::VkLmenu.vk_code();

        // The old hook was alive: the trigger key held ends its scroll, its up alone is suppressed.
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        kb.handle_key(key(vk, KeyAction::Down, 10), &mut host);
        assert!(kb.is_scrolling());
        kb.on_keyboard_hook_installed(false, &mut host);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 50), &mut host), HookDecision::Suppress);

        // Windows had removed the old hook: the key may have been released and pressed again
        // meanwhile, so its records are forgotten and its up passes.
        let mut lost = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        lost.handle_key(key(vk, KeyAction::Down, 10), &mut host);
        lost.on_keyboard_hook_installed(true, &mut host);
        assert!(!lost.is_scrolling());
        assert_eq!(lost.handle_key(key(vk, KeyAction::Up, 50), &mut host), HookDecision::Pass);
        assert_eq!(lost.handle_key(key(vk, KeyAction::Down, 90), &mut host), HookDecision::Suppress);
        assert!(lost.is_scrolling());
    }
}
