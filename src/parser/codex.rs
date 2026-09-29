//! Parser for Codex's terminal UI.
//!
//! Layout observed in `herdr agent read --format ansi`:
//! - User prompt: lines whose first cell has a background color, starting at the first glyph,
//!   with no bold run (the TUI echoes user input plainly).
//! - Tool call: `• Ran ...` / `• Running ...`, followed by dim detail lines (`└ ...`).
//! - Approval notice: `✔ ...`.
//! - Approval prompt: full-width background rows (`Would you like ...`, `› 1. Yes ...`),
//!   which stay bold and group with the following plain hint line.
//! - `+ Show details` toggles carry no content and are skipped.

use crate::conversation::{Message, Role};
use crate::terminal::StyledLine;

const TOOL_MARKER: &str = "• ";
const APPROVAL_MARKER: &str = "✔ ";
const DETAILS_TOGGLE: &str = "+ Show details";

pub fn parse(lines: &[StyledLine]) -> Vec<Message> {
    let mut messages: Vec<Message> = Vec::new();

    for line in lines {
        if line.text.trim() == DETAILS_TOGGLE {
            continue;
        }
        if line.bg_at_start {
            // Approval-box rows (`› 1. Yes ...`) also start at column 0, but stay bold,
            // so they group with the box instead of starting a user message.
            if line.indent() == 0 && line.bold_prefix.is_empty() {
                push_grouped(&mut messages, Role::User, line.text.trim());
            } else {
                push_grouped(&mut messages, Role::System, line.text.trim());
            }
        } else if let Some(rest) = line.text.strip_prefix(TOOL_MARKER) {
            messages.push(Message::tool(rest.trim()));
        } else if let Some(rest) = line.text.strip_prefix(APPROVAL_MARKER) {
            messages.push(Message::new(Role::System, rest.trim()));
        } else if let Some(last) = messages.last_mut() {
            last.push_line(line.text.trim());
        }
        // Lines before the first marker are the startup banner or a message cut off by scrollback.
    }

    messages.iter_mut().for_each(Message::finish);
    messages
}

/// Consecutive user lines (or approval-box lines) belong to one message.
fn push_grouped(messages: &mut Vec<Message>, role: Role, text: &str) {
    match messages.last_mut() {
        Some(last) if last.role == role => last.push_line(text),
        _ => messages.push(Message::new(role, text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::parse_ansi;

    const USER_BG: &str = "\x1b[0m\x1b[48;2;66;66;79m";
    const BOX_BG: &str = "\x1b[0m\x1b[48;2;57;57;71m";

    fn sample() -> String {
        [
            "   banner",
            "",
            &format!("{USER_BG}Fix the parser:\x1b[0m"),
            &format!("{USER_BG}handle tools\x1b[0m"),
            "   \x1b[2m+ Show details\x1b[0m",
            "\x1b[1m• \x1b[0mRan cargo test",
            "  \x1b[2m└ ok\x1b[0m",
            "",
            "✔ You approved codex to run this",
            "",
            &format!("{BOX_BG}  Would you like to run this?\x1b[0m"),
            &format!("{BOX_BG}\x1b[1m› 1. Yes\x1b[0m"),
            "  Press enter to confirm or esc to cancel",
        ]
        .join("\r\n")
    }

    #[test]
    fn parses_codex_transcript() {
        let messages = parse(&parse_ansi(&sample()));
        let roles: Vec<Role> = messages.iter().map(|m| m.role).collect();
        assert_eq!(roles, [Role::User, Role::Tool, Role::System]);
        assert_eq!(messages[0].text, "Fix the parser:\nhandle tools");
        assert_eq!(messages[1].title.as_deref(), Some("Ran cargo test"));
        assert_eq!(messages[1].text, "└ ok");
        assert_eq!(
            messages[2].text,
            "You approved codex to run this\n\nWould you like to run this?\n› 1. Yes\nPress enter to confirm or esc to cancel"
        );
    }
}
