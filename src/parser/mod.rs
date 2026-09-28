//! Turns agent terminal output into agent-neutral messages.
//!
//! Each agent gets its own parser. Agents without one, or output a parser cannot
//! attribute, fall back to a single raw `Terminal` message so nothing is dropped.

mod droid;

use crate::conversation::{push_line, Message, Role};
use crate::terminal::{parse_ansi, StyledLine};

pub fn parse_terminal(agent: &str, ansi: &str) -> Vec<Message> {
    let lines = parse_ansi(ansi);
    let messages = match agent {
        "droid" => droid::parse(&lines),
        _ => Vec::new(),
    };
    if messages.is_empty() {
        generic(&lines)
    } else {
        messages
    }
}

fn generic(lines: &[StyledLine]) -> Vec<Message> {
    let mut message = Message::new(Role::Terminal, "");
    for line in lines {
        push_line(&mut message.text, &line.text);
    }
    message.finish();
    if message.text.is_empty() {
        Vec::new()
    } else {
        vec![message]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_agent_falls_back_to_terminal_text() {
        let messages = parse_terminal("mystery", "\n\x1b[1mhello\x1b[0m\nworld\n\n");
        assert_eq!(messages, vec![Message::new(Role::Terminal, "hello\nworld")]);
    }

    #[test]
    fn empty_output_has_no_messages() {
        assert!(parse_terminal("mystery", "\n  \n").is_empty());
    }
}
