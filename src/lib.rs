#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
compile_error!("This crate targets 64-bit x86 Windows.");

pub mod cli;
pub mod config;
pub mod exclusions;
pub mod ipc;
pub mod locale;
pub mod properties;
pub mod scroll;
pub mod sender;
pub mod trigger;
mod win32;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cli::IpcCommand;
pub use exclusions::{ExclusionList, ExclusionMatcher, ExclusionsFile};
pub use scroll::{ScrollAxis, ScrollConfig, WheelEvent};
pub use sender::MouseButton;
use sender::{InputSender, InputSenderHandle, Point, ResendTags};
pub use trigger::{EngineConfig, TriggerConfig};
use trigger::{HookDecision, HookKeyEvent, HookMouseEvent, TriggerEngine, TriggerHost};
use win32::{Command, LoopEvent};
pub use win32::{
    AdminStartResult, InstanceLock, allow_taskbar_created_message, is_process_elevated,
    post_window_message, restart_as_admin, set_process_priority, system_ui_language,
};

thread_local! {
    // Low-level hooks, Wait timers, and the Raw Input loop run on the same thread.
    static APP: RefCell<Option<HookApp>> = const { RefCell::new(None) };
}

const GUI_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

// Sent to the GUI when scroll cursor state starts, changes axis, or stops.
pub const SCROLL_ACTIVITY_MESSAGE: u32 = 0x8003; // WM_APP + 3
// Sent to the GUI when an injected W10Message IPC command arrives.
pub const IPC_COMMAND_MESSAGE: u32 = 0x8004; // WM_APP + 4
// Sent to the GUI when the keyboard hook could not be installed: wparam is the Win32 error code,
// lparam is KEYBOARD_HOOK_FAILED_ON_LOAD when it was installed for a load of the properties (at
// startup, Reload, switching...): the setting is then turned off without being written to the
// file, as .NET does. Otherwise (the setting changed in the dialog) the change is stored as off.
// (WM_APP + 5 to + 7 are the GUI's own: ADMIN_RESTART_MESSAGE, THEME_CHANGED_MESSAGE,
// TRAY_SYNC_MESSAGE)
pub const KEYBOARD_HOOK_FAILED_MESSAGE: u32 = 0x8008; // WM_APP + 8
pub const KEYBOARD_HOOK_FAILED_ON_LOAD: isize = 1;

struct Win32Host {
    window: win32::InputWindow,
    sender: InputSenderHandle,
    hook_thread_id: u32,
    scroll_window: usize,
    reported_axis: Option<ScrollAxis>,
    // A keyboard hook failure before the GUI watched (at startup): told when it does
    pending_hook_failure: Option<(usize, isize)>,
}

impl Win32Host {
    fn new(window: win32::InputWindow, sender: InputSenderHandle, hook_thread_id: u32) -> Self {
        Self {
            window,
            sender,
            hook_thread_id,
            scroll_window: 0,
            reported_axis: None,
            pending_hook_failure: None,
        }
    }

    fn report_keyboard_hook_failure(&mut self, error_code: i32, on_load: bool) {
        let failure = (
            error_code as u32 as usize,
            if on_load { KEYBOARD_HOOK_FAILED_ON_LOAD } else { 0 },
        );
        if self.scroll_window == 0
            || !win32::post_window_message(
                self.scroll_window,
                KEYBOARD_HOOK_FAILED_MESSAGE,
                failure.0,
                failure.1,
            )
        {
            self.pending_hook_failure = Some(failure);
        }
    }
}

impl TriggerHost for Win32Host {
    fn start_raw_input(&mut self) -> bool {
        self.window.register_mouse()
    }

    fn stop_raw_input(&mut self) {
        self.window.unregister_mouse();
    }

    fn set_poll_timer(&mut self, timeout_ms: u32) {
        self.window.set_poll_timer(timeout_ms);
    }

    fn kill_poll_timer(&mut self) {
        self.window.kill_poll_timer();
    }

    fn update_cursor(&mut self, axis: Option<ScrollAxis>) {
        if self.scroll_window != 0 && self.reported_axis != axis {
            if win32::post_scroll(self.scroll_window, axis) {
                self.reported_axis = axis;
            }
        }
    }

    fn notify_scroll_activity(&mut self) {}

    fn send_wheel(&mut self, event: WheelEvent, generation: u64) {
        self.sender.send_wheel(event, generation);
    }

    fn send_click(&mut self, button: MouseButton, point: Point, extra_info: usize) {
        self.sender.send_click(button, point, extra_info);
    }

    fn send_diff_point_click(
        &mut self,
        button: MouseButton,
        down_point: Point,
        up_point: Point,
        extra_info: usize,
    ) {
        self.sender
            .send_diff_point_click(button, down_point, up_point, extra_info);
    }

    fn send_down(&mut self, button: MouseButton, point: Point, extra_info: usize) {
        self.sender.send_down(button, point, extra_info);
    }

    fn send_up(&mut self, button: MouseButton, point: Point, extra_info: usize) {
        self.sender.send_up(button, point, extra_info);
    }

    fn set_sender_paused(&mut self, paused: bool) {
        self.sender.set_paused(paused);
    }

    fn set_sender_generation(&mut self, generation: u64) {
        self.sender.set_generation(generation);
    }

    fn cursor_pos(&self) -> Point {
        win32::cursor_pos()
    }

    fn is_escape_down(&self) -> bool {
        win32::is_escape_down()
    }

    fn is_modifier_down(&self) -> bool {
        win32::is_modifier_down()
    }

    fn is_excluded(&self, point: Point, exclusions: &ExclusionMatcher) -> bool {
        win32::is_point_excluded(point, exclusions)
    }
}

struct HookApp {
    host: Win32Host,
    engine: TriggerEngine,
    commands: Receiver<Command>,
    pending_ipc: VecDeque<IpcCommand>,
}

impl HookApp {
    fn new(
        window: win32::InputWindow,
        sender: InputSenderHandle,
        hook_thread_id: u32,
        commands: Receiver<Command>,
        config: EngineConfig,
        tags: ResendTags,
    ) -> Self {
        Self {
            host: Win32Host::new(window, sender, hook_thread_id),
            engine: TriggerEngine::new(config, tags),
            commands,
            pending_ipc: VecDeque::new(),
        }
    }

    fn on_event(&mut self, event: LoopEvent, hooks: &mut win32::Hooks) {
        match event {
            LoopEvent::RawMotion { x, y, time } => {
                self.engine.on_raw_motion(x, y, time, &mut self.host);
            }
            LoopEvent::PollTimeout => {
                self.engine.on_poll_timeout(&mut self.host);
            }
            LoopEvent::Wake => {
                while let Some(ipc_cmd) = self.pending_ipc.pop_front() {
                    match ipc_cmd {
                        IpcCommand::SendExit => win32::quit(),
                        IpcCommand::SendPassMode(paused) => self.set_paused(paused, hooks),
                        IpcCommand::SendInitState => {
                            self.engine.init_state(&mut self.host);
                            self.sync_keyboard_hook(&mut hooks.keyboard, true);
                        }
                        IpcCommand::SendReloadProp => {}
                    }
                }
                while let Ok(command) = self.commands.try_recv() {
                    match command {
                        Command::Pause(paused) => self.set_paused(paused, hooks),
                        Command::Config(config) => {
                            self.engine.set_config(*config, &mut self.host);
                            self.sync_keyboard_hook(&mut hooks.keyboard, false);
                        }
                        Command::Load(config) => {
                            self.engine.set_config(*config, &mut self.host);
                            // Installed again on every load, as in .NET (e.g. a Reload after
                            // Windows removed it for a timeout)
                            self.sync_keyboard_hook(&mut hooks.keyboard, true);
                            self.engine.end_scroll_after_load(&mut self.host);
                        }
                        Command::InitState => {
                            self.engine.init_state(&mut self.host);
                            self.sync_keyboard_hook(&mut hooks.keyboard, true);
                        }
                        Command::ReinstallHooks => self.reinstall_hooks(hooks),
                        Command::InputLost => self.engine.on_input_lost(&mut self.host),
                        Command::Stop => win32::quit(),
                        Command::WatchScroll(window) => {
                            self.host.scroll_window = window;
                            self.host.reported_axis = None;
                            if window != 0
                                && let Some((code, on_load)) = self.host.pending_hook_failure.take()
                            {
                                self.host
                                    .report_keyboard_hook_failure(code as u32 as i32, on_load != 0);
                            }
                        }
                    }
                }
            }
        }
    }

    /// While paused, the keyboard hook is removed (unlike .NET, which keeps it and passes the
    /// keys): nothing of the keyboard is seen then. The up of a key held when the pause starts
    /// passes alone, and a key held when it ends is a new press.
    /// The mouse hook stays, as it receives the commands (W10Message), and passes all.
    /// When the pause ends, the hooks are installed again. Pausing and resuming (Pass Mode) is
    /// then also the way to put them first again by hand, e.g. after mouse software that was
    /// turned off and on installed its hook again.
    fn set_paused(&mut self, paused: bool, hooks: &mut win32::Hooks) {
        self.engine.set_paused(paused, &mut self.host);
        if paused {
            self.sync_keyboard_hook(&mut hooks.keyboard, false);
        } else {
            self.reinstall_hooks(hooks);
        }
    }

    /// Installs the hooks again, to be first in the chain again (the newest hook is called
    /// first). Each new hook is installed before the old one is removed: a scroll in progress
    /// goes on. If one cannot be installed now, the old one stays and nothing is reported. While
    /// paused, the keyboard hook stays removed; at the end of a pause it is installed where there
    /// was none, and a failure is reported as at a load (the setting turned off, not stored).
    /// Only a hook that Windows had removed already has its records reset (events may have been
    /// missed); the input missed while the session was locked or the system asleep is reset by
    /// the InputLost sent then (see app.rs, on_input_lost).
    fn reinstall_hooks(&mut self, hooks: &mut win32::Hooks) {
        if let Ok(true) = hooks.reinstall_mouse() {
            self.engine.on_mouse_hook_lost(&mut self.host);
        }
        self.sync_keyboard_hook(&mut hooks.keyboard, true);
    }

    fn sync_keyboard_hook(&mut self, keyboard_hook: &mut Option<win32::KeyboardHook>, reinstall: bool) {
        let enabled = self.engine.keyboard_hook_enabled();
        match win32::KeyboardHook::sync(keyboard_hook, enabled, reinstall) {
            // Installed where there was none: no records of keys are kept (reset when it was
            // removed or could not be installed). Replaced seamlessly: they still hold.
            win32::KeyboardHookSync::Unchanged
            | win32::KeyboardHookSync::Installed
            | win32::KeyboardHookSync::Replaced {
                old_hook_lost: false,
            } => {}
            win32::KeyboardHookSync::Replaced {
                old_hook_lost: true,
            } => {
                self.engine.on_keyboard_hook_lost(&mut self.host);
            }
            win32::KeyboardHookSync::Removed => {
                self.engine.on_keyboard_hook_removed(&mut self.host);
            }
            win32::KeyboardHookSync::Failed(error_code) => {
                // No hook (there was none): as when it is turned off. The GUI turns the setting
                // off and says why (.NET: Ctx.setKeyboardHookOf).
                self.engine.on_keyboard_hook_removed(&mut self.host);
                self.host.report_keyboard_hook_failure(error_code, reinstall);
            }
        }
    }

    fn shutdown(&mut self) {
        self.engine.shutdown(&mut self.host);
    }
}

fn handle_ipc_command(command: IpcCommand, encoded: usize) {
    let _ = APP.try_with(|app| {
        if let Ok(mut app_opt) = app.try_borrow_mut()
            && let Some(app) = app_opt.as_mut()
        {
            if app.host.scroll_window != 0 {
                let _ = win32::post_window_message(
                    app.host.scroll_window,
                    IPC_COMMAND_MESSAGE,
                    encoded,
                    0,
                );
            } else {
                app.pending_ipc.push_back(command);
                let _ = win32::post_wake(app.host.hook_thread_id);
            }
        }
    });
}

fn handle_session_end() {
    let _ = APP.try_with(|app| {
        if let Ok(mut app_opt) = app.try_borrow_mut()
            && let Some(app) = app_opt.as_mut()
        {
            app.shutdown();
            if app.host.scroll_window != 0 {
                let _ = win32::post_window_message(
                    app.host.scroll_window,
                    IPC_COMMAND_MESSAGE,
                    ipc::W10_MESSAGE_EXIT as usize,
                    0,
                );
            }
            win32::quit();
        }
    });
}

fn handle_mouse(event: HookMouseEvent) -> HookDecision {
    APP.try_with(|app| {
        app.try_borrow_mut()
            .ok()
            .and_then(|mut app| {
                app.as_mut()
                    .map(|app| app.engine.handle_mouse(event, &mut app.host))
            })
            .unwrap_or(HookDecision::Pass)
    })
    .unwrap_or(HookDecision::Pass)
}

fn handle_key(event: HookKeyEvent) -> HookDecision {
    APP.try_with(|app| {
        app.try_borrow_mut()
            .ok()
            .and_then(|mut app| {
                app.as_mut()
                    .map(|app| app.engine.handle_key(event, &mut app.host))
            })
            .unwrap_or(HookDecision::Pass)
    })
    .unwrap_or(HookDecision::Pass)
}

fn run_input_loop(
    config: EngineConfig,
    commands: Receiver<Command>,
    on_ready: impl FnOnce(u32),
) -> io::Result<()> {
    let window = win32::InputWindow::new()?;
    let tags = ResendTags::next_unique();
    let initial_keyboard_hook = config.trigger.keyboard_enabled();
    let mut sender = InputSender::start(win32::Win32SendSink);
    let begin_close_handle = sender.handle();

    let hook_thread_id = win32::current_thread_id();

    APP.with(|app| {
        app.replace(Some(HookApp::new(
            window,
            sender.handle(),
            hook_thread_id,
            commands,
            config,
            tags,
        )));
    });

    let result = win32::message_loop(
        initial_keyboard_hook,
        |error_code| {
            // Not fatal (as in .NET): W10Wheel runs without the keyboard trigger, and says why
            APP.with_borrow_mut(|app| {
                if let Some(app) = app.as_mut() {
                    app.host.report_keyboard_hook_failure(error_code, true);
                }
            });
        },
        on_ready,
        move || begin_close_handle.begin_close(),
        |event, hooks| {
            APP.with_borrow_mut(|app| {
                if let Some(app) = app.as_mut() {
                    app.on_event(event, hooks);
                }
            });
        },
    );

    APP.with(|app| {
        if let Some(mut app) = app.replace(None) {
            app.shutdown();
        }
    });
    sender.close();
    result
}

#[derive(Clone)]
pub struct EngineControl {
    thread: u32,
    commands: Sender<Command>,
}

impl EngineControl {
    fn send(&self, command: Command) -> io::Result<()> {
        self.commands.send(command).map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "input hook thread stopped")
        })?;
        win32::post_wake(self.thread)
    }

    pub fn set_paused(&self, paused: bool) -> io::Result<()> {
        self.send(Command::Pause(paused))
    }

    /// A setting changed: applied at once (a scroll in progress goes on)
    pub fn set_config(&self, config: EngineConfig) -> io::Result<()> {
        self.send(Command::Config(Box::new(config)))
    }

    /// The properties loaded (Reload, switching, adding, deleting or resetting them): applied, the
    /// keyboard hook installed again, and a scroll in progress ended
    pub fn load_config(&self, config: EngineConfig) -> io::Result<()> {
        self.send(Command::Load(Box::new(config)))
    }

    pub fn init_state(&self) -> io::Result<()> {
        self.send(Command::InitState)
    }

    /// Installs the hooks again (after a resume or an unlock): first in the chain again
    pub fn reinstall_hooks(&self) -> io::Result<()> {
        self.send(Command::ReinstallHooks)
    }

    /// The input was not seen (a lock, an unlock, a suspend or a resume): a scroll is ended and
    /// the records of all keys and buttons are reset
    pub fn input_lost(&self) -> io::Result<()> {
        self.send(Command::InputLost)
    }

    pub fn watch_scroll(&self, window: usize) -> io::Result<()> {
        self.send(Command::WatchScroll(window))
    }
}

pub struct InputEngine {
    control: EngineControl,
    worker: JoinHandle<io::Result<()>>,
    completed: Receiver<()>,
}

impl InputEngine {
    pub fn start(config: EngineConfig) -> io::Result<Self> {
        let (ready, thread_id) = mpsc::sync_channel(0);
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (completion, completed) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("w10wheel-hook".into())
            .spawn(move || {
                let result = run_input_loop(config, cmd_rx, move |id| {
                    if ready.send(id).is_err() {
                        win32::quit();
                    }
                });
                let _ = completion.send(());
                result
            })?;
        match thread_id.recv() {
            Ok(id) => Ok(Self {
                control: EngineControl {
                    thread: id,
                    commands: cmd_tx,
                },
                worker,
                completed,
            }),
            Err(_) => {
                join_engine(worker)?;
                Err(io::Error::other("input hook exited during startup"))
            }
        }
    }

    pub fn control(&self) -> EngineControl {
        self.control.clone()
    }

    pub fn shutdown(self) -> io::Result<()> {
        if !self.worker.is_finished() {
            self.control.send(Command::Stop)?;
            if matches!(
                self.completed.recv_timeout(GUI_SHUTDOWN_TIMEOUT),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                let _ = self.control.send(Command::Stop);
            }
        }
        join_engine(self.worker)
    }
}

fn join_engine(worker: JoinHandle<io::Result<()>>) -> io::Result<()> {
    worker
        .join()
        .map_err(|_| io::Error::other("input hook stopped unexpectedly"))?
}
