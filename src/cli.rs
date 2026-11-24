#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IpcCommand {
    SendExit,
    SendPassMode(bool),
    SendReloadProp,
    SendInitState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    pub profile: Option<String>,
    pub wait_for_lock: bool,
    pub open_settings: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliAction {
    Run(LaunchOptions),
    Command(IpcCommand),
    UnknownCommand(String),
    InvalidBoolArg { command: String, value: String },
}

pub fn parse_args<I, S>(args: I) -> CliAction
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let Some(first) = args.first() else {
        return CliAction::Run(LaunchOptions::default());
    };

    match first.as_str() {
        "--sendExit" => CliAction::Command(IpcCommand::SendExit),
        "--sendPassMode" => match args.get(1) {
            None => CliAction::Command(IpcCommand::SendPassMode(true)),
            Some(value) if value.eq_ignore_ascii_case("true") => {
                CliAction::Command(IpcCommand::SendPassMode(true))
            }
            Some(value) if value.eq_ignore_ascii_case("false") => {
                CliAction::Command(IpcCommand::SendPassMode(false))
            }
            Some(value) => CliAction::InvalidBoolArg {
                command: first.clone(),
                value: value.clone(),
            },
        },
        "--sendReloadProp" => CliAction::Command(IpcCommand::SendReloadProp),
        "--sendInitState" => CliAction::Command(IpcCommand::SendInitState),
        "--restartAdmin" => {
            let rest = &args[1..];
            let (open_settings, remaining) = match rest.first().map(String::as_str) {
                Some("--openSettings") => (true, &rest[1..]),
                _ => (false, rest),
            };
            CliAction::Run(LaunchOptions {
                profile: remaining.first().cloned(),
                wait_for_lock: true,
                open_settings,
            })
        }
        arg if arg.starts_with("--") => CliAction::UnknownCommand(first.clone()),
        _ => CliAction::Run(LaunchOptions {
            profile: Some(first.clone()),
            wait_for_lock: false,
            open_settings: false,
        }),
    }
}

pub fn build_restart_admin_args(profile: Option<&str>, open_settings: bool) -> String {
    let mut args = String::from("--restartAdmin");
    if open_settings {
        args.push_str(" --openSettings");
    }
    if let Some(name) = profile
        && !name.is_empty()
        && name != "Default"
    {
        args.push_str(" \"");
        args.push_str(name);
        args.push('"');
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_ipc_commands_restart_flags_and_profile_names() {
        assert_eq!(
            parse_args(Vec::<String>::new()),
            CliAction::Run(LaunchOptions::default())
        );
        assert_eq!(
            parse_args(["--sendExit"]),
            CliAction::Command(IpcCommand::SendExit)
        );
        assert_eq!(
            parse_args(["--sendPassMode"]),
            CliAction::Command(IpcCommand::SendPassMode(true))
        );
        assert_eq!(
            parse_args(["--sendPassMode", "True"]),
            CliAction::Command(IpcCommand::SendPassMode(true))
        );
        assert_eq!(
            parse_args(["--sendPassMode", "FALSE"]),
            CliAction::Command(IpcCommand::SendPassMode(false))
        );
        assert_eq!(
            parse_args(["--sendPassMode", "nope"]),
            CliAction::InvalidBoolArg {
                command: "--sendPassMode".into(),
                value: "nope".into(),
            }
        );
        assert_eq!(
            parse_args(["--sendReloadProp"]),
            CliAction::Command(IpcCommand::SendReloadProp)
        );
        assert_eq!(
            parse_args(["--sendInitState"]),
            CliAction::Command(IpcCommand::SendInitState)
        );
        assert_eq!(
            parse_args(["--unknown"]),
            CliAction::UnknownCommand("--unknown".into())
        );
        assert_eq!(
            parse_args(["Gaming"]),
            CliAction::Run(LaunchOptions {
                profile: Some("Gaming".into()),
                wait_for_lock: false,
                open_settings: false,
            })
        );
        assert_eq!(
            parse_args(["--restartAdmin"]),
            CliAction::Run(LaunchOptions {
                profile: None,
                wait_for_lock: true,
                open_settings: false,
            })
        );
        assert_eq!(
            parse_args(["--restartAdmin", "--openSettings"]),
            CliAction::Run(LaunchOptions {
                profile: None,
                wait_for_lock: true,
                open_settings: true,
            })
        );
        assert_eq!(
            parse_args(["--restartAdmin", "--openSettings", "Work"]),
            CliAction::Run(LaunchOptions {
                profile: Some("Work".into()),
                wait_for_lock: true,
                open_settings: true,
            })
        );
    }

    #[test]
    fn builds_restart_admin_arguments_preserving_profile_and_open_settings() {
        assert_eq!(build_restart_admin_args(None, false), "--restartAdmin");
        assert_eq!(
            build_restart_admin_args(Some("Default"), false),
            "--restartAdmin"
        );
        assert_eq!(
            build_restart_admin_args(Some(""), true),
            "--restartAdmin --openSettings"
        );
        assert_eq!(
            build_restart_admin_args(Some("Gaming"), false),
            "--restartAdmin \"Gaming\""
        );
        assert_eq!(
            build_restart_admin_args(Some("My Profile"), true),
            "--restartAdmin --openSettings \"My Profile\""
        );
    }
}
