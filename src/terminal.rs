//! Decodes ANSI terminal text into plain lines plus the few style hints the parsers need.
//! Escape-sequence tokenizing is delegated to `anstyle-parse`; this module only interprets SGR.

use anstyle_parse::{Params, Parser, Perform, Utf8Parser};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StyledLine {
    /// Visible text without escape sequences or trailing whitespace.
    pub text: String,
    /// The first cell had a background color; some agents mark user prompts this way.
    pub bg_at_start: bool,
    /// Leading run of bold, non-italic text starting at the first visible glyph.
    pub bold_prefix: String,
    /// Byte ranges of `text` drawn bold and not italic, in order.
    pub bold_spans: Vec<(usize, usize)>,
}

impl StyledLine {
    pub fn indent(&self) -> usize {
        self.text.chars().take_while(|c| *c == ' ').count()
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Style {
    bold: bool,
    italic: bool,
    bg: bool,
}

pub fn parse_ansi(input: &str) -> Vec<StyledLine> {
    let mut parser = Parser::<Utf8Parser>::new();
    let mut collector = Collector::default();
    for byte in input.bytes() {
        parser.advance(&mut collector, byte);
    }
    collector.finish_line();
    collector.lines
}

#[derive(Default)]
struct Collector {
    lines: Vec<StyledLine>,
    line: StyledLine,
    style: Style,
    seen_glyph: bool,
    in_bold_prefix: bool,
}

impl Collector {
    fn finish_line(&mut self) {
        let mut line = std::mem::take(&mut self.line);
        line.text.truncate(line.text.trim_end().len());
        line.bold_prefix.truncate(line.bold_prefix.trim_end().len());
        let len = line.text.len();
        line.bold_spans = line
            .bold_spans
            .iter()
            .map(|&(start, end)| (start.min(len), end.min(len)))
            .filter(|&(start, end)| start < end)
            .collect();
        self.lines.push(line);
        // Rows are decoded independently so a style left open on one row
        // (e.g. a prompt background) cannot mark the next row as a user prompt.
        self.style = Style::default();
        self.seen_glyph = false;
        self.in_bold_prefix = false;
    }
}

impl Perform for Collector {
    fn print(&mut self, c: char) {
        if self.line.text.is_empty() {
            self.line.bg_at_start = self.style.bg;
        }
        let plain_bold = self.style.bold && !self.style.italic;
        if !self.seen_glyph && !c.is_whitespace() {
            self.seen_glyph = true;
            self.in_bold_prefix = plain_bold;
        }
        if self.in_bold_prefix {
            if plain_bold {
                self.line.bold_prefix.push(c);
            } else {
                self.in_bold_prefix = false;
            }
        }
        let start = self.line.text.len();
        self.line.text.push(c);
        if plain_bold {
            let end = self.line.text.len();
            match self.line.bold_spans.last_mut() {
                Some(span) if span.1 == start => span.1 = end,
                _ => self.line.bold_spans.push((start, end)),
            }
        }
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            self.finish_line();
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: u8) {
        if action == b'm' {
            apply_sgr(params, &mut self.style);
        }
    }
}

fn apply_sgr(params: &Params, style: &mut Style) {
    let params: Vec<&[u16]> = params.iter().collect();
    if params.is_empty() {
        *style = Style::default();
        return;
    }
    let mut i = 0;
    while i < params.len() {
        let param = params[i];
        let code = param.first().copied().unwrap_or(0);
        match code {
            0 => *style = Style::default(),
            1 => style.bold = true,
            3 => style.italic = true,
            22 => style.bold = false,
            23 => style.italic = false,
            40..=47 | 100..=107 => style.bg = true,
            49 => style.bg = false,
            38 | 48 => {
                if code == 48 {
                    style.bg = true;
                }
                // Semicolon form (`38;2;r;g;b`) spreads the color over the following params;
                // colon form (`38:2::r:g:b`) keeps it inside this one.
                if param.len() == 1 {
                    i += match params.get(i + 1).and_then(|next| next.first()) {
                        Some(5) => 2,
                        Some(2) => 4,
                        _ => 0,
                    };
                }
            }
            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_escapes_and_carriage_returns() {
        let lines = parse_ansi("\x1b[0m\x1b[38;2;1;2;3mhello\x1b[0m world  \r\nnext");
        assert_eq!(lines[0].text, "hello world");
        assert_eq!(lines[1].text, "next");
    }

    #[test]
    fn detects_background_at_start() {
        let line = &parse_ansi("\x1b[48;2;215;95;0m \x1b[0m\x1b[48;2;38;38;38m  hi")[0];
        assert!(line.bg_at_start);
        assert_eq!(line.text, "   hi");
        assert!(!parse_ansi("   \x1b[48;5;1mhi")[0].bg_at_start);
    }

    #[test]
    fn style_does_not_carry_over_lines() {
        let lines = parse_ansi("\x1b[48;5;1m prompt\n   next");
        assert!(lines[0].bg_at_start);
        assert!(!lines[1].bg_at_start);
    }

    #[test]
    fn truecolor_params_do_not_leak_into_style() {
        // 38;2;1;3;4 must not be read as bold (1) or italic (3).
        assert!(parse_ansi("\x1b[38;2;1;3;4mplain")[0]
            .bold_prefix
            .is_empty());
        assert!(parse_ansi("\x1b[38:2::1:3:4mplain")[0]
            .bold_prefix
            .is_empty());
    }

    #[test]
    fn captures_bold_prefix() {
        let line = &parse_ansi("   \x1b[0m\x1b[1m\x1b[38;2;215;135;95mWeb Fetch\x1b[0m 1 URL")[0];
        assert_eq!(line.bold_prefix, "Web Fetch");
        assert_eq!(line.indent(), 3);
        let italic = &parse_ansi("   \x1b[1m\x1b[3mnote\x1b[0m")[0];
        assert!(italic.bold_prefix.is_empty());
    }

    #[test]
    fn records_bold_spans() {
        let line = &parse_ansi("   see \x1b[1mCargo.toml\x1b[0m and \x1b[1m\x1b[3mnot\x1b[0m")[0];
        assert_eq!(line.bold_spans, [(7, 17)]);
    }

    #[test]
    fn skips_osc_and_charset_sequences() {
        let line = &parse_ansi("\x1b]8;;https://x.dev\x07link\x1b]8;;\x1b\\ \x1b(Bdone")[0];
        assert_eq!(line.text, "link done");
    }
}
