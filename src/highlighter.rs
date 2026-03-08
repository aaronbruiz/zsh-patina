use std::io::Cursor;

use syntect::{
    easy::HighlightLines,
    highlighting::{Color, Style, Theme, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

fn to_hex(color: Color) -> String {
    format!("#{:0>2x}{:0>2x}{:0>2x}", color.r, color.g, color.b)
}

/// This is similar to how the ansi theme works in Bat
/// (https://github.com/sharkdp/bat): Colors are specified in the form #RRGGBBAA
/// where AA can have the following values:
///
/// * 00: The red channel specifies which ANSI color to use. Valid values are
///   00-07 (black, red, green, yellow, blue, magenta, cyan, white in this
///   order).
/// * 01: In this case the terminal's default foreground color is used
/// * else: the color is used as-is without the alpha channel (i.e. #RRGGBB)
fn to_ansi_color(color: Color) -> Option<String> {
    if color.a == 0 {
        Some(match color.r {
            0x00 => "black".to_string(),
            0x01 => "red".to_string(),
            0x02 => "green".to_string(),
            0x03 => "yellow".to_string(),
            0x04 => "blue".to_string(),
            0x05 => "magenta".to_string(),
            0x06 => "cyan".to_string(),
            0x07 => "white".to_string(),
            _ => to_hex(color),
        })
    } else if color.a == 1 {
        None
    } else {
        Some(to_hex(color))
    }
}

pub struct Span {
    pub start: usize,
    pub end: usize,
    pub foreground_color: String,
}

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl Highlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();

        let theme_str = include_str!("theme.tmTheme");
        let mut cursor = Cursor::new(theme_str);
        let theme = ThemeSet::load_from_reader(&mut cursor).expect("Unable to load theme");

        Self { syntax_set, theme }
    }

    pub fn highlight(&self, command: &str) -> Vec<Span> {
        if command.trim_ascii_start().starts_with("time ") {
            let rest = command.find("time ").unwrap() + 5;
            let mut spans = self.highlight_internal(&command[0..rest]);
            spans.extend(
                self.highlight_internal(&command[rest..])
                    .into_iter()
                    .map(|mut s| {
                        s.start += rest;
                        s.end += rest;
                        s
                    }),
            );
            spans
        } else {
            self.highlight_internal(command)
        }
    }

    fn highlight_internal(&self, command: &str) -> Vec<Span> {
        let syntax = self.syntax_set.find_syntax_by_extension("sh").unwrap();

        let mut h = HighlightLines::new(syntax, &self.theme);
        let mut i = 0;
        let mut result = Vec::new();
        for line in LinesWithEndings::from(command.trim_ascii_end()) {
            let ranges: Vec<(Style, &str)> = h.highlight_line(line, &self.syntax_set).unwrap();

            for r in ranges {
                let fg = to_ansi_color(r.0.foreground);

                // this is O(n) but necessary in case the command contains
                // multi-byte characters
                let len = r.1.chars().count();

                // highlighting `None` or `white` (i.e. default terminal color)
                // is not necessary
                if let Some(fg) = fg
                    && fg != "white"
                {
                    result.push(Span {
                        start: i,
                        end: i + len,
                        foreground_color: fg,
                    });
                }

                i += len;
            }
        }

        // optimization: merge consecutive spans with the same color
        let mut merged: Vec<Span> = Vec::new();
        for span in result.into_iter() {
            if let Some(prev) = merged.last_mut()
                && prev.end == span.start
                && prev.foreground_color == span.foreground_color
            {
                prev.end = span.end;
            } else {
                merged.push(span);
            }
        }

        merged
    }
}
