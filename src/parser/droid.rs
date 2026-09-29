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
const BULLET_MARKER: char = '•';
const TABLE_SEPARATOR: char = '│';
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
            append_continuation(last, line);
        }
        // Lines before the first marker are the startup banner or a message cut off by scrollback.
        prev_blank = line.is_blank();
        prev_user = is_user;
    }

    messages.iter_mut().for_each(Message::finish);
    for message in &mut messages {
        if message.role == Role::Assistant {
            message.text = tables(&message.text);
        }
    }
    messages
}

/// Droid draws a markdown table as columns split by `│`, with a `----│----` rule under the
/// header. Rebuilds those blocks as GFM tables; `|` inside a cell is escaped.
fn tables(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let is_header = lines[i].contains(TABLE_SEPARATOR)
            && lines.get(i + 1).is_some_and(|next| is_table_rule(next));
        if !is_header {
            out.push(lines[i].to_owned());
            i += 1;
            continue;
        }
        let header = cells(lines[i]);
        out.push(table_row(&header));
        out.push(table_row(&vec!["---".to_owned(); header.len()]));
        i += 2;
        while i < lines.len() && lines[i].contains(TABLE_SEPARATOR) {
            out.push(table_row(&cells(lines[i])));
            i += 1;
        }
    }
    out.join("\n")
}

fn is_table_rule(line: &str) -> bool {
    let line = line.trim();
    line.contains(TABLE_SEPARATOR)
        && line
            .split(TABLE_SEPARATOR)
            .all(|part| !part.trim().is_empty() && part.trim().chars().all(|c| c == '-'))
}

fn cells(line: &str) -> Vec<String> {
    line.split(TABLE_SEPARATOR)
        .map(|cell| balance(cell.trim()).replace('|', "\\|"))
        .collect()
}

/// A bold run drawn across several cells (Droid bolds the whole header row) leaves an unmatched
/// `**` at either end of the cells it touched; close it inside each cell.
fn balance(cell: &str) -> String {
    let opens = cell.starts_with("**");
    let closes = cell.len() > 2 && cell.ends_with("**");
    match (opens, closes) {
        (true, false) => format!("{cell}**"),
        (false, true) => format!("**{cell}"),
        _ => cell.to_owned(),
    }
}

fn table_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

fn start_message(line: &StyledLine, prev_blank: bool, rest: &[StyledLine]) -> Option<Message> {
    let text = line.text.as_str();
    if let Some(rest) = text.strip_prefix(ASSISTANT_MARKER) {
        return Some(Message::new(
            Role::Assistant,
            markdown(line, rest.trim_start()),
        ));
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

/// Rebuilds markdown emphasis for `body`, a suffix of `line.text`.
/// Droid draws both `code` and **bold** as plain bold, so spans that look like code
/// (paths, identifiers, flags) get backticks and the rest get `**`.
fn markdown(line: &StyledLine, body: &str) -> String {
    let offset = line.text.len() - body.len();
    let mut out = String::with_capacity(body.len());
    let mut cursor = offset;
    for &(start, end) in &line.bold_spans {
        let start = start.max(offset);
        if start >= end {
            continue;
        }
        out.push_str(&line.text[cursor..start]);
        let span = &line.text[start..end];
        let (inner, trailing) = span.split_at(span.trim_end().len());
        if inner.is_empty() {
            out.push_str(span);
        } else {
            let fence = if looks_like_code(inner) { "`" } else { "**" };
            out.push_str(&format!("{fence}{inner}{fence}{trailing}"));
        }
        cursor = end;
    }
    out.push_str(&line.text[cursor..]);
    list_item(out)
}

/// Droid draws list items as `•  text`, nested items two columns deeper, and hard-wraps their
/// text onto indented lines. Markdown does not know `•`, so the marker becomes `- `; wrapped
/// lines stay lazy continuations of the item. Nesting under `1. ` needs three columns in
/// markdown, so each level of two becomes three.
fn list_item(line: String) -> String {
    let rest = line.trim_start_matches(' ');
    match rest.strip_prefix(BULLET_MARKER) {
        Some(item) => {
            let indent = (line.len() - rest.len()) * 3 / 2;
            format!("{}- {}", " ".repeat(indent), item.trim_start())
        }
        None => line,
    }
}

fn looks_like_code(span: &str) -> bool {
    let core = span.trim_end_matches([':', ',', '.', ';', '：', '，', '。', '；']);
    !core.is_empty()
        && !core.contains(char::is_whitespace)
        && core
            .chars()
            .any(|c| !c.is_alphanumeric() && !is_cjk_punctuation(c))
}

fn is_cjk_punctuation(c: char) -> bool {
    ('\u{3000}'..='\u{303f}').contains(&c) || ('\u{ff00}'..='\u{ffef}').contains(&c)
}

fn append_continuation(message: &mut Message, line: &StyledLine) {
    let text = line.text.as_str();
    if message.role == Role::Assistant {
        message.push_line(&markdown(line, strip_indent(text, BODY_INDENT)));
        return;
    }
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
            "Done. Summary:\n**Note:** bold text stays assistant\n\nSecond paragraph.\n\n**Checks**\n\n- cargo test passes"
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
    fn restores_inline_code_and_bold() {
        let ansi = "\x1b[1m⛬\x1b[0m  Edit \x1b[1mweb/src/App.tsx\x1b[0m, \x1b[1mvery\x1b[0m done";
        let messages = parse(&parse_ansi(ansi));
        assert_eq!(messages[0].text, "Edit `web/src/App.tsx`, **very** done");
        assert!(looks_like_code("--release"));
        assert!(looks_like_code("Cargo.toml."));
        assert!(!looks_like_code("Note:"));
        assert!(!looks_like_code("最值得试的："));
        assert!(!looks_like_code("顺带发现的一个上游（issue）"));
        assert!(looks_like_code("√π"));
        assert!(!looks_like_code("two words"));
    }

    #[test]
    fn converts_nested_bullets() {
        let ansi = [
            "\x1b[1m⛬\x1b[0m  Options:",
            "   •  first item wraps",
            "      onto a second line",
            "     •  nested",
            "   •  second",
        ]
        .join("\r\n");
        let messages = parse(&parse_ansi(&ansi));
        assert_eq!(
            messages[0].text,
            "Options:\n- first item wraps\n   onto a second line\n   - nested\n- second"
        );
    }

    #[test]
    fn rebuilds_tables() {
        let ansi = [
            "\x1b[1m⛬\x1b[0m  Results:",
            "",
            "   \x1b[1mInput          │ Result\x1b[0m",
            "   ---------------│--------",
            "   E = mc²        │ ok",
            "   P(θ | x)       │ ok, fenced",
            "",
            "   After │ the table",
        ]
        .join("\r\n");
        let messages = parse(&parse_ansi(&ansi));
        assert_eq!(
            messages[0].text,
            "Results:\n\n| **Input** | **Result** |\n| --- | --- |\n| E = mc² | ok |\n| P(θ \\| x) | ok, fenced |\n\nAfter │ the table"
        );
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
