use std::{
    ops::Range,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use syntect::{
    easy::HighlightLines,
    highlighting::Theme as SyntectTheme,
    parsing::{ClearAmount, ParseState, ScopeStackOp, SyntaxSet},
    util::LinesWithEndings,
};

use super::dynamic::*;
use super::*;
use crate::{
    HighlightingConfig,
    theme::{ScopeMapping, Style, Theme, ThemeSource},
};

/// If the command starts with a prefix keyword (e.g. `time`), returns the byte
/// offset where the rest of the command begins. This can be used to split the
/// command and process the prefix and the rest separately.
fn find_prefix_split(command: &str) -> Option<usize> {
    if command.trim_ascii_start().starts_with("time ") {
        Some(command.find("time ").unwrap() + 5)
    } else {
        None
    }
}

/// Lookup a scope in a theme and convert the retrieved style to a
/// [`StaticStyle`] struct
fn resolve_static_style(scope: &str, theme: &Theme) -> Option<StaticStyle> {
    let style = theme.resolve(scope)?;

    let fg = style.foreground.map(|c| c.to_ansi_color());
    let bg = style.background.map(|c| c.to_ansi_color());

    if fg.is_none() && bg.is_none() && !style.bold && !style.underline {
        None
    } else {
        Some(StaticStyle {
            foreground_color: fg,
            background_color: bg,
            bold: style.bold,
            underline: style.underline,
        })
    }
}

fn insert_marker_style(theme: &mut Theme, scope: &str) {
    if !theme.contains(scope) {
        if let Some(style) = theme.resolve(scope) {
            theme.insert(scope.to_string(), style);
        } else {
            theme.insert(scope.to_string(), Style::default());
        }
    }
}

fn insert_marker_style_with_fallback(theme: &mut Theme, scope: &str, fallback: &str) {
    if !theme.contains(scope) {
        if let Some(style) = theme.resolve(fallback) {
            theme.insert(scope.to_string(), style);
        } else {
            theme.insert(scope.to_string(), Style::default());
        }
    }
}

pub fn update_groups<'a>(
    scope: &'a str,
    range: &Range<usize>,
    groups: &mut Vec<DynamicTokenGroup<'a>>,
) {
    // try to extend last group
    let dynamic_type = match scope {
        ARGUMENTS | STRING_QUOTED_DOUBLE_ARGUMENTS => {
            if let Some(group) = groups.last_mut()
                && group.range.end == range.start
                && group.dynamic_type != DynamicType::Callable
            {
                group.range.end = range.end;
                group.dynamic_type = DynamicType::Arguments;
                group.tokens.push(DynamicToken::new(range, scope));
                None
            } else {
                Some(DynamicType::Arguments)
            }
        }

        CALLABLE | STRING_QUOTED_DOUBLE_CALLABLE => {
            if let Some(group) = groups.last_mut()
                && group.range.end == range.start
                && group.dynamic_type != DynamicType::Arguments
            {
                group.range.end = range.end;
                group.dynamic_type = DynamicType::Callable;
                group.tokens.push(DynamicToken::new(range, scope));
                None
            } else {
                Some(DynamicType::Callable)
            }
        }

        CHARACTER_ESCAPE => {
            if let Some(group) = groups.last_mut()
                && group.range.end == range.start
            {
                group.range.end = range.end;
                group.tokens.push(DynamicToken::new(range, scope));
                None
            } else {
                Some(DynamicType::Unknown(CHARACTER_ESCAPE))
            }
        }

        TILDE => Some(DynamicType::Unknown(TILDE)),

        _ => None,
    };

    // create new group if necessary
    if let Some(dynamic_type) = dynamic_type {
        groups.push(DynamicTokenGroup::new(range, dynamic_type, scope));
    }
}

pub struct Highlighter {
    max_line_length: usize,
    timeout: Duration,
    syntax_set: SyntaxSet,
    theme: Theme,
    scope_mapping: ScopeMapping,
    syntect_theme: SyntectTheme,
    callable_choices: Vec<(String, StaticStyle)>,
}

impl Highlighter {
    pub fn new(config: &HighlightingConfig) -> Result<Self> {
        let syntax_set: SyntaxSet = syntect::dumps::from_uncompressed_data(include_bytes!(
            concat!(env!("OUT_DIR"), "/syntax_set.packdump")
        ))
        .expect("Unable to load shell syntax");

        let mut theme = Theme::load(&config.theme)?;

        // Insert dummy style for callables into the theme. We need it as a
        // marker so Syntect returns a token for it.
        insert_marker_style(&mut theme, CALLABLE);

        // Do the same for other scopes
        insert_marker_style(&mut theme, ARGUMENTS);
        insert_marker_style(&mut theme, CHARACTER_ESCAPE);
        insert_marker_style(&mut theme, TILDE);
        insert_marker_style_with_fallback(
            &mut theme,
            STRING_QUOTED_DOUBLE_CALLABLE,
            STRING_QUOTED_DOUBLE,
        );
        insert_marker_style_with_fallback(
            &mut theme,
            STRING_QUOTED_DOUBLE_ARGUMENTS,
            STRING_QUOTED_DOUBLE,
        );

        let scope_mapping = ScopeMapping::new(&theme);

        let syntect_theme =
            theme
                .to_syntect(&scope_mapping)
                .with_context(|| match &config.theme {
                    ThemeSource::Lavender => "Failed to parse lavender theme".to_string(),
                    ThemeSource::Nord => "Failed to parse nord theme".to_string(),
                    ThemeSource::Patina => "Failed to parse default theme".to_string(),
                    ThemeSource::Simple => "Failed to parse simple theme".to_string(),
                    ThemeSource::TokyoNight => "Failed to parse tokyonight theme".to_string(),
                    ThemeSource::File(path) => format!("Failed to parse theme file `{path}'"),
                })?;

        let mut callable_choices: FxHashMap<StaticStyle, String> = FxHashMap::default();
        if let Some(alias_style) = resolve_static_style(DYNAMIC_CALLABLE_ALIAS, &theme) {
            callable_choices.entry(alias_style).or_default().push('a');
        }
        if let Some(builtin_style) = resolve_static_style(DYNAMIC_CALLABLE_BUILTIN, &theme) {
            callable_choices.entry(builtin_style).or_default().push('b');
        }
        if let Some(command_style) = resolve_static_style(DYNAMIC_CALLABLE_COMMAND, &theme) {
            callable_choices.entry(command_style).or_default().push('c');
        }
        if let Some(function_style) = resolve_static_style(DYNAMIC_CALLABLE_FUNCTION, &theme) {
            callable_choices
                .entry(function_style)
                .or_default()
                .push('f');
        }
        if let Some(missing_style) = resolve_static_style(DYNAMIC_CALLABLE_MISSING, &theme) {
            callable_choices.entry(missing_style).or_default().push('m');
        }
        if let Some(else_style) = resolve_static_style(CALLABLE, &theme) {
            callable_choices.entry(else_style).or_default().push('e');
        }
        let callable_choices = callable_choices
            .into_iter()
            .map(|(k, v)| (v, k))
            .collect::<Vec<_>>();

        Ok(Self {
            max_line_length: config.max_line_length,
            timeout: config.timeout,
            syntax_set,
            theme,
            scope_mapping,
            syntect_theme,
            callable_choices,
        })
    }

    /// Return a list of dynamic style choices the plugin has for callables
    pub fn callable_choices(&self) -> &[(String, StaticStyle)] {
        &self.callable_choices
    }

    pub fn highlight<P>(&self, command: &str, pwd: Option<&str>, predicate: P) -> Result<Vec<Span>>
    where
        P: Fn(&Range<usize>) -> bool + Copy,
    {
        if let Some(rest) = find_prefix_split(command) {
            let mut spans = self.highlight_internal(&command[0..rest], pwd, predicate)?;
            spans.extend(
                self.highlight(&command[rest..], pwd, predicate)?
                    .into_iter()
                    .map(|mut s| {
                        s.start += rest;
                        s.end += rest;
                        s
                    }),
            );
            Ok(spans)
        } else {
            self.highlight_internal(command, pwd, predicate)
        }
    }

    fn highlight_internal<P>(
        &self,
        command: &str,
        pwd: Option<&str>,
        predicate: P,
    ) -> Result<Vec<Span>>
    where
        P: Fn(&Range<usize>) -> bool,
    {
        let start = Instant::now();

        let syntax = self.syntax_set.find_syntax_by_extension("sh").unwrap();

        let mut h = HighlightLines::new(syntax, &self.syntect_theme);
        let mut i = 0;
        let mut result = Vec::new();
        for line in LinesWithEndings::from(command.trim_ascii_end()) {
            if line.len() > self.max_line_length {
                // skip lines that are too long
                continue;
            }

            if start.elapsed() > self.timeout {
                // stop if highlighting takes too long
                break;
            }

            let ranges = h.highlight_line(line, &self.syntax_set)?;

            let mut groups = Vec::new();

            for r in ranges {
                if r.1.is_empty() {
                    continue;
                }

                // this is O(n) but necessary in case the command contains
                // multi-byte characters
                let len = r.1.chars().count();

                if let Some(scope) = self.scope_mapping.decode(&r.0.foreground) {
                    let range = i..i + len;
                    if predicate(&range) {
                        update_groups(scope, &range, &mut groups);
                        self.highlight_other(range, scope, &mut result);
                    }
                }

                i += len;
            }

            // TODO process all groups and update result
            // println!("GROUPS: {groups:?}");
            // println!("RESULT {result:?}");
        }

        Ok(result)
    }

    fn highlight_other(&self, range: Range<usize>, scope: &str, result: &mut Vec<Span>) {
        if let Some(style) = resolve_static_style(scope, &self.theme) {
            result.push(Span {
                start: range.start,
                end: range.end,
                style: SpanStyle::Static(style),
            });
        }
    }

    pub fn tokenize(&self, command: &str) -> Result<Vec<Token>> {
        if let Some(rest) = find_prefix_split(command) {
            let mut tokens = self.tokenize_internal(&command[0..rest])?;
            tokens.extend(self.tokenize(&command[rest..])?.into_iter().map(|mut t| {
                if t.line == 1 {
                    t.column += rest;
                }
                t.range = (t.range.start + rest)..(t.range.end + rest);
                t
            }));
            Ok(tokens)
        } else {
            self.tokenize_internal(command)
        }
    }

    fn tokenize_internal(&self, command: &str) -> Result<Vec<Token>> {
        let syntax = self.syntax_set.find_syntax_by_extension("sh").unwrap();

        let mut offset = 0;
        let mut ps = ParseState::new(syntax);
        let mut result = Vec::new();
        let mut stack = Vec::new();
        let mut stash = Vec::new();
        for (line_number, line) in LinesWithEndings::from(command.trim_ascii_end()).enumerate() {
            let tokens = ps.parse_line(line, &self.syntax_set)?;

            for (i, s) in tokens {
                match s {
                    ScopeStackOp::Push(scope) => {
                        stack.push((
                            scope,
                            line_number + 1,
                            line[0..i].chars().count() + 1,
                            offset + i,
                        ));
                    }

                    ScopeStackOp::Pop(count) => {
                        for _ in 0..count {
                            let (scope, ln, col, start) = stack.pop().unwrap();
                            if offset + i >= start {
                                result.push(Token {
                                    scope: scope.build_string(),
                                    line: ln,
                                    column: col,
                                    range: start..offset + i,
                                });
                            }
                        }
                    }

                    ScopeStackOp::Clear(clear_amount) => {
                        // similar to ::Pop, but store popped items in stash so
                        // we can restore them if necessary
                        let count = match clear_amount {
                            ClearAmount::TopN(n) => n.min(stack.len()),
                            ClearAmount::All => stack.len(),
                        };

                        let mut to_stash = Vec::new();
                        for _ in 0..count {
                            let (scope, ln, col, start) = stack.pop().unwrap();
                            if offset + i >= start {
                                result.push(Token {
                                    scope: scope.build_string(),
                                    line: ln,
                                    column: col,
                                    range: start..offset + i,
                                });
                            }
                            to_stash.push((scope, ln, col, start));
                        }
                        stash.push(to_stash);
                    }

                    ScopeStackOp::Restore => {
                        // restore items from the stash (see ::Clear)
                        if let Some(mut s) = stash.pop() {
                            while let Some(e) = s.pop() {
                                stack.push(e);
                            }
                        }
                    }

                    ScopeStackOp::Noop => {}
                }
            }

            offset += line.len();
        }

        // consume the remaining items on the stack
        while let Some((scope, ln, col, start)) = stack.pop() {
            result.push(Token {
                scope: scope.build_string(),
                line: ln,
                column: col,
                range: start..command.len(),
            });
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, Permissions},
        os::unix::fs::PermissionsExt,
    };

    use super::*;
    use anyhow::Result;
    use pretty_assertions::assert_eq;

    fn test_config() -> HighlightingConfig {
        HighlightingConfig::default()
    }

    /// Test if a simple `echo` command is highlighted correctly
    #[test]
    fn echo() -> Result<()> {
        let highlighter = Highlighter::new(&test_config())?;
        let highlighted = highlighter.highlight("echo", None, |_| true)?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 4,
                style: SpanStyle::Dynamic(DynamicStyle::Callable)
            }]
        );
        Ok(())
    }

    /// Test if a command referring to a file is highlighted correctly
    #[test]
    fn argument_is_file() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let test_path = dir.path().join("test.txt");
        fs::write(test_path, "test contents")?;

        let highlighter = Highlighter::new(&test_config())?;
        let highlighted = highlighter.highlight(
            "cp test.txt dest.txt",
            Some(dir.path().to_str().unwrap()),
            |_| true,
        )?;

        let dynamic_file_style =
            resolve_static_style(DYNAMIC_PATH_FILE, &highlighter.theme).unwrap();

        assert_eq!(
            highlighted,
            vec![
                Span {
                    start: 0,
                    end: 2,
                    style: SpanStyle::Dynamic(DynamicStyle::Callable)
                },
                Span {
                    start: 3,
                    end: 11,
                    style: SpanStyle::Static(dynamic_file_style),
                }
            ]
        );

        Ok(())
    }

    /// Test if a command referring to a directory is highlighted correctly
    #[test]
    fn argument_is_directory() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let test_path = dir.path().join("test.txt");
        fs::write(test_path, "test contents")?;
        let dest_path = dir.path().join("dest");
        fs::create_dir(dest_path)?;

        let highlighter = Highlighter::new(&test_config())?;
        let highlighted = highlighter.highlight(
            "cp test.txt dest",
            Some(dir.path().to_str().unwrap()),
            |_| true,
        )?;

        let dynamic_file_style =
            resolve_static_style(DYNAMIC_PATH_FILE, &highlighter.theme).unwrap();
        let dynamic_directory_style =
            resolve_static_style(DYNAMIC_PATH_DIRECTORY, &highlighter.theme).unwrap();

        assert_eq!(
            highlighted,
            vec![
                Span {
                    start: 0,
                    end: 2,
                    style: SpanStyle::Dynamic(DynamicStyle::Callable)
                },
                Span {
                    start: 3,
                    end: 11,
                    style: SpanStyle::Static(dynamic_file_style),
                },
                Span {
                    start: 12,
                    end: 16,
                    style: SpanStyle::Static(dynamic_directory_style),
                }
            ]
        );

        Ok(())
    }

    /// Test if a command starting with a tilde is highlighted correctly
    #[test]
    fn command_with_tilde() -> Result<()> {
        let dir = tempfile::tempdir()?;

        let highlighter = Highlighter::new(&test_config())?;
        let tilde_style = resolve_static_style(TILDE, &highlighter.theme).unwrap();
        let dynamic_command_style =
            resolve_static_style(DYNAMIC_CALLABLE_COMMAND, &highlighter.theme).unwrap();

        let highlighted =
            highlighter.highlight("~", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 1,
                style: SpanStyle::Static(tilde_style.clone())
            }]
        );

        let highlighted =
            highlighter.highlight("~/", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 2,
                style: SpanStyle::Static(dynamic_command_style)
            }]
        );

        let highlighted =
            highlighter.highlight("~ echo", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 1,
                style: SpanStyle::Static(tilde_style)
            }]
        );

        let highlighted =
            highlighter.highlight("~doesnotexist", Some(dir.path().to_str().unwrap()), |_| {
                true
            })?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 13,
                style: SpanStyle::Dynamic(DynamicStyle::Callable)
            }]
        );

        Ok(())
    }

    /// Test if a path starting with a tilde is highlighted correctly
    #[test]
    fn path_with_tilde() -> Result<()> {
        let dir = tempfile::tempdir()?;

        let highlighter = Highlighter::new(&test_config())?;
        let tilde_style = resolve_static_style(TILDE, &highlighter.theme).unwrap();
        let dynamic_directory_style =
            resolve_static_style(DYNAMIC_PATH_DIRECTORY, &highlighter.theme).unwrap();

        let highlighted =
            highlighter.highlight("ls ~", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![
                Span {
                    start: 0,
                    end: 2,
                    style: SpanStyle::Dynamic(DynamicStyle::Callable)
                },
                Span {
                    start: 3,
                    end: 4,
                    style: SpanStyle::Static(tilde_style)
                }
            ]
        );

        let highlighted =
            highlighter.highlight("ls ~/", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![
                Span {
                    start: 0,
                    end: 2,
                    style: SpanStyle::Dynamic(DynamicStyle::Callable)
                },
                Span {
                    start: 3,
                    end: 5,
                    style: SpanStyle::Static(dynamic_directory_style)
                }
            ]
        );

        let highlighted = highlighter.highlight(
            "ls ~/this/path/does/not/exist",
            Some(dir.path().to_str().unwrap()),
            |_| true,
        )?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 2,
                style: SpanStyle::Dynamic(DynamicStyle::Callable)
            }]
        );

        Ok(())
    }

    #[test]
    fn quoted_callable() -> Result<()> {
        let dir = tempfile::tempdir()?;

        let highlighter = Highlighter::new(&test_config())?;

        let highlighted =
            highlighter.highlight("\"ls\"", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 4,
                style: SpanStyle::Dynamic(DynamicStyle::Callable)
            }]
        );

        let highlighted =
            highlighter.highlight("l\"s\"", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 4,
                style: SpanStyle::Dynamic(DynamicStyle::Callable)
            }]
        );

        let file_path = dir.path().join("script.sh");
        fs::write(&file_path, "#!/bin/sh")?;
        fs::set_permissions(&file_path, Permissions::from_mode(0o755))?;

        let dynamic_callable_style =
            resolve_static_style(DYNAMIC_CALLABLE_COMMAND, &highlighter.theme).unwrap();

        let highlighted = highlighter.highlight(
            "\"./script.sh\"",
            Some(dir.path().to_str().unwrap()),
            |_| true,
        )?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 13,
                style: SpanStyle::Static(dynamic_callable_style.clone())
            }]
        );

        let directory_path = dir.path().join("foo/bar");
        fs::create_dir_all(&directory_path)?;

        let highlighted =
            highlighter.highlight("foo/\"bar\"/", Some(dir.path().to_str().unwrap()), |_| true)?;
        assert_eq!(
            highlighted,
            vec![Span {
                start: 0,
                end: 10,
                style: SpanStyle::Static(dynamic_callable_style)
            }]
        );

        Ok(())
    }
}
