use std::ops::Range;

mod dynamic;
mod highlighter;

pub use highlighter::Highlighter;

const ARGUMENTS: &str = "meta.function-call.arguments.shell";
const DYNAMIC_PATH_DIRECTORY: &str = "dynamic.path.directory.shell";
const DYNAMIC_PATH_FILE: &str = "dynamic.path.file.shell";

const CALLABLE: &str = "variable.function.shell";
const DYNAMIC_CALLABLE_ALIAS: &str = "dynamic.callable.alias.shell";
const DYNAMIC_CALLABLE_BUILTIN: &str = "dynamic.callable.builtin.shell";
const DYNAMIC_CALLABLE_COMMAND: &str = "dynamic.callable.command.shell";
const DYNAMIC_CALLABLE_FUNCTION: &str = "dynamic.callable.function.shell";
const DYNAMIC_CALLABLE_MISSING: &str = "dynamic.callable.missing.shell";

const CHARACTER_ESCAPE: &str = "constant.character.escape.shell";
const TILDE: &str = "variable.language.tilde.shell";

const STRING_QUOTED_DOUBLE: &str = "string.quoted.double.shell";
const STRING_QUOTED_DOUBLE_CALLABLE: &str = "variable.function.shell string.quoted.double.shell";
const STRING_QUOTED_DOUBLE_ARGUMENTS: &str =
    "meta.function-call.arguments.shell string.quoted.double.shell";

/// A span of text with a foreground color. The range is specified in terms of
/// character indices, not byte indices.
#[derive(PartialEq, Eq, Debug)]
pub struct Span {
    /// The starting character index of the span (inclusive)
    pub start: usize,

    /// The ending character index of the span (exclusive)
    pub end: usize,

    /// The span's style
    pub style: SpanStyle,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct StaticStyle {
    /// The foreground color of the span
    pub foreground_color: Option<String>,

    /// The background color of the span
    pub background_color: Option<String>,

    /// `true` if the text should be shown in bold
    pub bold: bool,

    /// `true` if the text should be shown underlined
    pub underline: bool,
}

#[derive(PartialEq, Eq, Debug)]
pub enum DynamicStyle {
    Callable,
}

#[derive(PartialEq, Eq, Debug)]
pub enum SpanStyle {
    Static(StaticStyle),
    Dynamic(DynamicStyle),
}

/// A token with a scope, line and column number, and range in the input command
/// (byte indices). The line and column numbers are 1-based.
pub struct Token {
    /// The scope of the token (e.g. `keyword.control.for.shell`)
    pub scope: String,

    /// The line number of the token (1-based)
    pub line: usize,

    /// The column of the token (1-based)
    pub column: usize,

    /// The range of the token in the input command (byte indices)
    pub range: Range<usize>,
}
