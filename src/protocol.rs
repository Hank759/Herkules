use core::fmt::Write;
use embassy_stm32::can::{Can, Frame};
use embassy_time::{Duration, with_timeout};
use embedded_can::{Frame as _, Id};
use heapless::String;

use crate::swd::Swd;

type Reply = String<63>;

pub struct Line {
    bytes: [u8; 96],
    len: usize,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 96],
            len: 0,
        }
    }

    pub fn push(&mut self, byte: u8) -> Option<&str> {
        if byte == b'\n' {
            let n = core::mem::take(&mut self.len);
            // A full buffer may contain a truncated command. Discard that line.
            if n == self.bytes.len() {
                return Some("");
            }
            return Some(core::str::from_utf8(&self.bytes[..n]).unwrap_or_default());
        }
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
        None
    }
}

fn fixed(text: &str) -> Reply {
    Reply::try_from(text).expect("fixed reply fits one short USB packet")
}

pub async fn execute(command: &str, can: &mut Can<'_>, swd: &mut Option<Swd>) -> Reply {
    let mut words = command.split_ascii_whitespace();
    let mut out = Reply::new();
    match (words.next(), words.next()) {
        (Some("PING"), None) => return fixed("OK DM-MC02\n"),
        (Some("SWD"), Some("ID")) if words.next().is_none() => {
            let Some(swd) = swd.as_mut() else {
                return fixed("ERR SWD_NOT_CONFIGURED\n");
            };
            match swd.read_dpidr() {
                Some(id) => writeln!(out, "OK DPIDR {id:08X}").expect("SWD reply fits"),
                None => return fixed("ERR SWD\n"),
            }
        }
        (Some("CAN"), Some("TX")) => {
            let (Some(id), Some(hex), None) = (words.next(), words.next(), words.next()) else {
                return fixed("ERR SYNTAX\n");
            };
            let Some(frame) = parse_frame(id, hex) else {
                return fixed("ERR FRAME\n");
            };
            return match with_timeout(Duration::from_millis(100), can.write(&frame)).await {
                Ok(None) => fixed("OK QUEUED\n"),
                _ => fixed("ERR CAN TX\n"),
            };
        }
        (Some("CAN"), Some("RX")) if words.next().is_none() => {
            let envelope = match with_timeout(Duration::from_millis(100), can.read()).await {
                Ok(Ok(frame)) => frame,
                Ok(Err(_)) => return fixed("ERR CAN RX\n"),
                Err(_) => return fixed("EMPTY\n"),
            };
            let frame = envelope.frame;
            let Id::Standard(id) = *frame.id() else {
                return fixed("ERR UNSUPPORTED_FRAME\n");
            };
            if frame.is_remote_frame() {
                return fixed("ERR UNSUPPORTED_FRAME\n");
            }
            write!(out, "OK RX {:03X} ", id.as_raw()).expect("CAN prefix fits");
            if frame.data().is_empty() {
                out.push('-').expect("empty payload fits");
            }
            for byte in frame.data() {
                write!(out, "{byte:02X}").expect("8-byte payload fits");
            }
            out.push('\n').expect("line ending fits");
        }
        _ => return fixed("ERR SYNTAX\n"),
    }
    out
}

fn parse_frame(id: &str, payload: &str) -> Option<Frame> {
    let id = u16::from_str_radix(id, 16).ok()?;
    let mut bytes = [0; 8];
    let payload = if payload == "-" { "" } else { payload };
    if !payload.is_ascii() || payload.len() > 16 || !payload.len().is_multiple_of(2) {
        return None;
    }
    for (i, byte) in bytes[..payload.len() / 2].iter_mut().enumerate() {
        *byte = u8::from_str_radix(&payload[i * 2..i * 2 + 2], 16).ok()?;
    }
    Frame::new_standard(id, &bytes[..payload.len() / 2]).ok()
}

/// Parse a host request: TX <ID> <LEN> <DATA>
pub fn parse_bridge_tx(command: &str) -> Option<Frame> {
    let mut parts = command.split_ascii_whitespace();
    if parts.next() != Some("TX") {
        return None;
    }
    let id = parts.next()?;
    let length: usize = parts.next()?.parse().ok()?;
    let data = parts.next()?;

    // Reject extra fields.
    if parts.next().is_some() {
        return None;
    }

    // Classical CAN supports 0 to 8 data bytes.
    if length > 8 {
        return None;
    }

    // '-' represents an empty payload.
    if data == "-" {
        if length != 0 {
            return None;
        }
    } else if data.len() != length * 2 {
        return None;
    }

    // Reuse the existing hexadecimal parser.
    parse_frame(id, data)
}

/// Format a received CAN frame for the host.
pub fn format_bridge_rx(frame: &Frame) -> Option<String<63>> {
    let Id::Standard(id) = *frame.id() else {
        return None;
    };

    if frame.is_remote_frame() {
        return None;
    }

    let mut output = String::<63>::new();
    let data = frame.data();

    write!(&mut output, "RX {:03X} {} ", id.as_raw(), data.len()).ok()?;

    if data.is_empty() {
        output.push('-').ok()?;
    } else {
        for byte in data {
            write!(&mut output, "{byte:02X}").ok()?;
        }
    }

    output.push('\n').ok()?;
    Some(output)
}
