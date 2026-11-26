use std::collections::VecDeque;

use crate::config::{Config, CustomTableState, Trigger, TriggerKey};
use crate::exclusions::ExclusionMatcher;
use crate::properties::ConfigStore;
use crate::scroll::{ScrollAxis, ScrollConfig, ScrollSession, WheelEvent};
use crate::sender::{MouseButton, PRESS_ID_MASK, Point, ResendKind, ResendTags, ResentInfo};

const KEY_COUNT: usize = 256;
/// More presses than a sender held back that long could have queued
const MAX_RELEASES_QUEUED: usize = 16;

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
    /// Whether the key is down now (GetAsyncKeyState)
    fn is_key_down(&self, vk_code: u32) -> bool;
    fn is_excluded(&self, point: Point, exclusions: &ExclusionMatcher) -> bool;
}

/// What reached the application of a button's latest physical press, so that no down reaches it
/// alone (the button left down there) and no up alone (of a down it never had)
#[derive(Clone, Debug, Default)]
struct Press {
    /// The id of the press, told by its resent events (ResendTags)
    id: u32,
    /// Its physical down was suppressed
    down_suppressed: bool,
    /// The id of the press whose down reached the application (physical, or resent), 0 when none
    /// did or its up reached it too
    down_passed_id: u32,
    /// Its up has passed (or was dropped as a lone up, or may have been missed): a down of it
    /// resent later is dropped
    up_passed: bool,
    /// The ids of the presses whose resent up (ResendKind::Release) is queued for their down
    /// resent alone, in order until each up reaches the hook (evicting the oldest past
    /// MAX_RELEASES_QUEUED)
    releases_queued: VecDeque<u32>,
}

#[derive(Default)]
struct ButtonTracker {
    last_press_id: u32,
    presses: [Press; MouseButton::COUNT],
}

impl ButtonTracker {
    fn press(&self, button: MouseButton) -> &Press {
        &self.presses[button.index()]
    }

    fn press_mut(&mut self, button: MouseButton) -> &mut Press {
        &mut self.presses[button.index()]
    }

    fn drop_resent(&mut self, button: MouseButton, is_down: bool, kind: ResendKind, press_id: u32) -> bool {
        let p = self.press_mut(button);
        match (is_down, kind) {
            (true, ResendKind::Down) => {
                (press_id != p.id || p.up_passed) && !p.has_queued_release(press_id)
            }
            (false, ResendKind::Release) => {
                p.remove_queued_release(press_id);
                press_id != p.down_passed_id
            }
            _ => false,
        }
    }

    fn on_before_physical(&mut self, button: MouseButton, is_down: bool) {
        if !is_down {
            return;
        }
        self.last_press_id = (self.last_press_id % PRESS_ID_MASK) + 1;
        let id = self.last_press_id;
        let p = self.press_mut(button);
        p.id = id;
        p.up_passed = false;
        p.down_suppressed = false;
    }

    fn current_press_id(&self, button: MouseButton) -> u32 {
        self.press(button).id
    }

    fn would_drop_lone_up(&self, button: MouseButton) -> bool {
        let p = self.press(button);
        p.down_suppressed && p.down_passed_id != p.id
    }

    fn drop_lone_up(&mut self, button: MouseButton) -> bool {
        if self.would_drop_lone_up(button) {
            self.press_mut(button).up_passed = true;
            true
        } else {
            false
        }
    }

    /// The buttons' ups may have been missed (no hook saw them; the physical up reached the
    /// application then). The suppressed downs are forgotten, so the up of a press made meanwhile
    /// (which reached the application too) is not suppressed as theirs. A resent down still
    /// queued for a press whose release was not queued is dropped when it comes (as after a
    /// passed up), or it would reach the application alone and the button stay down; ones whose
    /// release is queued pass with it (the clicks are whole). The press ids are kept (resent
    /// events in flight are still told).
    fn forget_presses(&mut self) {
        for p in &mut self.presses {
            p.down_suppressed = false;
            if !p.has_queued_release(p.id) {
                p.up_passed = true;
            }
        }
    }

    fn mark_release_queued(&mut self, button: MouseButton, press_id: u32) {
        self.press_mut(button).queue_release(press_id);
    }

    /// After a physical event, passed or suppressed
    fn on_after_physical(&mut self, button: MouseButton, is_down: bool, passed: bool) {
        let p = self.press_mut(button);
        match (is_down, passed) {
            (true, true) => p.down_passed_id = p.id,
            (true, false) => p.down_suppressed = true,
            (false, true) => {
                p.down_passed_id = 0;
                p.up_passed = true;
                p.down_suppressed = false;
            }
            (false, false) => p.down_suppressed = false,
        }
    }

    /// After a resent event of ours that passed (one dropped by drop_resent did not)
    fn on_after_resent(&mut self, button: MouseButton, is_down: bool, kind: ResendKind, press_id: u32) {
        let p = self.press_mut(button);
        match (is_down, kind) {
            (true, ResendKind::Down) => p.down_passed_id = press_id,
            (false, ResendKind::Release) => p.down_passed_id = 0,
            _ => {}
        }
    }
}

impl Press {
    fn queue_release(&mut self, id: u32) {
        self.remove_queued_release(id);
        self.releases_queued.push_back(id);
        while self.releases_queued.len() > MAX_RELEASES_QUEUED {
            self.releases_queued.pop_front();
        }
    }

    fn remove_queued_release(&mut self, id: u32) {
        if let Some(pos) = self.releases_queued.iter().position(|&queued| queued == id) {
            self.releases_queued.remove(pos);
        }
    }

    fn has_queued_release(&self, id: u32) -> bool {
        self.releases_queued.contains(&id)
    }
}

/// Who has the up of a button's down, from the down to its up (or a reset): kept by the triggers,
/// beside what reached the application (ButtonTracker). Set at most for the press of the down:
/// none is left at the next physical down, nor after the physical up (finish_physical_mouse).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum UpOwner {
    /// No trigger: the up is handled as it comes
    #[default]
    None,
    /// A trigger suppressed its down: its up is suppressed too, and is the trigger's (it may end
    /// or release the scroll)
    Trigger,
    /// Its down was resent (the LR wait ended with no pair): its up passes, or its release is
    /// resent if the up is dropped as a lone up
    Resent,
}

#[derive(Default)]
struct UpOwners([UpOwner; MouseButton::COUNT]);

impl UpOwners {
    fn get(&self, button: MouseButton) -> UpOwner {
        self.0[button.index()]
    }

    fn set(&mut self, button: MouseButton, owner: UpOwner) {
        self.0[button.index()] = owner;
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    /// The owner of the up, none after this (the up has come)
    fn take(&mut self, button: MouseButton) -> UpOwner {
        std::mem::take(&mut self.0[button.index()])
    }
}

/// What is known of a key's press, for its repeats and its up
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum KeyState {
    /// Not pressed, as far as is known: its next down is a new press
    #[default]
    Idle,
    /// Its down was suppressed: its repeats and its up are too
    Suppressed,
    /// Its down passed: its repeats and its up pass too
    Passed,
    /// Its down was suppressed, then the triggers were reset (init_state): its up alone is still
    /// suppressed, a new down is a new press
    LoneUp,
}

struct KeyStates([KeyState; KEY_COUNT]);

impl Default for KeyStates {
    fn default() -> Self {
        Self([KeyState::Idle; KEY_COUNT])
    }
}

impl KeyStates {
    fn get(&self, vk: usize) -> KeyState {
        self.0[vk]
    }

    fn set(&mut self, vk: usize, state: KeyState) {
        self.0[vk] = state;
    }

    fn suppressed_to_lone_up(&mut self) {
        for state in &mut self.0 {
            if *state == KeyState::Suppressed {
                *state = KeyState::LoneUp;
            }
        }
    }

    /// The keyboard hook is removed, or keys were not seen (the session locked): the ups will not
    /// be seen, or were missed
    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug)]
struct LrWait {
    button: MouseButton,
    point: Point,
    press_id: u32,
    // The time of the waiting down (of the hook): an event later than pollTimeout after it is not
    // its pair, whether WM_TIMER has come yet or not (see expire_lr_wait_if_due)
    time: u32,
}

/// The trigger that started a scroll, and what is kept of it until its up
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollKind {
    /// A single trigger (Middle, X1, X2): its up releases the scroll within the locktime
    Single(MouseButton),
    /// A drag trigger: its up releases the scroll if it moved (draggedLock), or ends it (with the
    /// click resent if it did not)
    Drag {
        button: MouseButton,
        start: Point,
        /// A move away from the start seen by the hook while pressed, or a scroll
        moved: bool,
    },
    /// The pair of the LR, Left and Right triggers: the second up releases the scroll within the
    /// locktime
    Double { first_up_seen: bool },
    /// The keyboard trigger: its up releases the scroll within the locktime
    Keyboard,
}

impl ScrollKind {
    /// The button of a single or drag trigger
    fn button(self) -> Option<MouseButton> {
        match self {
            Self::Single(button) | Self::Drag { button, .. } => Some(button),
            Self::Double { .. } | Self::Keyboard => None,
        }
    }
}

struct ActiveScroll {
    kind: ScrollKind,
    /// The trigger was released within the locktime (or after a drag, draggedLock): the scroll
    /// goes on until a next down
    released: bool,
    /// The time of the trigger's down: an earlier move (Raw Input) is not of the scroll
    started_at: u32,
    session: ScrollSession,
}

impl ActiveScroll {
    fn new_immediate(kind: ScrollKind, started_at: u32, config: &ScrollConfig) -> Self {
        let mut session = ScrollSession::new(config);
        session.on_immediate_scroll_start(config);
        Self {
            kind,
            released: false,
            started_at,
            session,
        }
    }

    fn new_drag(button: MouseButton, start: Point, started_at: u32, config: &ScrollConfig) -> Self {
        Self {
            kind: ScrollKind::Drag {
                button,
                start,
                moved: false,
            },
            released: false,
            started_at,
            session: ScrollSession::new(config),
        }
    }

    fn is_keyboard(&self) -> bool {
        self.kind == ScrollKind::Keyboard
    }
}

/// The scroll a reset ends, if one is going on
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndScroll {
    /// Started by a mouse trigger
    Mouse,
    /// Started by the keyboard trigger
    Keyboard,
    Any,
}

/// What a reset does with the records of the mouse buttons
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseRecords {
    Keep,
    /// The LR wait is ended, and the triggers' records of the buttons (UpOwners) are reset. The
    /// presses are still known (ButtonTracker): the up of a suppressed down is still suppressed.
    Reset,
    /// As Reset, and the presses are forgotten too (ButtonTracker::forget_presses): their ups may
    /// have been missed.
    Forget,
}

/// What a reset does with the records of the keys (KeyStates)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyRecords {
    Keep,
    /// The up of a suppressed key is still suppressed, alone (KeyState::LoneUp)
    LoneUp,
    /// All forgotten: the ups may have been missed, or will not be seen
    Clear,
}

/// What an event ends and forgets, all in one place:
///
/// | Reset              | scroll   | mouse  | keys   |
/// |--------------------|----------|--------|--------|
/// | INIT_STATE         | Any      | Reset  | LoneUp |
/// | INPUT_LOST         | Any      | Forget | Clear  |
/// | MOUSE_HOOK_LOST    | Mouse    | Forget | Keep   |
/// | KEYBOARD_HOOK_GONE | Keyboard | Keep   | Clear  |
/// | LOAD               | Any      | Keep   | Keep   |
#[derive(Clone, Copy, Debug)]
struct Reset {
    scroll: EndScroll,
    mouse: MouseRecords,
    keys: KeyRecords,
}

impl Reset {
    /// init_state: all is ended, and nothing seen was missed
    const INIT_STATE: Self = Self {
        scroll: EndScroll::Any,
        mouse: MouseRecords::Reset,
        keys: KeyRecords::LoneUp,
    };
    /// on_input_lost: a lock, an unlock, a suspend or a resume (none of the input was seen)
    const INPUT_LOST: Self = Self {
        scroll: EndScroll::Any,
        mouse: MouseRecords::Forget,
        keys: KeyRecords::Clear,
    };
    /// on_mouse_hook_lost: Windows had removed the mouse hook (its events were missed)
    const MOUSE_HOOK_LOST: Self = Self {
        scroll: EndScroll::Mouse,
        mouse: MouseRecords::Forget,
        keys: KeyRecords::Keep,
    };
    /// on_keyboard_hook_lost and on_keyboard_hook_removed: no keyboard hook saw the keys, or will
    /// see them
    const KEYBOARD_HOOK_GONE: Self = Self {
        scroll: EndScroll::Keyboard,
        mouse: MouseRecords::Keep,
        keys: KeyRecords::Clear,
    };
    /// end_scroll_after_load: the scroll settings apply from the next scroll; the trigger's up is
    /// still suppressed
    const LOAD: Self = Self {
        scroll: EndScroll::Any,
        mouse: MouseRecords::Keep,
        keys: KeyRecords::Keep,
    };
}

pub struct TriggerEngine {
    config: EngineConfig,
    tags: ResendTags,
    paused: bool,
    generation: u64,
    tracker: ButtonTracker,
    up_owners: UpOwners,
    keys: KeyStates,
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
            up_owners: UpOwners::default(),
            keys: KeyStates::default(),
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

    /// Whether the keyboard hook is to be installed: the keyboard trigger is on, and not paused
    /// (no keyboard hook while paused: nothing of the keyboard is seen nor kept then)
    pub fn keyboard_hook_enabled(&self) -> bool {
        self.config.trigger.keyboard_enabled() && !self.paused
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
        if !event.injected {
            self.expire_lr_wait_if_due(event.time, host);
        }
        if let Some(wait) = self.lr_wait {
            if !event.injected && event.point != wait.point {
                self.end_lr_wait(host);
                self.resend_waiting_down(wait, host);
                return false;
            }
            return true;
        }
        if let Some(scroll) = &mut self.scroll {
            if !scroll.released
                && let ScrollKind::Drag { start, moved, .. } = &mut scroll.kind
                && !event.injected
                && event.point != *start
            {
                *moved = true;
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
                        self.tracker.on_after_resent(btn, is_down, kind, press_id);
                        true
                    }
                }
                None => true,
            };
        }

        // Before this event, as the timer would have been: it comes after the wait has timed out
        self.expire_lr_wait_if_due(event.time, host);

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
        if !is_down {
            let owner = self.up_owners.take(btn);
            if passed && self.tracker.drop_lone_up(btn) {
                passed = false;
                if owner == UpOwner::Resent {
                    let press_id = self.tracker.current_press_id(btn);
                    self.tracker.mark_release_queued(btn, press_id);
                    host.send_up(btn, point, self.tags.encode(ResendKind::Release, press_id));
                }
            }
        }
        self.tracker.on_after_physical(btn, is_down, passed);
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
                self.end_lr_wait(host);
            } else if self.is_second_double_button(btn, wait.button) {
                self.end_lr_wait(host);
                self.up_owners.set(wait.button, UpOwner::Trigger);
                self.up_owners.set(btn, UpOwner::Trigger);
                if self.is_released_scroll() || self.is_keyboard_scroll() {
                    self.exit_scroll(host);
                } else if !self.start_scroll(
                    ActiveScroll::new_immediate(ScrollKind::Double { first_up_seen: false }, time, &self.config.scroll),
                    host,
                ) {
                    // Scroll mode could not start (Raw Input refused): both downs are resent in
                    // order, as presses of their own, so that no click is lost and no move is
                    // suppressed without a scroll. Their ups are handled as those of resent downs.
                    let press_id = self.tracker.current_press_id(btn);
                    for (button, at, id) in
                        [(wait.button, wait.point, wait.press_id), (btn, point, press_id)]
                    {
                        self.up_owners.set(button, UpOwner::Resent);
                        host.send_down(button, at, self.tags.encode(ResendKind::Down, id));
                    }
                }
                return false;
            } else {
                self.end_lr_wait(host);
                self.resend_waiting_down(wait, host);
            }
        }

        // A new press: what the triggers kept of the earlier one goes (its up was missed, if any
        // is left)
        let up_of_trigger = self.up_owners.take(btn) == UpOwner::Trigger;
        // A down again whose earlier press held scroll mode (its up was missed): the press of the
        // single or drag trigger, or one of the pair of the LR, Left and Right triggers whose down
        // is still suppressed. Not a button of the pair pressed again after its up was seen (the
        // other button still holds the scroll): that press passes below.
        let held = self.is_single_scroll_owner(btn)
            || (self.config.trigger.first_trigger.is_double() && btn.is_lr() && up_of_trigger);
        if held
            && self.is_pressed_mouse_scroll()
            && let Some(scroll) = &mut self.scroll
        {
            scroll.released = true;
        }

        if self.is_released_scroll() {
            self.exit_scroll(host);
            self.up_owners.set(btn, UpOwner::Trigger);
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
                    time,
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
                self.up_owners.set(btn, UpOwner::Trigger);
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
                self.up_owners.set(btn, UpOwner::Trigger);
                return false;
            }
            let scroll = ActiveScroll::new_immediate(
                ScrollKind::Single(btn),
                time,
                &self.config.scroll,
            );
            if !self.start_scroll(scroll, host) {
                // Scroll mode could not start: the button works as if it were not the trigger
                return true;
            }
            self.up_owners.set(btn, UpOwner::Trigger);
            return false;
        }

        if let Some(trigger_btn) = MouseButton::drag_from_trigger(trigger) {
            if btn != trigger_btn {
                return true;
            }
            if self.is_keyboard_scroll() {
                self.exit_scroll(host);
                self.up_owners.set(btn, UpOwner::Trigger);
                return false;
            }
            if host.is_excluded(point, &self.config.exclusions) {
                return true;
            }
            let scroll = ActiveScroll::new_drag(btn, point, time, &self.config.scroll);
            if !self.start_scroll(scroll, host) {
                return true;
            }
            self.up_owners.set(btn, UpOwner::Trigger);
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
                self.end_lr_wait(host);
                if point == wait.point {
                    host.send_click(btn, point, self.tags.click_tag());
                } else {
                    host.send_diff_point_click(
                        btn,
                        wait.point,
                        point,
                        self.tags.encode(ResendKind::Plain, 0),
                    );
                }
                return false;
            }
            // The up of a down resent alone (its wait timed out), or a lone up (of a down that
            // reached no application), while the other button waits: on to finish_physical_mouse
            // (released, or dropped), and the wait goes on, as .NET does (lrUp: checkResentDown
            // before offerEventWaiter)
            if self.up_owners.get(btn) == UpOwner::Resent || self.tracker.would_drop_lone_up(btn) {
                return true;
            }
            // The up of another button whose down reached the application (another button than
            // the pair's, the other of the pair passed over an excluded window, the other button
            // of the Left or Right trigger): the wait ends, its down resent at once, and the up
            // passes as it is, a little before that down (another button: no harm). Not resent
            // after it: the button pressed again before the sender sent it would have its down
            // (passed at once) before that up (down, down, up). As .NET (24dfb4e, 11a7b45).
            self.end_lr_wait(host);
            self.resend_waiting_down(wait, host);
            return true;
        }

        if self.up_owners.get(btn) == UpOwner::Trigger {
            self.handle_suppressed_up_in_scroll(btn, point, time, host);
            return false;
        }

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
        let within_locktime =
            time.wrapping_sub(scroll.started_at) <= self.config.trigger.scroll_locktime_ms;
        match &mut scroll.kind {
            ScrollKind::Double { first_up_seen } if btn.is_lr() => {
                if !*first_up_seen {
                    *first_up_seen = true;
                } else if within_locktime {
                    scroll.released = true;
                } else {
                    self.exit_scroll(host);
                }
            }
            ScrollKind::Drag { button, moved, .. } if *button == btn => {
                let moved = *moved;
                if moved && self.config.trigger.dragged_lock {
                    scroll.released = true;
                } else {
                    self.exit_scroll(host);
                    if !moved {
                        host.send_click(btn, point, self.tags.click_tag());
                    }
                }
            }
            ScrollKind::Single(button) if *button == btn => {
                if within_locktime {
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

        // The keyboard hook is removed while paused (keyboard_hook_enabled)
        if self.paused {
            return true;
        }

        let is_trigger_key = self.config.trigger.keyboard_enabled()
            && event.vk_code == self.config.trigger.target_vk_code.vk_code();

        match event.action {
            KeyAction::Down => {
                // Pause is a key as the others: held, it repeats too (a USB keyboard), and its up
                // comes (with its down at once, a PS/2 one)
                match self.keys.get(vk) {
                    KeyState::Suppressed => return false,
                    KeyState::Passed => return true,
                    KeyState::Idle | KeyState::LoneUp => {}
                }

                // A new press: the lone up left by a reset is not to come any more
                self.keys.set(vk, KeyState::Idle);
                // Already down in the system: in the hook, GetAsyncKeyState still tells the state
                // from before this event, so this is a repeat of a press whose down reached the
                // application with no record of it here. E.g. passed just before an InputLost
                // sent earlier (at an unlock or a resume) was handled: the hook's events are
                // handled before the posted messages, and the reset forgot it. It passes as that
                // press did, its up too: taken as a new press, it would start a scroll (or end
                // one) and be suppressed, its up too, the key left pressed in the application.
                if host.is_key_down(event.vk_code) {
                    self.keys.set(vk, KeyState::Passed);
                    return true;
                }

                if is_trigger_key {
                    if self.is_released_scroll() || self.is_mouse_scroll() {
                        self.exit_scroll(host);
                        self.keys.set(vk, KeyState::Suppressed);
                        return false;
                    }
                    if host.is_excluded(host.cursor_pos(), &self.config.exclusions) {
                        self.keys.set(vk, KeyState::Passed);
                        return true;
                    }
                    let scroll = ActiveScroll::new_immediate(
                        ScrollKind::Keyboard,
                        event.time,
                        &self.config.scroll,
                    );
                    if !self.start_scroll(scroll, host) {
                        // Scroll mode could not start: the key passes as if it were not the
                        // trigger (its repeats and its up too)
                        self.keys.set(vk, KeyState::Passed);
                        return true;
                    }
                    self.keys.set(vk, KeyState::Suppressed);
                    return false;
                }

                if self.is_released_scroll() {
                    self.exit_scroll(host);
                    self.keys.set(vk, KeyState::Suppressed);
                    return false;
                }

                self.keys.set(vk, KeyState::Passed);
                true
            }
            KeyAction::Up => match self.keys.get(vk) {
                KeyState::Suppressed => {
                    self.keys.set(vk, KeyState::Idle);
                    if is_trigger_key && self.is_pressed_keyboard_scroll() {
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
                    false
                }
                KeyState::LoneUp => {
                    self.keys.set(vk, KeyState::Idle);
                    false
                }
                KeyState::Passed => {
                    self.keys.set(vk, KeyState::Idle);
                    true
                }
                KeyState::Idle => true,
            },
        }
    }

    /// The LR trigger wait has timed out by the time of this event (later than pollTimeout after
    /// the waiting down): handled as its timeout, before the event. WM_TIMER is only made when the
    /// queue of the thread has nothing else (the hook events come first), so it can come late: a
    /// second down 80 ms after the first would still start scroll mode with a pollTimeout of 50 ms.
    /// The .NET version waits on a thread of its own with the timeout, so an event after it is
    /// not offered to the wait either (EventWaiter).
    fn expire_lr_wait_if_due(&mut self, time: u32, host: &mut impl TriggerHost) {
        if let Some(wait) = self.lr_wait {
            let elapsed = time.wrapping_sub(wait.time) as i32;
            if elapsed > self.config.trigger.poll_timeout_ms as i32 {
                self.on_poll_timeout(host);
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
        self.resend_waiting_down(wait, host);
    }

    /// Ends the LR wait, if any, and its timer: its down is not resent
    fn end_lr_wait(&mut self, host: &mut impl TriggerHost) {
        if self.lr_wait.take().is_some() {
            host.kill_poll_timer();
        }
    }

    /// The down of the LR wait, ended with no pair, resent alone: its up passes (or its release is
    /// resent, if the up is dropped as a lone up)
    fn resend_waiting_down(&mut self, wait: LrWait, host: &mut impl TriggerHost) {
        self.up_owners.set(wait.button, UpOwner::Resent);
        host.send_down(
            wait.button,
            wait.point,
            self.tags.encode(ResendKind::Down, wait.press_id),
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
            if let ScrollKind::Drag { moved, .. } = &mut scroll.kind {
                *moved = true;
            }
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

    /// --sendInitState, Pass Mode turned on, the trigger changed, Escape down at a button's up, the
    /// exit: see Reset::INIT_STATE
    pub fn init_state(&mut self, host: &mut impl TriggerHost) {
        self.reset(Reset::INIT_STATE, host);
    }

    /// The mouse hook has been replaced, and Windows had removed the old one (e.g. for a
    /// timeout): buttons may have been released with no hook to see it, and a scroll mode they
    /// started would not end (the moves suppressed). The mouse triggers' records are reset (see
    /// ButtonTracker::forget_presses) and that scroll mode is ended (a keyboard one goes on). A
    /// hook replaced while the old one was still installed missed nothing: nothing is changed
    /// then.
    pub fn on_mouse_hook_lost(&mut self, host: &mut impl TriggerHost) {
        self.reset(Reset::MOUSE_HOOK_LOST, host);
    }

    /// The keyboard hook has been replaced, and Windows had removed the old one (e.g. for a
    /// timeout): keys may have been released, and pressed again, with no hook to see it (the new
    /// down reached the application). So nothing of before is kept: the scroll mode of the
    /// trigger key is ended and all the keys' records are forgotten (a key passed over an
    /// excluded window and released meanwhile would pass its next press). A trigger key held now
    /// passes (see pass_held_trigger_key). The other keys pass anyway.
    /// A hook replaced while the old one was still installed missed nothing: nothing is changed
    /// then.
    pub fn on_keyboard_hook_lost(&mut self, host: &mut impl TriggerHost) {
        self.reset(Reset::KEYBOARD_HOOK_GONE, host);
        self.pass_held_trigger_key(host);
    }

    /// The keyboard hook has been installed where there was none (at the end of a pause, at
    /// startup, or turned on): the keys' records were reset when it was removed or could not be
    /// installed, and the keys pressed meanwhile reached the application. A trigger key held now
    /// passes (see pass_held_trigger_key): its repeats would start a scroll, and its up be
    /// suppressed, the key left pressed in the application.
    pub fn on_keyboard_hook_installed(&mut self, host: &mut impl TriggerHost) {
        self.pass_held_trigger_key(host);
    }

    /// The trigger key, if it is down now, is taken as passed: its down reached the application
    /// with no hook to see it, so its repeats and its up pass, and its next press is the trigger.
    /// GetAsyncKeyState tells only what it can (0 also when it cannot): a key taken as up is a
    /// new press.
    fn pass_held_trigger_key(&mut self, host: &mut impl TriggerHost) {
        let trigger = self.config.trigger.target_vk_code.vk_code();
        if self.config.trigger.keyboard_enabled()
            && let Ok(vk) = usize::try_from(trigger)
            && vk < KEY_COUNT
            && host.is_key_down(trigger)
        {
            self.keys.set(vk, KeyState::Passed);
        }
    }

    /// The session was locked or unlocked, or the system suspended or resumed: the hooks saw none
    /// of the input meanwhile (it went to the secure desktop, or was lost). As init_state, and the
    /// records of all keys and buttons are forgotten as when the hooks were lost: none is still
    /// held from before. Handled after the hook's events that came after it was sent: a key
    /// passed then and forgotten passes on (see handle_key_inner, a repeat of a key down already).
    pub fn on_input_lost(&mut self, host: &mut impl TriggerHost) {
        self.reset(Reset::INPUT_LOST, host);
    }

    /// The keyboard hook has been removed (keyboard trigger off, or it could not be installed):
    /// its records are reset and the scroll mode of the trigger key is ended.
    pub fn on_keyboard_hook_removed(&mut self, host: &mut impl TriggerHost) {
        self.reset(Reset::KEYBOARD_HOOK_GONE, host);
    }

    /// The properties have been loaded (Reload, switching, adding, deleting or resetting them):
    /// the scroll settings apply from the next scroll, so a scroll in progress is ended, as .NET
    /// does (Ctx.applyProperties). The trigger's up is still suppressed (its flags are kept).
    pub fn end_scroll_after_load(&mut self, host: &mut impl TriggerHost) {
        self.reset(Reset::LOAD, host);
    }

    pub fn shutdown(&mut self, host: &mut impl TriggerHost) {
        self.init_state(host);
    }

    /// Ends the LR wait and the scroll, and forgets the records, as `reset` tells (see Reset)
    fn reset(&mut self, reset: Reset, host: &mut impl TriggerHost) {
        if reset.mouse != MouseRecords::Keep {
            self.end_lr_wait(host);
        }
        let ends_scroll = match reset.scroll {
            EndScroll::Mouse => self.is_mouse_scroll(),
            EndScroll::Keyboard => self.is_keyboard_scroll(),
            EndScroll::Any => self.scroll.is_some(),
        };
        if ends_scroll {
            self.exit_scroll(host);
        }
        match reset.mouse {
            MouseRecords::Keep => {}
            MouseRecords::Reset | MouseRecords::Forget => {
                self.up_owners.reset();
                if reset.mouse == MouseRecords::Forget {
                    self.tracker.forget_presses();
                }
            }
        }
        match reset.keys {
            KeyRecords::Keep => {}
            KeyRecords::LoneUp => self.keys.suppressed_to_lone_up(),
            KeyRecords::Clear => self.keys.reset(),
        }
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
        self.scroll.as_ref().is_some_and(ActiveScroll::is_keyboard)
    }

    fn is_pressed_keyboard_scroll(&self) -> bool {
        self.scroll.as_ref().is_some_and(|s| !s.released && s.is_keyboard())
    }

    fn is_mouse_scroll(&self) -> bool {
        self.scroll.as_ref().is_some_and(|s| !s.is_keyboard())
    }

    /// The scroll mode was started by this button as the single or drag trigger
    fn is_single_scroll_owner(&self, btn: MouseButton) -> bool {
        self.scroll.as_ref().is_some_and(|s| s.kind.button() == Some(btn))
    }

    fn is_pressed_mouse_scroll(&self) -> bool {
        self.scroll.as_ref().is_some_and(|s| !s.released && !s.is_keyboard())
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

    const VK_PAUSE: u32 = 0x13;

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
        keys_down: Vec<u32>,
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
                keys_down: Vec::new(),
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
        fn is_key_down(&self, vk_code: u32) -> bool {
            self.keys_down.contains(&vk_code)
        }
        fn is_excluded(&self, point: Point, exclusions: &ExclusionMatcher) -> bool {
            if exclusions.is_empty() {
                return false;
            }
            self.exclude_all || self.excluded_points.contains(&point)
        }
    }

    fn base_engine(trigger: Trigger) -> (TriggerEngine, ResendTags, MockHost) {
        let cfg = Config {
            first_trigger: trigger,
            accel_table: false,
            vertical_threshold: 5,
            horizontal_threshold: 5,
            cursor_change: true,
            vh_adjuster_mode: false,
            ..Config::default()
        };
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

        // Simulate physical Up arriving BEFORE resent ResendKind::Down reaches the hook.
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
                kind: ResendKind::Release,
                press_id: 1
            })
        );

        // Both ResendKind::Down(1) and ResendKind::Release(1) must now be allowed to pass.
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
        let cfg = Config {
            first_trigger: Trigger::RightDrag,
            dragged_lock: true,
            accel_table: false,
            vertical_threshold: 5,
            horizontal_threshold: 5,
            cursor_change: true,
            vh_adjuster_mode: false,
            ..Config::default()
        };
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
        let cfg = Config {
            first_trigger: Trigger::Middle,
            send_middle_click: true,
            accel_table: false,
            vertical_threshold: 5,
            cursor_change: true,
            vh_adjuster_mode: false,
            ..Config::default()
        };
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
        let cfg = Config {
            first_trigger: Trigger::Middle,
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkPause,
            ..Config::default()
        };
        let tags = ResendTags::next_unique();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            tags,
        );
        let mut host = MockHost::new();

        // A press of VK_PAUSE (its down and up at once, as a PS/2 keyboard sends them) starts a
        // released keyboard scroll; the next press ends it.
        assert_eq!(engine.handle_key(key(VK_PAUSE, KeyAction::Down, 100), &mut host), HookDecision::Suppress);
        assert_eq!(engine.handle_key(key(VK_PAUSE, KeyAction::Up, 100), &mut host), HookDecision::Suppress);
        assert!(engine.is_scrolling());
        assert_eq!(engine.handle_key(key(VK_PAUSE, KeyAction::Down, 400), &mut host), HookDecision::Suppress);
        assert!(!engine.is_scrolling());
        assert_eq!(engine.handle_key(key(VK_PAUSE, KeyAction::Up, 400), &mut host), HookDecision::Suppress);
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
        let mut cfg = Config {
            first_trigger: Trigger::RightDrag,
            dragged_lock: true,
            accel_table: false,
            vertical_threshold: 5,
            ..Config::default()
        };
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

        // 4. Keyboard trigger (VK_PAUSE) passes when cursor is over excluded window, its up too,
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
        assert_eq!(kb_engine.handle_key(key(VK_PAUSE, KeyAction::Up, 250), &mut host), HookDecision::Pass);

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
                (MouseButton::Left, Some(ResentInfo::Button { kind: ResendKind::Down, press_id: 1 })),
                (MouseButton::Right, Some(ResentInfo::Button { kind: ResendKind::Down, press_id: 2 })),
            ]
        );
        // The resent downs reach the hook, then the physical ups pass (releasing them).
        for (btn, id) in [(MouseButton::Left, 1), (MouseButton::Right, 2)] {
            assert_eq!(
                engine.handle_mouse(
                    inj_mouse(HookMouseKind::Down(btn), 1, 2, 121, tags.encode(ResendKind::Down, id)),
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
        let cfg = Config {
            first_trigger: Trigger::None,
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
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
    fn lr_second_down_after_poll_timeout_is_not_a_pair_even_if_the_timer_is_late() {
        let cfg = Config {
            first_trigger: Trigger::Lr,
            poll_timeout: 50,
            ..Config::default()
        };
        let tags = ResendTags::next_unique();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            tags,
        );
        let mut host = MockHost::new();

        // 80 ms apart, WM_TIMER not handled yet: the wait timed out first (its down resent),
        // no scroll mode; the right down starts a wait of its own
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 5, 5, 1000), &mut host);
        host.take_calls();
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 5, 5, 1080), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        assert_eq!(
            host.take_calls(),
            vec![
                HostCall::KillTimer,
                HostCall::Down(MouseButton::Left, Point { x: 5, y: 5 }, tags.encode(ResendKind::Down, 1)),
                HostCall::SetTimer(50),
            ]
        );
        // The left up is released as that of a resent down
        engine.handle_mouse(
            inj_mouse(HookMouseKind::Down(MouseButton::Left), 5, 5, 1081, tags.encode(ResendKind::Down, 1)),
            &mut host,
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 5, 5, 1090), &mut host),
            HookDecision::Pass
        );
        // ... and the wait of the right down goes on: the left pressed again within its
        // pollTimeout is its pair
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 5, 5, 1100), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());

        // A move after the timeout also resends the waiting down (as a timeout), and passes
        engine.init_state(&mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 5, 5, 2000), &mut host);
        host.take_calls();
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Move, 9, 9, 2060), &mut host),
            HookDecision::Pass
        );
        assert!(host.take_calls().iter().any(|c| matches!(c, HostCall::Down(MouseButton::Left, ..))));

        // Within pollTimeout it is still a pair
        engine.init_state(&mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 5, 5, 3000), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 5, 5, 3050), &mut host);
        assert!(engine.is_scrolling());
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
    fn keyboard_hook_is_off_while_paused() {
        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        let vk = TriggerKey::VkLmenu.vk_code();
        assert!(engine.keyboard_hook_enabled());

        assert_eq!(engine.handle_key(key(vk, KeyAction::Down, 10), &mut host), HookDecision::Suppress);
        assert!(engine.is_scrolling());
        engine.set_paused(true, &mut host);
        assert!(!engine.is_scrolling());
        // Removed while paused (the hook thread syncs it), so its records are reset
        assert!(!engine.keyboard_hook_enabled());
        engine.on_keyboard_hook_removed(&mut host);
        assert_eq!(engine.handle_key(key(vk, KeyAction::Up, 500), &mut host), HookDecision::Pass);

        // Installed again when the pause ends: the trigger key works again
        engine.set_paused(false, &mut host);
        assert!(engine.keyboard_hook_enabled());
        assert_eq!(engine.handle_key(key(vk, KeyAction::Down, 800), &mut host), HookDecision::Suppress);
        assert!(engine.is_scrolling());

        // Turned off: not installed again when the pause ends
        let mut off = cfg.clone();
        off.keyboard_hook = false;
        engine.set_config(EngineConfig::from_config(&off, &CustomTableState::default()), &mut host);
        engine.set_paused(true, &mut host);
        engine.set_paused(false, &mut host);
        assert!(!engine.keyboard_hook_enabled());
    }

    #[test]
    fn lost_mouse_hook_ends_mouse_scroll_only() {
        // A drag scroll whose up was missed (the old hook removed by Windows): ended, and the
        // trigger works again.
        let (mut engine, _tags, mut host) = base_engine(Trigger::RightDrag);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 100), &mut host);
        assert!(engine.is_scrolling());
        engine.on_mouse_hook_lost(&mut host);
        assert!(!engine.is_scrolling());
        assert!(host.take_calls().contains(&HostCall::StopRaw));
        // Released and pressed again with no hook (that down reached the application): its up
        // passes, not suppressed as the up of the first press (the button would stay down).
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 10, 10, 800), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 900), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());

        // A keyboard scroll goes on.
        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        kb.handle_key(key(TriggerKey::VkLmenu.vk_code(), KeyAction::Down, 10), &mut host);
        assert!(kb.is_scrolling());
        kb.on_mouse_hook_lost(&mut host);
        assert!(kb.is_scrolling());
    }

    #[test]
    fn lost_mouse_hook_drops_queued_resent_down_without_its_release() {
        // LR: the wait timed out and the down was queued to be resent; then the hook was lost and
        // its physical up missed (it reached the application). The resent down, sent after the
        // recovery, must not reach the application alone (the button would stay down).
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 50, 60, 100), &mut host);
        host.take_calls();
        engine.on_poll_timeout(&mut host);
        let down_extra = match host.take_calls().as_slice() {
            [HostCall::KillTimer, HostCall::Down(MouseButton::Left, _, extra)] => *extra,
            other => panic!("unexpected timeout calls: {other:?}"),
        };
        engine.on_mouse_hook_lost(&mut host);
        assert_eq!(
            engine.handle_mouse(
                inj_mouse(HookMouseKind::Down(MouseButton::Left), 50, 60, 400, down_extra),
                &mut host
            ),
            HookDecision::Suppress
        );
        // The next press works as before.
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 50, 60, 900), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(host.take_calls(), vec![HostCall::SetTimer(200)]);

        // Its physical up came before the loss (the release queued too): the click is whole, both
        // pass after the recovery.
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 50, 60, 100), &mut host);
        host.take_calls();
        engine.on_poll_timeout(&mut host);
        let down_extra = match host.take_calls().as_slice() {
            [HostCall::KillTimer, HostCall::Down(MouseButton::Left, _, extra)] => *extra,
            other => panic!("unexpected timeout calls: {other:?}"),
        };
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 50, 60, 310), &mut host);
        let rel_extra = match host.take_calls().as_slice() {
            [HostCall::Up(MouseButton::Left, _, extra)] => *extra,
            other => panic!("unexpected release calls: {other:?}"),
        };
        engine.on_mouse_hook_lost(&mut host);
        assert_eq!(
            engine.handle_mouse(
                inj_mouse(HookMouseKind::Down(MouseButton::Left), 50, 60, 400, down_extra),
                &mut host
            ),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(
                inj_mouse(HookMouseKind::Up(MouseButton::Left), 50, 60, 410, rel_extra),
                &mut host
            ),
            HookDecision::Pass
        );
    }

    #[test]
    fn lost_keyboard_hook_forgets_passed_keys() {
        use crate::exclusions::ExclusionList;

        // The trigger key passed over an excluded window, released with no hook: after the
        // recovery, its first press starts the scroll.
        let excluded_pt = Point { x: 100, y: 100 };
        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);
        let vk = TriggerKey::VkLmenu.vk_code();
        host.cursor = excluded_pt;
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 10), &mut host), HookDecision::Pass);
        kb.on_keyboard_hook_lost(&mut host);
        host.cursor = Point { x: 20, y: 20 };
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 500), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());

        // Passed over an excluded window and still held at the recovery (moved out meanwhile):
        // its repeats and its up pass, no scroll starts.
        kb.init_state(&mut host);
        kb.handle_key(key(vk, KeyAction::Up, 600), &mut host);
        host.cursor = excluded_pt;
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 700), &mut host), HookDecision::Pass);
        host.cursor = Point { x: 20, y: 20 };
        host.keys_down.push(vk);
        kb.on_keyboard_hook_lost(&mut host);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 730), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 800), &mut host), HookDecision::Pass);
        host.keys_down.clear();

        // The trigger key in its scroll, released and pressed again with no hook (the new down
        // reached the application), down at the recovery: the scroll is ended, and the repeats
        // and the up of that new press pass (not suppressed as the old one's). The next press is
        // the trigger.
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 900), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
        host.keys_down.push(vk);
        kb.on_keyboard_hook_lost(&mut host);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 930), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 2000), &mut host), HookDecision::Pass);
        host.keys_down.clear();
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 2100), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
        kb.handle_key(key(vk, KeyAction::Up, 2200), &mut host);

        // First pressed with no hook (no record of it), down at the recovery: its repeats and its
        // up pass, no scroll starts.
        kb.init_state(&mut host);
        host.keys_down.push(vk);
        kb.on_keyboard_hook_lost(&mut host);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 3000), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 3100), &mut host), HookDecision::Pass);
    }

    #[test]
    fn input_lost_forgets_all_keys_and_buttons() {
        use crate::exclusions::ExclusionList;

        // The trigger key passed over an excluded window, released while the session was locked:
        // after the unlock, its first press starts the scroll.
        let excluded_pt = Point { x: 100, y: 100 };
        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);
        let vk = TriggerKey::VkLmenu.vk_code();
        host.cursor = excluded_pt;
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 10), &mut host), HookDecision::Pass);
        kb.on_input_lost(&mut host);
        host.cursor = Point { x: 20, y: 20 };
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 500), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());

        // A drag scroll whose trigger was released and pressed again while locked: ended, and the
        // up after the unlock passes.
        let (mut engine, _tags, mut host) = base_engine(Trigger::RightDrag);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 100), &mut host);
        assert!(engine.is_scrolling());
        engine.on_input_lost(&mut host);
        assert!(!engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 10, 10, 800), &mut host),
            HookDecision::Pass
        );
    }

    #[test]
    fn input_lost_after_a_passed_press_keeps_passing_it() {
        use crate::exclusions::ExclusionList;

        // After the unlock, the trigger key passed over an excluded window before the reset sent
        // at the unlock was handled (the hook's events come first): its repeats and its up pass,
        // no scroll starts. The next press is the trigger.
        let excluded_pt = Point { x: 100, y: 100 };
        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);
        let vk = TriggerKey::VkLmenu.vk_code();
        host.cursor = excluded_pt;
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 10), &mut host), HookDecision::Pass);
        host.keys_down.push(vk);
        kb.on_input_lost(&mut host);
        host.cursor = Point { x: 20, y: 20 };
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 500), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 530), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 600), &mut host), HookDecision::Pass);
        host.keys_down.clear();
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 700), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
        kb.handle_key(key(vk, KeyAction::Up, 800), &mut host);

        // Another key passed before the reset was handled, then a released scroll: the key's
        // repeat does not end the scroll, and is not suppressed (its up would be too).
        kb.init_state(&mut host);
        let k = 0x41;
        assert_eq!(kb.handle_key(key(k, KeyAction::Down, 900), &mut host), HookDecision::Pass);
        host.keys_down.push(k);
        kb.on_input_lost(&mut host);
        kb.handle_key(key(vk, KeyAction::Down, 1000), &mut host);
        kb.handle_key(key(vk, KeyAction::Up, 1050), &mut host);
        assert!(kb.is_scrolling());
        assert_eq!(kb.handle_key(key(k, KeyAction::Down, 1100), &mut host), HookDecision::Pass);
        assert!(kb.is_scrolling());
        assert_eq!(kb.handle_key(key(k, KeyAction::Up, 1200), &mut host), HookDecision::Pass);
    }

    #[test]
    fn a_repeat_passed_with_no_record_passes_its_up_too() {
        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let vk = TriggerKey::VkLmenu.vk_code();
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();

        // Suppressed in its scroll, reset (its up left to be suppressed alone), then down in the
        // system (its up missed, pressed again with the down reaching the application): its
        // repeat and its up pass.
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 100), &mut host), HookDecision::Suppress);
        kb.init_state(&mut host);
        host.keys_down.push(vk);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 500), &mut host), HookDecision::Pass);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 600), &mut host), HookDecision::Pass);
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

        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let vk = TriggerKey::VkLmenu.vk_code();

        // A load with the trigger key held (the hook replaced seamlessly: no engine call) ends its
        // scroll; its repeats and its up stay suppressed, and no new scroll starts.
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        kb.handle_key(key(vk, KeyAction::Down, 10), &mut host);
        assert!(kb.is_scrolling());
        kb.end_scroll_after_load(&mut host);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 40), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 50), &mut host), HookDecision::Suppress);

        // Windows had removed the old hook: the key may have been released and pressed again
        // meanwhile, so its records are forgotten and its up passes.
        let mut lost = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        lost.handle_key(key(vk, KeyAction::Down, 10), &mut host);
        lost.on_keyboard_hook_lost(&mut host);
        assert!(!lost.is_scrolling());
        assert_eq!(lost.handle_key(key(vk, KeyAction::Up, 50), &mut host), HookDecision::Pass);
        assert_eq!(lost.handle_key(key(vk, KeyAction::Down, 90), &mut host), HookDecision::Suppress);
        assert!(lost.is_scrolling());
    }

    #[test]
    fn installed_keyboard_hook_passes_held_trigger_key() {
        let cfg = Config {
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let vk = TriggerKey::VkLmenu.vk_code();
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();

        // Pressed while paused (no hook: its down reached the application), held at the end of
        // the pause: its repeats and its up pass, no scroll starts. The next press is the trigger.
        kb.on_keyboard_hook_removed(&mut host);
        host.keys_down.push(vk);
        kb.on_keyboard_hook_installed(&mut host);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 30), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 100), &mut host), HookDecision::Pass);
        host.keys_down.clear();
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 200), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
        kb.handle_key(key(vk, KeyAction::Up, 300), &mut host);

        // Not held at the end of the pause: its first press is the trigger.
        kb.init_state(&mut host);
        kb.on_keyboard_hook_removed(&mut host);
        kb.on_keyboard_hook_installed(&mut host);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 400), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
    }

    /// The keyboard trigger on `target`, the mouse trigger on Middle (scroll_locktime 200 ms)
    fn kb_engine(target: TriggerKey) -> (TriggerEngine, MockHost, u32) {
        let cfg = Config {
            first_trigger: Trigger::Middle,
            keyboard_hook: true,
            target_vk_code: target,
            ..Config::default()
        };
        let engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        (engine, MockHost::new(), target.vk_code())
    }

    #[test]
    fn other_keys_pass_their_down_repeats_and_up() {
        let (mut kb, mut host, _) = kb_engine(TriggerKey::VkLmenu);
        let k = 0x41;
        for (action, time) in [(KeyAction::Down, 10), (KeyAction::Down, 40), (KeyAction::Up, 60)] {
            assert_eq!(kb.handle_key(key(k, action, time), &mut host), HookDecision::Pass);
        }
        // An up with no down seen passes too
        assert_eq!(kb.handle_key(key(k, KeyAction::Up, 70), &mut host), HookDecision::Pass);
        // A code out of the table passes, untouched
        for action in [KeyAction::Down, KeyAction::Up] {
            assert_eq!(kb.handle_key(key(0x1FF, action, 80), &mut host), HookDecision::Pass);
        }
        assert!(!kb.is_scrolling());
    }

    #[test]
    fn trigger_key_up_ends_or_releases_the_scroll_by_the_locktime() {
        let (mut kb, mut host, vk) = kb_engine(TriggerKey::VkLmenu);

        // Held longer than the locktime: its repeats are suppressed, and its up ends the scroll
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 100), &mut host), HookDecision::Suppress);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 130), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 500), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());

        // Released within the locktime: the scroll goes on, released
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 600), &mut host), HookDecision::Suppress);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 700), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());

        // Another key ends the released scroll: its down, its repeats and its up are suppressed
        let k = 0x41;
        assert_eq!(kb.handle_key(key(k, KeyAction::Down, 800), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(k, KeyAction::Down, 830), &mut host), HookDecision::Suppress);
        assert_eq!(kb.handle_key(key(k, KeyAction::Up, 850), &mut host), HookDecision::Suppress);
        // Its next press passes
        assert_eq!(kb.handle_key(key(k, KeyAction::Down, 900), &mut host), HookDecision::Pass);
        assert_eq!(kb.handle_key(key(k, KeyAction::Up, 950), &mut host), HookDecision::Pass);

        // The trigger key ends a released scroll of its own: suppressed until its up
        kb.handle_key(key(vk, KeyAction::Down, 1000), &mut host);
        kb.handle_key(key(vk, KeyAction::Up, 1050), &mut host);
        assert!(kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 1100), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 1130), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 1500), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
    }

    #[test]
    fn trigger_key_ends_a_mouse_scroll_and_is_suppressed_until_its_up() {
        let (mut kb, mut host, vk) = kb_engine(TriggerKey::VkLmenu);
        kb.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Middle), 0, 0, 100), &mut host);
        assert!(kb.is_scrolling());

        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 200), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        // Its repeats do not start a scroll of their own
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 230), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 300), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());

        // Its next press is the trigger
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 400), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
    }

    #[test]
    fn reset_leaves_the_up_of_a_suppressed_key_to_be_suppressed_alone() {
        let (mut kb, mut host, vk) = kb_engine(TriggerKey::VkLmenu);

        // The trigger key in its scroll, reset (--sendInitState): its up is suppressed alone, as
        // its down was, and only that one up
        kb.handle_key(key(vk, KeyAction::Down, 100), &mut host);
        kb.init_state(&mut host);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 200), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 250), &mut host), HookDecision::Pass);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 300), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
        kb.handle_key(key(vk, KeyAction::Up, 350), &mut host);

        // Pressed again before that up (its up missed): the new down is a press of its own, the
        // trigger, and the lone up is forgotten
        kb.init_state(&mut host);
        kb.handle_key(key(vk, KeyAction::Down, 400), &mut host);
        kb.init_state(&mut host);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Down, 500), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 1000), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 1050), &mut host), HookDecision::Pass);

        // Another key suppressed as it ended a released scroll: its up too
        let k = 0x41;
        kb.handle_key(key(vk, KeyAction::Down, 1100), &mut host);
        kb.handle_key(key(vk, KeyAction::Up, 1150), &mut host);
        assert_eq!(kb.handle_key(key(k, KeyAction::Down, 1200), &mut host), HookDecision::Suppress);
        kb.init_state(&mut host);
        assert_eq!(kb.handle_key(key(k, KeyAction::Up, 1300), &mut host), HookDecision::Suppress);

        // A key passed keeps passing: its up passes after the reset
        assert_eq!(kb.handle_key(key(k, KeyAction::Down, 1400), &mut host), HookDecision::Pass);
        kb.init_state(&mut host);
        assert_eq!(kb.handle_key(key(k, KeyAction::Up, 1500), &mut host), HookDecision::Pass);

        // The input lost (a lock, a suspend...): nothing is kept, the up passes
        kb.handle_key(key(vk, KeyAction::Down, 1600), &mut host);
        kb.on_input_lost(&mut host);
        assert_eq!(kb.handle_key(key(vk, KeyAction::Up, 1700), &mut host), HookDecision::Pass);
    }

    #[test]
    fn pause_key_repeats_as_the_other_keys() {
        use crate::exclusions::ExclusionList;

        let excluded_pt = Point { x: 100, y: 100 };
        let cfg = Config {
            first_trigger: Trigger::Middle,
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkPause,
            ..Config::default()
        };
        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let mut kb = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);

        // Held (a USB keyboard repeats it): its repeats are suppressed and the scroll goes on
        // (not ended and started again at each repeat, the cursor flickering); its up ends it
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Down, 100), &mut host), HookDecision::Suppress);
        for time in [600, 633, 666, 700] {
            assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Down, time), &mut host), HookDecision::Suppress);
            assert!(kb.is_scrolling());
        }
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Up, 1000), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());

        // Its down and up at once (a PS/2 keyboard): a released scroll, the next press ends it
        kb.handle_key(key(VK_PAUSE, KeyAction::Down, 1100), &mut host);
        kb.handle_key(key(VK_PAUSE, KeyAction::Up, 1100), &mut host);
        assert!(kb.is_scrolling());
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Down, 1500), &mut host), HookDecision::Suppress);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Up, 1500), &mut host), HookDecision::Suppress);

        // Passed over an excluded window: its repeats pass after moving out, no scroll starts
        host.cursor = excluded_pt;
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Down, 2000), &mut host), HookDecision::Pass);
        host.cursor = Point { x: 20, y: 20 };
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Down, 2500), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Up, 2600), &mut host), HookDecision::Pass);

        // Held when the keyboard hook is installed: its repeats and its up pass
        host.keys_down.push(VK_PAUSE);
        kb.on_keyboard_hook_removed(&mut host);
        kb.on_keyboard_hook_installed(&mut host);
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Down, 3000), &mut host), HookDecision::Pass);
        assert!(!kb.is_scrolling());
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Up, 3100), &mut host), HookDecision::Pass);
        host.keys_down.clear();
        assert_eq!(kb.handle_key(key(VK_PAUSE, KeyAction::Down, 3200), &mut host), HookDecision::Suppress);
        assert!(kb.is_scrolling());
    }

    /// The mouse trigger on LR, the keyboard trigger on Left Alt
    fn lr_kb_engine() -> (TriggerEngine, MockHost, u32) {
        let cfg = Config {
            first_trigger: Trigger::Lr,
            keyboard_hook: true,
            target_vk_code: TriggerKey::VkLmenu,
            ..Config::default()
        };
        let engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            ResendTags::next_unique(),
        );
        (engine, MockHost::new(), TriggerKey::VkLmenu.vk_code())
    }

    #[test]
    fn keyboard_hook_changes_keep_the_mouse_triggers() {
        let (mut engine, mut host, _) = lr_kb_engine();

        // The wait of the LR trigger goes on: the other button makes the pair
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        host.take_calls();
        engine.on_keyboard_hook_lost(&mut host);
        engine.on_keyboard_hook_removed(&mut host);
        assert!(host.take_calls().is_empty());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 10, 10, 150), &mut host),
            HookDecision::Suppress
        );
        assert!(engine.is_scrolling());

        // The mouse scroll goes on
        engine.on_keyboard_hook_lost(&mut host);
        engine.on_keyboard_hook_removed(&mut host);
        assert!(engine.is_scrolling());
    }

    #[test]
    fn mouse_hook_lost_keeps_the_keys() {
        let (mut engine, mut host, vk) = lr_kb_engine();
        assert_eq!(engine.handle_key(key(vk, KeyAction::Down, 100), &mut host), HookDecision::Suppress);
        engine.on_mouse_hook_lost(&mut host);
        assert!(engine.is_scrolling());
        // Its up is still the trigger's: suppressed, it ends the scroll (past the locktime)
        assert_eq!(engine.handle_key(key(vk, KeyAction::Up, 500), &mut host), HookDecision::Suppress);
        assert!(!engine.is_scrolling());
    }

    #[test]
    fn a_load_ends_the_scroll_and_keeps_the_lr_wait() {
        let (mut engine, mut host, _) = lr_kb_engine();
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        host.take_calls();
        engine.end_scroll_after_load(&mut host);
        assert!(host.take_calls().is_empty());
        // Timed out, the waiting down is resent
        engine.on_poll_timeout(&mut host);
        assert!(matches!(
            host.take_calls().as_slice(),
            [HostCall::KillTimer, HostCall::Down(MouseButton::Left, _, _)]
        ));
    }

    #[test]
    fn init_state_ends_the_lr_wait_and_input_lost_forgets_its_press() {
        // init_state: the wait is ended (its down not resent), and the up of that press, whose
        // down reached no application, is suppressed alone
        let (mut engine, mut host, _) = lr_kb_engine();
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        host.take_calls();
        engine.init_state(&mut host);
        assert!(host.take_calls().contains(&HostCall::KillTimer));
        engine.on_poll_timeout(&mut host);
        assert_eq!(host.take_calls(), vec![HostCall::KillTimer]);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 200), &mut host),
            HookDecision::Suppress
        );

        // on_input_lost: the press is forgotten too (its up may have been missed, and the up of a
        // press made meanwhile reached the application with its down): the up passes
        let (mut engine, mut host, _) = lr_kb_engine();
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        host.take_calls();
        engine.on_input_lost(&mut host);
        assert!(host.take_calls().contains(&HostCall::KillTimer));
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 200), &mut host),
            HookDecision::Pass
        );
    }

    #[test]
    fn a_trigger_pressed_again_with_its_up_missed_ends_the_scroll() {
        // The single and the drag triggers: the second down ends the scroll, suppressed with its
        // up; the next press is the trigger again
        for (trigger, btn) in [(Trigger::Middle, MouseButton::Middle), (Trigger::RightDrag, MouseButton::Right)] {
            let (mut engine, _tags, mut host) = base_engine(trigger);
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(btn), 0, 0, 100), &mut host);
            assert!(engine.is_scrolling());
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Down(btn), 0, 0, 300), &mut host),
                HookDecision::Suppress
            );
            assert!(!engine.is_scrolling());
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Up(btn), 0, 0, 400), &mut host),
                HookDecision::Suppress
            );
            assert!(!host.take_calls().iter().any(|c| matches!(c, HostCall::Click(..))));
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Down(btn), 0, 0, 500), &mut host),
                HookDecision::Suppress
            );
            assert!(engine.is_scrolling());
        }

        // A button of the LR pair: the same, and the other button's up is still suppressed
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 0, 0, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 0, 0, 150), &mut host);
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 0, 0, 300), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        for (btn, time) in [(MouseButton::Left, 350), (MouseButton::Right, 400)] {
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Up(btn), 0, 0, time), &mut host),
                HookDecision::Suppress
            );
        }
        host.take_calls();
        // The next press waits for its pair again
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 0, 0, 500), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(host.take_calls(), vec![HostCall::SetTimer(200)]);
    }

    #[test]
    fn a_waiting_down_resent_by_a_move_or_another_button_passes_its_up() {
        // Moved while waiting: the down is resent, and its up passes
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        host.take_calls();
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Move, 12, 10, 120), &mut host), HookDecision::Suppress);
        let down_extra = match host.take_calls().as_slice() {
            [HostCall::KillTimer, HostCall::Down(MouseButton::Left, _, extra)] => *extra,
            other => panic!("unexpected move calls: {other:?}"),
        };
        assert_eq!(
            engine.handle_mouse(inj_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 121, down_extra), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 12, 10, 200), &mut host),
            HookDecision::Pass
        );
        assert!(!engine.is_scrolling());

        // Another button (not of the pair) pressed while waiting: the waiting down is resent
        // before it, both pass with their ups
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        host.take_calls();
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Middle), 10, 10, 120), &mut host),
            HookDecision::Pass
        );
        let down_extra = match host.take_calls().as_slice() {
            [HostCall::KillTimer, HostCall::Down(MouseButton::Left, _, extra)] => *extra,
            other => panic!("unexpected calls: {other:?}"),
        };
        assert_eq!(
            engine.handle_mouse(inj_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 121, down_extra), &mut host),
            HookDecision::Pass
        );
        for (btn, time) in [(MouseButton::Middle, 150), (MouseButton::Left, 200)] {
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Up(btn), 10, 10, time), &mut host),
                HookDecision::Pass
            );
        }
        assert!(host.take_calls().is_empty());
    }

    #[test]
    fn a_new_press_forgets_a_resent_down_whose_up_was_missed() {
        use crate::exclusions::ExclusionList;

        // LR: the wait of Left timed out (resent), its up missed, another button between; Left
        // pressed again over an excluded window (passed), then Right waits: Left's up ends the
        // wait (Right's down resent at once) and passes, as for any passed press; not passed as
        // the up of the old resent down with Right still waiting (no down resent)
        let excluded_pt = Point { x: 100, y: 100 };
        let cfg = Config {
            first_trigger: Trigger::Lr,
            ..Config::default()
        };
        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);

        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 100), &mut host);
        engine.on_poll_timeout(&mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Middle), 10, 10, 400), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Middle), 10, 10, 450), &mut host);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 100, 100, 500), &mut host),
            HookDecision::Pass
        );
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 20, 20, 600), &mut host);
        host.take_calls();
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 20, 20, 650), &mut host),
            HookDecision::Pass
        );
        assert!(matches!(
            host.take_calls().as_slice(),
            [HostCall::KillTimer, HostCall::Down(MouseButton::Right, _, _)]
        ));
    }

    #[test]
    fn a_physical_up_ends_what_the_triggers_kept_of_its_press() {
        // LR: a released scroll ended by Right's down (its up the trigger's), Left waits, then
        // Right's up comes: dropped as a lone up, the wait goes on, and nothing of Right's press
        // is kept after its up
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 0, 0, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 0, 0, 150), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 0, 0, 200), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 0, 0, 250), &mut host);
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 0, 0, 1000), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 0, 0, 1050), &mut host);
        host.take_calls();
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Right), 0, 0, 1100), &mut host),
            HookDecision::Suppress
        );
        assert!(host.take_calls().is_empty());
        assert_eq!(engine.up_owners.get(MouseButton::Right), UpOwner::None);
    }

    /// RightDrag, with draggedLock as given (thresholds 5)
    fn drag_engine(dragged_lock: bool) -> (TriggerEngine, ResendTags, MockHost) {
        let cfg = Config {
            first_trigger: Trigger::RightDrag,
            dragged_lock,
            accel_table: false,
            vertical_threshold: 5,
            horizontal_threshold: 5,
            vh_adjuster_mode: false,
            ..Config::default()
        };
        let tags = ResendTags::next_unique();
        let engine = TriggerEngine::new(EngineConfig::from_config(&cfg, &CustomTableState::default()), tags);
        (engine, tags, MockHost::new())
    }

    fn has_click(host: &mut MockHost) -> bool {
        host.take_calls().iter().any(|c| matches!(c, HostCall::Click(..)))
    }

    #[test]
    fn drag_takes_a_scroll_or_a_real_move_as_dragged() {
        let right = MouseButton::Right;

        // Scrolled with no move seen by the hook: dragged, released (draggedLock)
        let (mut engine, _tags, mut host) = drag_engine(true);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 10, 10, 100), &mut host);
        engine.on_raw_motion(0, 15, 120, &mut host);
        assert!(host.take_calls().iter().any(|c| matches!(c, HostCall::Wheel(..))));
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 10, 10, 200), &mut host), HookDecision::Suppress);
        assert!(engine.is_scrolling());
        assert!(!has_click(&mut host));

        // Dragged with draggedLock off: its up ends the scroll, with no click
        let (mut engine, _tags, mut host) = drag_engine(false);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 10, 10, 100), &mut host);
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Move, 12, 10, 120), &mut host), HookDecision::Suppress);
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 12, 10, 200), &mut host), HookDecision::Suppress);
        assert!(!engine.is_scrolling());
        assert!(!has_click(&mut host));

        // A move to the same point, an injected move, and a move older than the start are not a
        // drag: the up ends the scroll and the click is resent
        let (mut engine, _tags, mut host) = drag_engine(true);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 10, 10, 1000), &mut host);
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Move, 10, 10, 1010), &mut host), HookDecision::Suppress);
        assert_eq!(
            engine.handle_mouse(inj_mouse(HookMouseKind::Move, 30, 30, 1020, 0), &mut host),
            HookDecision::Suppress
        );
        engine.on_raw_motion(0, 15, 900, &mut host);
        assert!(!host.take_calls().iter().any(|c| matches!(c, HostCall::Wheel(..))));
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 10, 10, 1100), &mut host), HookDecision::Suppress);
        assert!(!engine.is_scrolling());
        assert!(has_click(&mut host));

        // Released, the drag's moves are suppressed and the next down ends it
        let (mut engine, _tags, mut host) = drag_engine(true);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 10, 10, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Move, 12, 10, 120), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 12, 10, 200), &mut host);
        assert!(engine.is_scrolling());
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Move, 40, 40, 300), &mut host), HookDecision::Suppress);
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 40, 40, 400), &mut host), HookDecision::Suppress);
        assert!(!engine.is_scrolling());
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 40, 40, 450), &mut host), HookDecision::Suppress);
        assert!(!has_click(&mut host));
    }

    #[test]
    fn lr_pair_ends_or_releases_at_its_second_up_by_the_locktime() {
        let (left, right) = (MouseButton::Left, MouseButton::Right);

        // The first up keeps the scroll; the second, later than the locktime after the start,
        // ends it
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(left), 0, 0, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 0, 0, 150), &mut host);
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Up(left), 0, 0, 200), &mut host), HookDecision::Suppress);
        assert!(engine.is_scrolling());
        assert_eq!(engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 0, 0, 500), &mut host), HookDecision::Suppress);
        assert!(!engine.is_scrolling());

        // Within the locktime: released, the next down ends it
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(left), 0, 0, 1000), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 0, 0, 1050), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 0, 0, 1100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(left), 0, 0, 1200), &mut host);
        assert!(engine.is_scrolling());
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Middle), 0, 0, 1500), &mut host),
            HookDecision::Suppress
        );
        assert!(!engine.is_scrolling());

        // A button other than the pair's up does not count as the pair's: the ups of the pair,
        // later than the locktime, are the first (the scroll goes on) and the second (it ends)
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Middle), 0, 0, 1550), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(left), 0, 0, 2000), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(right), 0, 0, 2050), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::X1), 0, 0, 2060), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::X1), 0, 0, 2070), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(left), 0, 0, 2400), &mut host);
        assert!(engine.is_scrolling());
        engine.handle_mouse(phys_mouse(HookMouseKind::Up(right), 0, 0, 2450), &mut host);
        assert!(!engine.is_scrolling());
    }

    #[test]
    fn an_up_whose_down_passed_ends_the_lr_wait_and_passes_as_it_is() {
        use crate::exclusions::ExclusionList;

        // The waiting down is resent at once; the up passes as it is (not resent after that
        // down: the button pressed again before the sender sent it would have its down first)
        fn ends_and_passes(engine: &mut TriggerEngine, host: &mut MockHost, btn: MouseButton, waiting: MouseButton) {
            host.take_calls();
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Up(btn), 20, 20, 300), host),
                HookDecision::Pass
            );
            let calls = host.take_calls();
            assert!(
                matches!(calls.as_slice(), [HostCall::KillTimer, HostCall::Down(b, _, _)] if *b == waiting),
                "{calls:?}"
            );
            // Pressed again at once: its down passes, after its up
            assert_eq!(
                engine.handle_mouse(phys_mouse(HookMouseKind::Down(btn), 20, 20, 310), host),
                HookDecision::Pass
            );
        }

        // Another button than the pair's (X1), pressed before the wait
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::X1), 20, 20, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 200), &mut host);
        ends_and_passes(&mut engine, &mut host, MouseButton::X1, MouseButton::Left);

        // The other button of the pair, its down passed over an excluded window
        let excluded_pt = Point { x: 20, y: 20 };
        let cfg = Config {
            first_trigger: Trigger::Lr,
            ..Config::default()
        };
        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 20, 20, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 200), &mut host);
        ends_and_passes(&mut engine, &mut host, MouseButton::Right, MouseButton::Left);

        // The Left trigger's other button (Right), pressed before the wait (not the trigger's)
        let (mut engine, _tags, mut host) = base_engine(Trigger::Left);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Right), 20, 20, 100), &mut host);
        engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 200), &mut host);
        ends_and_passes(&mut engine, &mut host, MouseButton::Right, MouseButton::Left);
    }

    /// Drives `Left↓ -> X1↓ -> Left↑ -> X1↑` at `t`..`t+60`, asserting `[Suppress, Pass, Suppress, Pass]`
    /// so Left's wait is ended by X1 and its down and release are queued without being delivered yet.
    fn queue_lr_wait_ended_by_x1(engine: &mut TriggerEngine, host: &mut MockHost, x: i32, y: i32, t: u32) {
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), x, y, t), host),
            HookDecision::Suppress
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::X1), x, y, t + 20), host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), x, y, t + 40), host),
            HookDecision::Suppress
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::X1), x, y, t + 60), host),
            HookDecision::Pass
        );
    }

    fn take_resent_button_calls(host: &mut MockHost) -> Vec<(HookMouseKind, Point, usize)> {
        host.take_calls()
            .into_iter()
            .filter_map(|call| match call {
                HostCall::Down(btn, pt, extra) => Some((HookMouseKind::Down(btn), pt, extra)),
                HostCall::Up(btn, pt, extra) => Some((HookMouseKind::Up(btn), pt, extra)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lr_waits_ended_by_another_button_while_sender_is_slow_keep_each_click() {
        let cfg = Config {
            first_trigger: Trigger::Lr,
            poll_timeout: 500,
            ..Config::default()
        };
        let tags = ResendTags::next_unique();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()),
            tags,
        );
        let mut host = MockHost::new();

        for t in [100, 300] {
            queue_lr_wait_ended_by_x1(&mut engine, &mut host, 10, 10, t);
        }

        let resent = take_resent_button_calls(&mut host);
        assert_eq!(resent.len(), 4);
        assert_eq!(
            resent
                .iter()
                .map(|(kind, _, extra)| (*kind, tags.decode(*extra)))
                .collect::<Vec<_>>(),
            vec![
                (
                    HookMouseKind::Down(MouseButton::Left),
                    Some(ResentInfo::Button { kind: ResendKind::Down, press_id: 1 })
                ),
                (
                    HookMouseKind::Up(MouseButton::Left),
                    Some(ResentInfo::Button { kind: ResendKind::Release, press_id: 1 })
                ),
                (
                    HookMouseKind::Down(MouseButton::Left),
                    Some(ResentInfo::Button { kind: ResendKind::Down, press_id: 3 })
                ),
                (
                    HookMouseKind::Up(MouseButton::Left),
                    Some(ResentInfo::Button { kind: ResendKind::Release, press_id: 3 })
                ),
            ]
        );

        for (idx, (kind, pt, extra)) in resent.into_iter().enumerate() {
            assert_eq!(
                engine.handle_mouse(inj_mouse(kind, pt.x, pt.y, 400 + idx as u32, extra), &mut host),
                HookDecision::Pass
            );
        }
    }

    #[test]
    fn queued_release_arriving_during_or_after_later_press_keeps_both_intact() {
        // A: a queued Release arriving while a later Left press is waiting does not mark that
        // later press as up_passed (its timed-out Down still passes, and its Up passes).
        let (mut engine, tags, mut host) = base_engine(Trigger::Lr);
        queue_lr_wait_ended_by_x1(&mut engine, &mut host, 10, 10, 100);
        let resent1 = take_resent_button_calls(&mut host);
        assert_eq!(resent1.len(), 2);

        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 200), &mut host),
            HookDecision::Suppress
        );
        for (idx, (kind, pt, extra)) in resent1.into_iter().enumerate() {
            assert_eq!(
                engine.handle_mouse(inj_mouse(kind, pt.x, pt.y, 210 + idx as u32, extra), &mut host),
                HookDecision::Pass
            );
        }
        host.take_calls();
        engine.on_poll_timeout(&mut host);
        let resent2 = take_resent_button_calls(&mut host);
        assert_eq!(resent2.len(), 1);
        let (kind2, pt2, extra2) = resent2[0];
        assert_eq!(
            tags.decode(extra2),
            Some(ResentInfo::Button {
                kind: ResendKind::Down,
                press_id: 3,
            })
        );
        assert_eq!(
            engine.handle_mouse(inj_mouse(kind2, pt2.x, pt2.y, 410, extra2), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 420), &mut host),
            HookDecision::Pass
        );

        // B: a queued Release arriving during a later press preserves down_suppressed, so
        // init_state still suppresses that later press's lone physical Up.
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        queue_lr_wait_ended_by_x1(&mut engine, &mut host, 10, 10, 100);
        let resent1 = take_resent_button_calls(&mut host);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 200), &mut host),
            HookDecision::Suppress
        );
        for (idx, (kind, pt, extra)) in resent1.into_iter().enumerate() {
            assert_eq!(
                engine.handle_mouse(inj_mouse(kind, pt.x, pt.y, 210 + idx as u32, extra), &mut host),
                HookDecision::Pass
            );
        }
        engine.init_state(&mut host);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 10, 10, 250), &mut host),
            HookDecision::Suppress
        );

        // C: a subsequent press that passes physically (over an excluded window) does not clear an
        // earlier press's queued Release.
        use crate::exclusions::ExclusionList;
        let excluded_pt = Point { x: 100, y: 100 };
        let cfg = Config {
            first_trigger: Trigger::Lr,
            ..Config::default()
        };
        let matcher = ExclusionList::from_lines(["C:\\Games\\game.exe"]).to_matcher();
        let mut engine = TriggerEngine::new(
            EngineConfig::from_config(&cfg, &CustomTableState::default()).with_exclusions(matcher),
            ResendTags::next_unique(),
        );
        let mut host = MockHost::new();
        host.excluded_points.push(excluded_pt);

        queue_lr_wait_ended_by_x1(&mut engine, &mut host, 10, 10, 100);
        let resent1 = take_resent_button_calls(&mut host);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 100, 100, 200), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::Left), 100, 100, 220), &mut host),
            HookDecision::Pass
        );
        for (idx, (kind, pt, extra)) in resent1.into_iter().enumerate() {
            assert_eq!(
                engine.handle_mouse(inj_mouse(kind, pt.x, pt.y, 230 + idx as u32, extra), &mut host),
                HookDecision::Pass
            );
        }
    }

    #[test]
    fn queued_releases_evict_oldest_past_limit_and_clean_up_on_dropped_release_and_hook_loss() {
        // 1. Beyond MAX_RELEASES_QUEUED, the oldest queued press is evicted while the newest 16
        // keep their clicks intact.
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        for i in 0..=MAX_RELEASES_QUEUED {
            queue_lr_wait_ended_by_x1(&mut engine, &mut host, 10, 10, 100 + (i as u32) * 100);
        }
        let resent = take_resent_button_calls(&mut host);
        assert_eq!(resent.len(), (MAX_RELEASES_QUEUED + 1) * 2);
        for (idx, (kind, pt, extra)) in resent.into_iter().enumerate() {
            let expected = if idx < 2 {
                HookDecision::Suppress
            } else {
                HookDecision::Pass
            };
            assert_eq!(
                engine.handle_mouse(inj_mouse(kind, pt.x, pt.y, 5000 + idx as u32, extra), &mut host),
                expected
            );
        }

        // 2. When a Release is dropped (e.g. arrived without its Down passing), it is removed from
        // releases_queued so its Down arriving later is also dropped rather than left stuck down.
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        queue_lr_wait_ended_by_x1(&mut engine, &mut host, 10, 10, 100);
        let resent = take_resent_button_calls(&mut host);
        assert_eq!(resent.len(), 2);
        let (down_kind, down_pt, down_extra) = resent[0];
        let (rel_kind, rel_pt, rel_extra) = resent[1];
        assert_eq!(
            engine.handle_mouse(inj_mouse(rel_kind, rel_pt.x, rel_pt.y, 200, rel_extra), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(
            engine.handle_mouse(inj_mouse(down_kind, down_pt.x, down_pt.y, 210, down_extra), &mut host),
            HookDecision::Suppress
        );

        // 3. on_mouse_hook_lost keeps queued full clicks across multiple presses while dropping a
        // subsequent Down whose Release was never queued.
        let (mut engine, _tags, mut host) = base_engine(Trigger::Lr);
        queue_lr_wait_ended_by_x1(&mut engine, &mut host, 10, 10, 100);
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::Left), 10, 10, 200), &mut host),
            HookDecision::Suppress
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Down(MouseButton::X1), 10, 10, 220), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(phys_mouse(HookMouseKind::Up(MouseButton::X1), 10, 10, 240), &mut host),
            HookDecision::Pass
        );
        let resent = take_resent_button_calls(&mut host);
        assert_eq!(resent.len(), 3);

        engine.on_mouse_hook_lost(&mut host);
        assert_eq!(
            engine.handle_mouse(inj_mouse(resent[0].0, resent[0].1.x, resent[0].1.y, 300, resent[0].2), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(inj_mouse(resent[1].0, resent[1].1.x, resent[1].1.y, 310, resent[1].2), &mut host),
            HookDecision::Pass
        );
        assert_eq!(
            engine.handle_mouse(inj_mouse(resent[2].0, resent[2].1.x, resent[2].1.y, 320, resent[2].2), &mut host),
            HookDecision::Suppress
        );
    }
}
