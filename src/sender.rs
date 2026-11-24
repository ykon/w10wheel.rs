use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::ScrollAxis;
use crate::WheelEvent;
use crate::config::Trigger;

const DEFAULT_CLICK_DELAY: Duration = Duration::from_millis(10);
// Ups that could not be sent are tried again at this interval for as long as it takes (e.g. while
// the secure desktop of UAC is shown, SendInput fails): given up, the button would stay pressed.
const DEFAULT_RETRY_INTERVAL: Duration = Duration::from_millis(100);

pub const KIND_PLAIN: usize = 0;
pub const KIND_DOWN: usize = 1;
pub const KIND_RELEASE: usize = 2;

const KIND_MASK: usize = 0x03;
const PRESS_ID_SHIFT: u32 = 2;
pub const PRESS_ID_BITS: u32 = 14;
pub const PRESS_ID_MASK: u32 = (1 << PRESS_ID_BITS) - 1;
const PAYLOAD_MASK: usize = ((PRESS_ID_MASK as usize) << PRESS_ID_SHIFT) | KIND_MASK;
const CLICK_TAG_BIT: usize = 1 << 16;
const TAG_MASK: usize = !(PAYLOAD_MASK | CLICK_TAG_BIT);
const BASE_TAG_MARKER: usize = 0x4000_0000;

static NEXT_TAG_SEED: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
    X1 = 3,
    X2 = 4,
}

impl MouseButton {
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn is_lr(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    pub const fn other_lr(self) -> Option<Self> {
        match self {
            Self::Left => Some(Self::Right),
            Self::Right => Some(Self::Left),
            _ => None,
        }
    }

    pub const fn single_from_trigger(trigger: Trigger) -> Option<Self> {
        match trigger {
            Trigger::Middle => Some(Self::Middle),
            Trigger::X1 => Some(Self::X1),
            Trigger::X2 => Some(Self::X2),
            _ => None,
        }
    }

    pub const fn drag_from_trigger(trigger: Trigger) -> Option<Self> {
        match trigger {
            Trigger::LeftDrag => Some(Self::Left),
            Trigger::RightDrag => Some(Self::Right),
            Trigger::MiddleDrag => Some(Self::Middle),
            Trigger::X1Drag => Some(Self::X1),
            Trigger::X2Drag => Some(Self::X2),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResentInfo {
    Click,
    Button { kind: usize, press_id: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResendTags {
    resend_tag: usize,
    resend_click_tag: usize,
}

impl ResendTags {
    pub fn next_unique() -> Self {
        let seed = NEXT_TAG_SEED.fetch_add(1, Ordering::Relaxed) as usize;
        let high = BASE_TAG_MARKER | ((seed & 0x0fff) << 17);
        Self {
            resend_tag: high,
            resend_click_tag: high | CLICK_TAG_BIT,
        }
    }

    pub const fn click_tag(self) -> usize {
        self.resend_click_tag
    }

    pub const fn encode(self, kind: usize, press_id: u32) -> usize {
        self.resend_tag
            | (((press_id as usize) & (PRESS_ID_MASK as usize)) << PRESS_ID_SHIFT)
            | (kind & KIND_MASK)
    }

    pub const fn decode(self, extra_info: usize) -> Option<ResentInfo> {
        if extra_info == self.resend_click_tag {
            Some(ResentInfo::Click)
        } else if (extra_info & TAG_MASK) == self.resend_tag && (extra_info & CLICK_TAG_BIT) == 0 {
            Some(ResentInfo::Button {
                kind: extra_info & KIND_MASK,
                press_id: ((extra_info >> PRESS_ID_SHIFT) & (PRESS_ID_MASK as usize)) as u32,
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendPacket {
    Wheel {
        axis: ScrollAxis,
        delta: i32,
    },
    ButtonDown {
        button: MouseButton,
        point: Point,
        extra_info: usize,
    },
    ButtonUp {
        button: MouseButton,
        point: Point,
        extra_info: usize,
    },
}

pub trait SendSink: Send + 'static {
    fn send(&mut self, packet: SendPacket) -> bool;
}

#[derive(Clone, Copy, Debug)]
enum ButtonAction {
    Click {
        button: MouseButton,
        point: Point,
        extra_info: usize,
    },
    DiffPointClick {
        button: MouseButton,
        down_point: Point,
        up_point: Point,
        extra_info: usize,
    },
    Down {
        button: MouseButton,
        point: Point,
        extra_info: usize,
    },
    Up {
        button: MouseButton,
        point: Point,
        extra_info: usize,
    },
}

enum SenderMsg {
    Wheel { event: WheelEvent, generation: u64 },
    Button(ButtonAction),
    Pause(bool),
    BumpGeneration(u64),
    BeginClose,
    Close(mpsc::Sender<()>),
}

struct PendingUp {
    button: MouseButton,
    point: Point,
    extra_info: usize,
    due: Instant,
}

struct UnsentUp {
    button: MouseButton,
    point: Point,
    extra_info: usize,
    next_retry: Instant,
}

#[derive(Clone)]
pub struct InputSenderHandle {
    tx: mpsc::Sender<SenderMsg>,
}

impl InputSenderHandle {
    pub fn send_wheel(&self, event: WheelEvent, generation: u64) {
        let _ = self.tx.send(SenderMsg::Wheel { event, generation });
    }

    pub fn send_click(&self, button: MouseButton, point: Point, extra_info: usize) {
        let _ = self.tx.send(SenderMsg::Button(ButtonAction::Click {
            button,
            point,
            extra_info,
        }));
    }

    pub fn send_diff_point_click(
        &self,
        button: MouseButton,
        down_point: Point,
        up_point: Point,
        extra_info: usize,
    ) {
        let _ = self.tx.send(SenderMsg::Button(ButtonAction::DiffPointClick {
            button,
            down_point,
            up_point,
            extra_info,
        }));
    }

    pub fn send_down(&self, button: MouseButton, point: Point, extra_info: usize) {
        let _ = self.tx.send(SenderMsg::Button(ButtonAction::Down {
            button,
            point,
            extra_info,
        }));
    }

    pub fn send_up(&self, button: MouseButton, point: Point, extra_info: usize) {
        let _ = self.tx.send(SenderMsg::Button(ButtonAction::Up {
            button,
            point,
            extra_info,
        }));
    }

    pub fn set_paused(&self, paused: bool) {
        let _ = self.tx.send(SenderMsg::Pause(paused));
    }

    pub fn set_generation(&self, generation: u64) {
        let _ = self.tx.send(SenderMsg::BumpGeneration(generation));
    }

    pub fn begin_close(&self) {
        let _ = self.tx.send(SenderMsg::BeginClose);
    }
}

pub struct InputSender {
    handle: InputSenderHandle,
    join: Option<JoinHandle<()>>,
}

impl InputSender {
    pub fn start(sink: impl SendSink) -> Self {
        Self::start_with_intervals(sink, DEFAULT_CLICK_DELAY, DEFAULT_RETRY_INTERVAL)
    }

    pub fn start_with_intervals(
        sink: impl SendSink,
        click_delay: Duration,
        retry_interval: Duration,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("w10wheel-sender".to_owned())
            .spawn(move || run_sender_loop(rx, sink, click_delay, retry_interval))
            .expect("failed to spawn input sender thread");
        Self {
            handle: InputSenderHandle { tx },
            join: Some(join),
        }
    }

    pub fn handle(&self) -> InputSenderHandle {
        self.handle.clone()
    }

    pub fn begin_close(&self) {
        self.handle.begin_close();
    }

    pub fn close(&mut self) {
        self.handle.begin_close();
        if let Some(join) = self.join.take() {
            let (ack_tx, ack_rx) = mpsc::channel();
            if self.handle.tx.send(SenderMsg::Close(ack_tx)).is_ok() {
                let _ = ack_rx.recv();
            }
            let _ = join.join();
        }
    }
}

impl Drop for InputSender {
    fn drop(&mut self) {
        self.close();
    }
}

struct SenderWorker<S: SendSink> {
    sink: S,
    click_delay: Duration,
    retry_interval: Duration,
    paused: bool,
    valid_generation: u64,
    closing: bool,
    pending_up: Option<PendingUp>,
    button_queue: VecDeque<ButtonAction>,
    unsent_ups: Vec<UnsentUp>,
    last_down_sent: [Option<Instant>; MouseButton::COUNT],
    // The press of the last down resent alone (KIND_DOWN) that was sent, by button (0: none): at
    // exit the up resent for such a down (KIND_RELEASE) is sent only if that down was
    sent_down_id: [u32; MouseButton::COUNT],
}

fn kind_of(extra_info: usize) -> usize {
    extra_info & KIND_MASK
}

fn press_id_of(extra_info: usize) -> u32 {
    ((extra_info >> PRESS_ID_SHIFT) & (PRESS_ID_MASK as usize)) as u32
}

impl<S: SendSink> SenderWorker<S> {
    fn new(sink: S, click_delay: Duration, retry_interval: Duration) -> Self {
        Self {
            sink,
            click_delay,
            retry_interval,
            paused: false,
            valid_generation: 0,
            closing: false,
            pending_up: None,
            button_queue: VecDeque::new(),
            unsent_ups: Vec::new(),
            last_down_sent: [None; MouseButton::COUNT],
            sent_down_id: [0; MouseButton::COUNT],
        }
    }

    /// At exit (closing: the hooks are being removed) only ups are sent, so that no button is
    /// left pressed: a down sent now would not be released by W10Wheel (its physical up may have
    /// come already). The up resent for a down resent alone is sent only if that down was, or it
    /// would be an up without a down (e.g. a right up opens a context menu). As .NET
    /// (Windows.isSendableAtExit).
    fn is_sendable_at_exit(&self, action: &ButtonAction) -> bool {
        match *action {
            ButtonAction::Up {
                button, extra_info, ..
            } => {
                kind_of(extra_info) != KIND_RELEASE
                    || press_id_of(extra_info) == self.sent_down_id[button.index()]
            }
            _ => false,
        }
    }

    fn handle_message(&mut self, msg: SenderMsg) -> Option<mpsc::Sender<()>> {
        match msg {
            SenderMsg::Wheel { event, generation } => {
                if !self.paused && !self.closing && generation == self.valid_generation {
                    let (axis, delta) = match event {
                        WheelEvent::Vertical(delta) => (ScrollAxis::Vertical, delta),
                        WheelEvent::Horizontal(delta) => (ScrollAxis::Horizontal, delta),
                    };
                    let _ = self.sink.send(SendPacket::Wheel { axis, delta });
                }
                None
            }
            SenderMsg::Button(action) => {
                self.button_queue.push_back(action);
                self.drain_buttons();
                None
            }
            SenderMsg::Pause(paused) => {
                self.paused = paused;
                None
            }
            SenderMsg::BumpGeneration(generation) => {
                self.valid_generation = generation;
                None
            }
            SenderMsg::BeginClose => {
                self.closing = true;
                self.flush_pending_and_queue();
                None
            }
            SenderMsg::Close(ack) => Some(ack),
        }
    }

    fn flush_pending_and_queue(&mut self) {
        if let Some(pending) = self.pending_up.take() {
            self.send_up_or_queue(pending.button, pending.point, pending.extra_info);
        }
        self.drain_buttons();
    }

    fn drain_buttons(&mut self) {
        while self.pending_up.is_none() {
            let Some(action) = self.button_queue.pop_front() else {
                break;
            };
            self.execute_button_action(action);
        }
    }

    fn execute_button_action(&mut self, action: ButtonAction) {
        if self.closing && !self.is_sendable_at_exit(&action) {
            return;
        }
        // Ups still unsent are sent first, in order. While some remain (SendInput keeps failing),
        // no other button is pressed, as in .NET: it could not be released either, and the old ups
        // would come after the new press. A down resent alone that is dropped here does not reach
        // the hook, which then drops its resent up too (an up for a down not passed).
        if !self.unsent_ups.is_empty() {
            self.retry_unsent_now();
        }
        if !self.unsent_ups.is_empty() && !matches!(action, ButtonAction::Up { .. }) {
            return;
        }

        match action {
            ButtonAction::Click {
                button,
                point,
                extra_info,
            } => {
                self.send_down_now(button, point, extra_info);
                if self.closing || self.click_delay.is_zero() {
                    self.send_up_or_queue(button, point, extra_info);
                } else {
                    self.pending_up = Some(PendingUp {
                        button,
                        point,
                        extra_info,
                        due: Instant::now() + self.click_delay,
                    });
                }
            }
            ButtonAction::DiffPointClick {
                button,
                down_point,
                up_point,
                extra_info,
            } => {
                self.send_down_now(button, down_point, extra_info);
                if self.closing || self.click_delay.is_zero() {
                    self.send_up_or_queue(button, up_point, extra_info);
                } else {
                    self.pending_up = Some(PendingUp {
                        button,
                        point: up_point,
                        extra_info,
                        due: Instant::now() + self.click_delay,
                    });
                }
            }
            ButtonAction::Down {
                button,
                point,
                extra_info,
            } => {
                self.send_down_now(button, point, extra_info);
            }
            ButtonAction::Up {
                button,
                point,
                extra_info,
            } => {
                if !self.closing && !self.click_delay.is_zero() {
                    if let Some(sent_at) = self.last_down_sent[button.index()] {
                        let due = sent_at + self.click_delay;
                        if due > Instant::now() {
                            self.pending_up = Some(PendingUp {
                                button,
                                point,
                                extra_info,
                                due,
                            });
                            return;
                        }
                    }
                }
                self.send_up_or_queue(button, point, extra_info);
            }
        }
    }

    fn send_down_now(&mut self, button: MouseButton, point: Point, extra_info: usize) {
        if self.sink.send(SendPacket::ButtonDown {
            button,
            point,
            extra_info,
        }) {
            self.last_down_sent[button.index()] = Some(Instant::now());
            if kind_of(extra_info) == KIND_DOWN {
                self.sent_down_id[button.index()] = press_id_of(extra_info);
            }
        }
    }

    fn send_up_or_queue(&mut self, button: MouseButton, point: Point, extra_info: usize) {
        self.last_down_sent[button.index()] = None;
        // After the ups unsent before it (of earlier presses): queued behind them
        if self.unsent_ups.is_empty() {
            if self.sink.send(SendPacket::ButtonUp {
                button,
                point,
                extra_info,
            }) {
                return;
            }
            if self.closing {
                // At exit an up is tried once
                return;
            }
        }
        {
            self.unsent_ups.push(UnsentUp {
                button,
                point,
                extra_info,
                next_retry: Instant::now() + self.retry_interval,
            });
        }
    }

    /// Sends the unsent ups in order, stopping at the first that fails again
    fn retry_unsent_now(&mut self) {
        let now = Instant::now();
        while let Some(entry) = self.unsent_ups.first() {
            let ok = self.sink.send(SendPacket::ButtonUp {
                button: entry.button,
                point: entry.point,
                extra_info: entry.extra_info,
            });
            if !ok {
                if !self.closing {
                    for entry in &mut self.unsent_ups {
                        entry.next_retry = now + self.retry_interval;
                    }
                } else {
                    // At exit they are tried once
                    self.unsent_ups.clear();
                }
                return;
            }
            self.unsent_ups.remove(0);
        }
    }

    fn process_deadlines(&mut self, now: Instant) {
        if let Some(pending) = &self.pending_up {
            if now >= pending.due || self.closing {
                let pending = self.pending_up.take().unwrap();
                self.send_up_or_queue(pending.button, pending.point, pending.extra_info);
                self.drain_buttons();
            }
        }

        if let Some(first) = self.unsent_ups.first()
            && (self.closing || now >= first.next_retry)
        {
            self.retry_unsent_now();
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        let mut earliest = self.pending_up.as_ref().map(|p| p.due);
        for entry in &self.unsent_ups {
            earliest = Some(match earliest {
                Some(cur) => cur.min(entry.next_retry),
                None => entry.next_retry,
            });
        }
        earliest
    }
}

fn run_sender_loop(
    rx: mpsc::Receiver<SenderMsg>,
    sink: impl SendSink,
    click_delay: Duration,
    retry_interval: Duration,
) {
    let mut worker = SenderWorker::new(sink, click_delay, retry_interval);

    loop {
        worker.process_deadlines(Instant::now());
        let deadline = worker.next_deadline();
        let recv_result = match deadline {
            Some(due) => {
                let wait = due.saturating_duration_since(Instant::now());
                rx.recv_timeout(wait)
            }
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };

        match recv_result {
            Ok(msg) => {
                if let Some(ack) = worker.handle_message(msg) {
                    worker.closing = true;
                    worker.flush_pending_and_queue();
                    worker.process_deadlines(Instant::now());
                    let _ = ack.send(());
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                worker.process_deadlines(Instant::now());
            }
            Err(RecvTimeoutError::Disconnected) => {
                worker.closing = true;
                worker.flush_pending_and_queue();
                worker.process_deadlines(Instant::now());
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct TestSink {
        packets: Arc<Mutex<Vec<SendPacket>>>,
        fail_next_up: Arc<Mutex<usize>>,
    }

    impl SendSink for TestSink {
        fn send(&mut self, packet: SendPacket) -> bool {
            if matches!(packet, SendPacket::ButtonUp { .. }) {
                let mut fail = self.fail_next_up.lock().unwrap();
                if *fail > 0 {
                    *fail -= 1;
                    return false;
                }
            }
            self.packets.lock().unwrap().push(packet);
            true
        }
    }

    fn make_sink() -> (TestSink, Arc<Mutex<Vec<SendPacket>>>, Arc<Mutex<usize>>) {
        let packets = Arc::new(Mutex::new(Vec::new()));
        let fail_next_up = Arc::new(Mutex::new(0));
        let sink = TestSink {
            packets: Arc::clone(&packets),
            fail_next_up: Arc::clone(&fail_next_up),
        };
        (sink, packets, fail_next_up)
    }

    #[test]
    fn resend_tags_encode_decode_without_collision() {
        let tags_a = ResendTags::next_unique();
        let tags_b = ResendTags::next_unique();
        assert_ne!(tags_a.click_tag(), tags_b.click_tag());

        assert_eq!(tags_a.decode(tags_a.click_tag()), Some(ResentInfo::Click));
        assert_eq!(tags_a.decode(tags_b.click_tag()), None);
        assert_eq!(tags_a.decode(0), None);
        assert_eq!(tags_a.decode(0x1234_5678), None);

        let down = tags_a.encode(KIND_DOWN, 42);
        assert_eq!(
            tags_a.decode(down),
            Some(ResentInfo::Button {
                kind: KIND_DOWN,
                press_id: 42
            })
        );
        let rel = tags_a.encode(KIND_RELEASE, 16383);
        assert_eq!(
            tags_a.decode(rel),
            Some(ResentInfo::Button {
                kind: KIND_RELEASE,
                press_id: 16383
            })
        );
    }

    #[test]
    fn sender_drops_stale_or_paused_wheel_and_flushes_click_on_close() {
        let (sink, packets, _) = make_sink();
        let mut sender = InputSender::start_with_intervals(
            sink,
            Duration::from_millis(5),
            Duration::from_millis(20),
        );
        let handle = sender.handle();
        let tags = ResendTags::next_unique();

        handle.send_wheel(WheelEvent::Vertical(120), 0);
        handle.set_generation(1);
        handle.send_wheel(WheelEvent::Vertical(-120), 0);
        handle.send_wheel(WheelEvent::Horizontal(60), 1);
        handle.set_paused(true);
        handle.send_wheel(WheelEvent::Vertical(240), 1);
        handle.set_paused(false);
        handle.send_click(
            MouseButton::Right,
            Point { x: 100, y: 200 },
            tags.click_tag(),
        );

        sender.close();

        let sent = packets.lock().unwrap();
        assert_eq!(
            *sent,
            vec![
                SendPacket::Wheel {
                    axis: ScrollAxis::Vertical,
                    delta: 120,
                },
                SendPacket::Wheel {
                    axis: ScrollAxis::Horizontal,
                    delta: 60,
                },
                SendPacket::ButtonDown {
                    button: MouseButton::Right,
                    point: Point { x: 100, y: 200 },
                    extra_info: tags.click_tag(),
                },
                SendPacket::ButtonUp {
                    button: MouseButton::Right,
                    point: Point { x: 100, y: 200 },
                    extra_info: tags.click_tag(),
                },
            ]
        );
    }

    #[test]
    fn sender_retries_failed_button_up() {
        let (sink, packets, fail_next_up) = make_sink();
        *fail_next_up.lock().unwrap() = 2;

        let mut sender = InputSender::start_with_intervals(
            sink,
            Duration::from_millis(1),
            Duration::from_millis(5),
        );
        let handle = sender.handle();
        let tags = ResendTags::next_unique();

        handle.send_down(
            MouseButton::Left,
            Point { x: 10, y: 20 },
            tags.encode(KIND_DOWN, 7),
        );
        handle.send_up(
            MouseButton::Left,
            Point { x: 12, y: 22 },
            tags.encode(KIND_RELEASE, 7),
        );

        thread::sleep(Duration::from_millis(35));
        sender.close();

        let sent = packets.lock().unwrap();
        assert_eq!(
            *sent,
            vec![
                SendPacket::ButtonDown {
                    button: MouseButton::Left,
                    point: Point { x: 10, y: 20 },
                    extra_info: tags.encode(KIND_DOWN, 7),
                },
                SendPacket::ButtonUp {
                    button: MouseButton::Left,
                    point: Point { x: 12, y: 22 },
                    extra_info: tags.encode(KIND_RELEASE, 7),
                },
            ]
        );
    }

    #[test]
    fn sender_keeps_retrying_unsent_up_and_presses_nothing_else_meanwhile() {
        let (sink, packets, fail_next_up) = make_sink();
        // Far more failures than the old limit (5 retries): the up must still be sent in the end.
        *fail_next_up.lock().unwrap() = 20;

        let mut sender = InputSender::start_with_intervals(
            sink,
            Duration::from_millis(1),
            Duration::from_millis(1),
        );
        let handle = sender.handle();
        let tags = ResendTags::next_unique();
        let left_down = tags.encode(KIND_DOWN, 3);
        let left_up = tags.encode(KIND_RELEASE, 3);

        handle.send_down(MouseButton::Left, Point { x: 1, y: 1 }, left_down);
        handle.send_up(MouseButton::Left, Point { x: 1, y: 1 }, left_up);
        // While the up cannot be sent, no other button is pressed (it could not be released).
        handle.send_click(MouseButton::Right, Point { x: 2, y: 2 }, tags.click_tag());

        let deadline = Instant::now() + Duration::from_secs(2);
        while *fail_next_up.lock().unwrap() > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(20));
        handle.send_click(MouseButton::Right, Point { x: 3, y: 3 }, tags.click_tag());
        sender.close();

        let sent = packets.lock().unwrap();
        assert_eq!(
            *sent,
            vec![
                SendPacket::ButtonDown {
                    button: MouseButton::Left,
                    point: Point { x: 1, y: 1 },
                    extra_info: left_down,
                },
                SendPacket::ButtonUp {
                    button: MouseButton::Left,
                    point: Point { x: 1, y: 1 },
                    extra_info: left_up,
                },
                SendPacket::ButtonDown {
                    button: MouseButton::Right,
                    point: Point { x: 3, y: 3 },
                    extra_info: tags.click_tag(),
                },
                SendPacket::ButtonUp {
                    button: MouseButton::Right,
                    point: Point { x: 3, y: 3 },
                    extra_info: tags.click_tag(),
                },
            ]
        );
    }

    #[test]
    fn sender_sends_only_releasable_ups_at_exit() {
        let (sink, packets, _) = make_sink();
        let mut sender = InputSender::start_with_intervals(
            sink,
            Duration::from_millis(30),
            Duration::from_millis(20),
        );
        let handle = sender.handle();
        let tags = ResendTags::next_unique();

        // Sent before exit: the left down resent alone (press 5)
        handle.send_down(MouseButton::Left, Point { x: 1, y: 1 }, tags.encode(KIND_DOWN, 5));
        // A click in progress (its up waits for the click delay), then queued behind it
        handle.send_click(MouseButton::Middle, Point { x: 2, y: 2 }, tags.click_tag());
        handle.send_down(MouseButton::Right, Point { x: 3, y: 3 }, tags.encode(KIND_DOWN, 6));
        handle.send_up(MouseButton::Right, Point { x: 3, y: 3 }, tags.encode(KIND_RELEASE, 6));
        handle.send_up(MouseButton::Left, Point { x: 1, y: 1 }, tags.encode(KIND_RELEASE, 5));
        thread::sleep(Duration::from_millis(5));
        sender.close();

        let sent = packets.lock().unwrap();
        assert_eq!(
            *sent,
            vec![
                SendPacket::ButtonDown {
                    button: MouseButton::Left,
                    point: Point { x: 1, y: 1 },
                    extra_info: tags.encode(KIND_DOWN, 5),
                },
                SendPacket::ButtonDown {
                    button: MouseButton::Middle,
                    point: Point { x: 2, y: 2 },
                    extra_info: tags.click_tag(),
                },
                // The click in progress is completed (its down was sent)
                SendPacket::ButtonUp {
                    button: MouseButton::Middle,
                    point: Point { x: 2, y: 2 },
                    extra_info: tags.click_tag(),
                },
                // Not the right down (no press at exit), nor its up (an up without a down);
                // the left up is sent (its down was)
                SendPacket::ButtonUp {
                    button: MouseButton::Left,
                    point: Point { x: 1, y: 1 },
                    extra_info: tags.encode(KIND_RELEASE, 5),
                },
            ]
        );
    }

    #[test]
    fn sender_serializes_diff_point_click_before_next_down() {
        let (sink, packets, _) = make_sink();
        let mut sender = InputSender::start_with_intervals(
            sink,
            Duration::from_millis(10),
            Duration::from_millis(20),
        );
        let handle = sender.handle();
        let tags = ResendTags::next_unique();

        handle.send_diff_point_click(
            MouseButton::Left,
            Point { x: 1, y: 2 },
            Point { x: 3, y: 4 },
            tags.encode(KIND_PLAIN, 0),
        );
        handle.send_down(
            MouseButton::Right,
            Point { x: 5, y: 6 },
            tags.encode(KIND_DOWN, 1),
        );

        // Before closing: a down still queued at exit is not sent
        thread::sleep(Duration::from_millis(60));
        sender.close();

        let sent = packets.lock().unwrap();
        assert_eq!(
            *sent,
            vec![
                SendPacket::ButtonDown {
                    button: MouseButton::Left,
                    point: Point { x: 1, y: 2 },
                    extra_info: tags.encode(KIND_PLAIN, 0),
                },
                SendPacket::ButtonUp {
                    button: MouseButton::Left,
                    point: Point { x: 3, y: 4 },
                    extra_info: tags.encode(KIND_PLAIN, 0),
                },
                SendPacket::ButtonDown {
                    button: MouseButton::Right,
                    point: Point { x: 5, y: 6 },
                    extra_info: tags.encode(KIND_DOWN, 1),
                },
            ]
        );
    }
}
