//! Parser for Factory Droid's terminal UI.
//!
//! Layout observed in `herdr agent read --format ansi` (columns are significant):
//! - User prompt: lines whose first cell has a background color (a colored gutter).
//! - Assistant: `⛬  text`, continuation lines indented by 3.
//! - System notice: `●  text`.
//! - Reasoning: `   Thinking:` followed by indented lines.
//! - Tool call: a bold tool name at indent 3 after a blank line (`   Execute cmd`),
//!   optional header continuation lines, then output lines starting with `    ↳ `.
//! - Bottom chrome (plan, spinner, input box, status bar) ends the transcript.

use crate::conversation::{push_line, Message, Role};
use crate::terminal::StyledLine;

const ASSISTANT_MARKER: char = '⛬';
const SYSTEM_MARKER: char = '●';
const OUTPUT_MARKER: &str = "↳ ";
const BODY_INDENT: usize = 3;
const TOOL_INDENT: usize = 4;

pub fn parse(lines: &[StyledLine]) -> Vec<Message> {
    let mut messages: Vec<Message> = Vec::new();
    let mut prev_blank = true;
    let mut prev_user = false;

    for (i, line) in lines.iter().enumerate() {
        if is_chrome(line) {
            break;
        }
        let is_user = line.bg_at_start;
        if is_user {
            let text = strip_indent(&line.text, BODY_INDENT);
            match messages.last_mut() {
                Some(last) if prev_user => last.push_line(text),
                _ => messages.push(Message::new(Role::User, text)),
            }
        } else if let Some(message) = start_message(line, prev_blank, &lines[i + 1..]) {
            messages.push(message);
        } else if let Some(last) = messages.last_mut() {
            append_continuation(last, &line.text);
        }
        // Lines before the first marker are the startup banner or a message cut off by scrollback.
        prev_blank = line.is_blank();
        prev_user = is_user;
    }

    messages.iter_mut().for_each(Message::finish);
    messages
}

fn start_message(line: &StyledLine, prev_blank: bool, rest: &[StyledLine]) -> Option<Message> {
    let text = line.text.as_str();
    if let Some(rest) = text.strip_prefix(ASSISTANT_MARKER) {
        return Some(Message::new(Role::Assistant, rest.trim_start()));
    }
    if let Some(rest) = text.strip_prefix(SYSTEM_MARKER) {
        return Some(Message::new(Role::System, rest.trim_start()));
    }
    if line.indent() != BODY_INDENT {
        return None;
    }
    if text.trim() == "Thinking:" {
        return Some(Message::new(Role::Reasoning, ""));
    }
    if prev_blank && is_tool_name(&line.bold_prefix) && !is_heading(rest) {
        return Some(Message::tool(text.trim()));
    }
    None
}

/// A bold markdown heading (`**Checks**`) looks like a tool name, but the text after it stays at
/// the body indent, while a tool's own body (`    ↳ ...`, AskUser questions) is indented further.
fn is_heading(rest: &[StyledLine]) -> bool {
    rest.iter()
        .find(|line| !line.is_blank())
        .is_some_and(|next| {
            next.indent() == BODY_INDENT
                && !next.bg_at_start
                && next.text.trim() != "Thinking:"
                && !is_tool_name(&next.bold_prefix)
        })
}

fn append_continuation(message: &mut Message, text: &str) {
    if message.role != Role::Tool {
        message.push_line(strip_indent(text, BODY_INDENT));
        return;
    }
    let body = strip_indent(text, TOOL_INDENT);
    if let Some(output) = body.strip_prefix(OUTPUT_MARKER) {
        message.push_line(output);
    } else if message.text.is_empty() && !body.trim().is_empty() {
        // Wrapped command lines, e.g. `&& cat ...`, belong to the call rather than its output.
        if let Some(title) = message.title.as_mut() {
            push_line(title, body.trim());
        }
    } else {
        message.push_line(strip_indent(body, OUTPUT_MARKER.chars().count()));
    }
}

/// Tool names are short title-case words such as `Execute` or `Web Fetch`.
/// This keeps bold markdown in assistant text (`**Note:**`) from starting a tool call.
fn is_tool_name(prefix: &str) -> bool {
    let words: Vec<&str> = prefix.split(' ').collect();
    !prefix.is_empty()
        && words.len() <= 3
        && words.iter().all(|word| {
            word.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && word.chars().all(|c| c.is_ascii_alphabetic())
        })
}

fn is_chrome(line: &StyledLine) -> bool {
    let text = line.text.as_str();
    text.starts_with('╭')
        || text.starts_with("Plan · ")
        || (text.starts_with(' ') && text.trim_start().starts_with(is_spinner))
        || is_status_bar(text)
}

/// The status bar shows the autonomy mode first, e.g. ` Auto (Med) · allow reversible commands  Opus 5.5 (Low)`.
fn is_status_bar(text: &str) -> bool {
    let Some((mode, _)) = text
        .strip_prefix(' ')
        .and_then(|rest| rest.split_once(" · "))
    else {
        return false;
    };
    mode == "Manual" || mode == "Spec" || (mode.starts_with("Auto (") && mode.ends_with(')'))
}

fn is_spinner(c: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&c)
}

fn strip_indent(text: &str, max: usize) -> &str {
    let spaces = text.chars().take(max).take_while(|c| *c == ' ').count();
    &text[spaces..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::parse_ansi;

    const USER: &str = "\x1b[0m\x1b[48;2;215;95;0m \x1b[0m\x1b[48;2;38;38;38m  \x1b[0m";
    const TOOL: &str = "   \x1b[0m\x1b[1m\x1b[38;2;215;135;95m";

    fn sample() -> String {
        [
            "   ███  banner",
            "",
            "\x1b[1m●\x1b[0m  Model switched to Opus",
            "",
            &format!("{USER}Fix the parser:"),
            &format!("{USER}1. handle tools"),
            "",
            "   \x1b[3mThinking:\x1b[0m",
            "   \x1b[3mLet me look.\x1b[0m",
            "",
            &format!("{TOOL}Execute\x1b[0m cd /repo"),
            "           && cat Cargo.toml",
            "    \x1b[38;2;1;1;1m↳ [package]\x1b[0m",
            "    \x1b[38;2;1;1;1m  name = \"x\"\x1b[0m",
            "    ... 3 more, Ctrl+O to view",
            "",
            "\x1b[1m⛬\x1b[0m  Done. Summary:",
            "   \x1b[1mNote:\x1b[0m bold text stays assistant",
            "",
            "   Second paragraph.",
            "",
            "   \x1b[1mChecks\x1b[0m",
            "",
            "   •  cargo test passes",
            "",
            &format!("{TOOL}Ask User\x1b[0m"),
            "    1. Pick one?",
            "",
            "Plan · 1/2",
            "┃ ● step",
            " ⣠ Executing...",
            "╭──────╮",
        ]
        .join("\r\n")
    }

    #[test]
    fn parses_droid_transcript() {
        let messages = parse(&parse_ansi(&sample()));
        let roles: Vec<Role> = messages.iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            [
                Role::System,
                Role::User,
                Role::Reasoning,
                Role::Tool,
                Role::Assistant,
                Role::Tool
            ]
        );
        assert_eq!(messages[0].text, "Model switched to Opus");
        assert_eq!(messages[1].text, "Fix the parser:\n1. handle tools");
        assert_eq!(messages[2].text, "Let me look.");
        assert_eq!(
            messages[3].title.as_deref(),
            Some("Execute cd /repo\n&& cat Cargo.toml")
        );
        assert_eq!(
            messages[3].text,
            "[package]\nname = \"x\"\n... 3 more, Ctrl+O to view"
        );
        assert_eq!(
            messages[4].text,
            "Done. Summary:\nNote: bold text stays assistant\n\nSecond paragraph.\n\nChecks\n\n•  cargo test passes"
        );
        assert_eq!(messages[5].title.as_deref(), Some("Ask User\n1. Pick one?"));
    }

    #[test]
    fn stops_at_the_status_bar() {
        let ansi = [
            "\x1b[1m⛬\x1b[0m  Done.",
            "",
            " \x1b[38;2;215;135;0mAuto (Med)\x1b[0m · allow reversible commands      Opus 5.5 (Low)",
        ]
        .join("\r\n");
        let messages = parse(&parse_ansi(&ansi));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text, "Done.");
        assert!(is_status_bar(" Manual · ask before changes"));
        assert!(!is_status_bar("   Auto (Med) · indented body text"));
        assert!(!is_status_bar(" Automatic · not a mode"));
    }

    #[test]
    fn tool_name_heuristic() {
        assert!(is_tool_name("Execute"));
        assert!(is_tool_name("Web Fetch"));
        assert!(!is_tool_name("Note:"));
        assert!(!is_tool_name("lowercase"));
        assert!(!is_tool_name(""));
    }
}
