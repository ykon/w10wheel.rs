use crate::cli::IpcCommand;
use crate::win32;

const W10_MESSAGE_BASE: u32 = 264_816_059 & 0x0FFF_FFFF;
pub const W10_MESSAGE_EXIT: u32 = W10_MESSAGE_BASE + 1;
pub const W10_MESSAGE_PASSMODE: u32 = W10_MESSAGE_BASE + 2;
pub const W10_MESSAGE_RELOAD_PROP: u32 = W10_MESSAGE_BASE + 3;
pub const W10_MESSAGE_INIT_STATE: u32 = W10_MESSAGE_BASE + 4;

/// The wheel delta of a command (the high word of mouseData in the hook)
pub const W10_MESSAGE_WHEEL_DELTA: u32 = 1;

const FLAG_MASK: u32 = 0x0FFF_FFFF;
const BOOL_SET_BIT: u32 = 0x1000_0000;
const BOOL_GET_MASK: u32 = 0xF000_0000;

pub fn encode_command(command: &IpcCommand) -> u32 {
    match *command {
        IpcCommand::SendExit => W10_MESSAGE_EXIT,
        IpcCommand::SendPassMode(true) => W10_MESSAGE_PASSMODE | BOOL_SET_BIT,
        IpcCommand::SendPassMode(false) => W10_MESSAGE_PASSMODE,
        IpcCommand::SendReloadProp => W10_MESSAGE_RELOAD_PROP,
        IpcCommand::SendInitState => W10_MESSAGE_INIT_STATE,
    }
}

pub fn decode_command(extra_info: usize) -> Option<IpcCommand> {
    if (extra_info as u64 >> 32) != 0 {
        return None;
    }
    let raw = extra_info as u32;
    match raw & FLAG_MASK {
        W10_MESSAGE_EXIT => Some(IpcCommand::SendExit),
        W10_MESSAGE_PASSMODE => Some(IpcCommand::SendPassMode((raw & BOOL_GET_MASK) != 0)),
        W10_MESSAGE_RELOAD_PROP => Some(IpcCommand::SendReloadProp),
        W10_MESSAGE_INIT_STATE => Some(IpcCommand::SendInitState),
        _ => None,
    }
}

pub fn send_command(command: &IpcCommand) -> bool {
    let pt = win32::cursor_pos();
    let msg = encode_command(command);
    win32::send_ipc_hwheel(pt, msg as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sender::{ResendKind, ResendTags};

    #[test]
    fn encodes_and_decodes_all_w10_messages_with_exact_constants() {
        assert_eq!(W10_MESSAGE_EXIT, 0x0FC8_C5BC);
        assert_eq!(W10_MESSAGE_PASSMODE, 0x0FC8_C5BD);
        assert_eq!(W10_MESSAGE_RELOAD_PROP, 0x0FC8_C5BE);
        assert_eq!(W10_MESSAGE_INIT_STATE, 0x0FC8_C5BF);

        assert_eq!(encode_command(&IpcCommand::SendExit), 0x0FC8_C5BC);
        assert_eq!(encode_command(&IpcCommand::SendPassMode(true)), 0x1FC8_C5BD);
        assert_eq!(
            encode_command(&IpcCommand::SendPassMode(false)),
            0x0FC8_C5BD
        );
        assert_eq!(encode_command(&IpcCommand::SendReloadProp), 0x0FC8_C5BE);
        assert_eq!(encode_command(&IpcCommand::SendInitState), 0x0FC8_C5BF);

        assert_eq!(decode_command(0x0FC8_C5BC), Some(IpcCommand::SendExit));
        assert_eq!(
            decode_command(0x1FC8_C5BD),
            Some(IpcCommand::SendPassMode(true))
        );
        assert_eq!(
            decode_command(0x8FC8_C5BD),
            Some(IpcCommand::SendPassMode(true))
        );
        assert_eq!(
            decode_command(0x0FC8_C5BD),
            Some(IpcCommand::SendPassMode(false))
        );
        assert_eq!(decode_command(0x0FC8_C5BE), Some(IpcCommand::SendReloadProp));
        assert_eq!(decode_command(0x0FC8_C5BF), Some(IpcCommand::SendInitState));
    }

    #[test]
    fn rejects_invalid_high_bits_unknown_flags_and_resend_tags() {
        assert_eq!(decode_command(0), None);
        assert_eq!(decode_command(0x0FC8_C5C0), None);
        assert_eq!(decode_command(0x0000_0001_0FC8_C5BC_usize), None);

        let tags = ResendTags::next_unique();
        assert_eq!(decode_command(tags.click_tag()), None);
        assert_eq!(decode_command(tags.encode(ResendKind::Down, 1)), None);
        assert_eq!(decode_command(tags.encode(ResendKind::Release, 42)), None);
    }
}
