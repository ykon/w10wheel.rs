#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

mod app;
mod cursor;
mod dialogs;
mod icons;
mod settings_ui;
mod theme;
mod ui_win32;

fn main() -> std::process::ExitCode {
    app::main()
}
