use std::ops::Range;

use super::Span;

#[derive(Debug)]
pub struct DynamicToken<'a> {
    range: Range<usize>,
    scope: &'a str,
}

impl<'a> DynamicToken<'a> {
    pub fn new(range: &Range<usize>, scope: &'a str) -> Self {
        Self {
            range: range.clone(),
            scope,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DynamicType<'a> {
    Unknown(&'a str),
    Callable,
    Arguments,
}

#[derive(Debug)]
pub struct DynamicTokenGroup<'a> {
    pub range: Range<usize>,
    pub dynamic_type: DynamicType<'a>,
    pub tokens: Vec<DynamicToken<'a>>,
}

impl<'a> DynamicTokenGroup<'a> {
    pub fn new(range: &Range<usize>, dynamic_type: DynamicType<'a>, scope: &'a str) -> Self {
        Self {
            range: range.clone(),
            dynamic_type,
            tokens: vec![DynamicToken::new(range, scope)],
        }
    }

    pub fn highlight(&self) -> Vec<Span> {
        todo!()
    }
}
