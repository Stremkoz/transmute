//! Opt-in output naming.
//!
//! The source basename is never changed: by default conversion only swaps the
//! extension. A naming rule can add a prefix and/or suffix around the
//! basename, e.g. `archive_` + `webtest` + `_{date}` → `archive_webtest_20261015.jxl`.
//!
//! Prefix and suffix are templates. Supported tokens:
//!
//! | Token            | Expands to                                         |
//! |------------------|----------------------------------------------------|
//! | `{format}`       | Output extension, e.g. `webp`                      |
//! | `{original_ext}` | Source file's extension, e.g. `jpg`                |
//! | `{date}`         | Date per [`DateSource`], formatted per [`DateStyle`]|
//! | `{counter}`      | 1-based position in the batch, zero-padded         |
//!
//! `{{` and `}}` produce literal braces. Expansion happens at planning time.

use serde::{Deserialize, Serialize};

use crate::validation::Validator;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(optional_fields))]
pub struct NamingRule {
    // Left out of the JSON when empty, so optional in TypeScript too.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "typescript", ts(as = "Option<String>", optional))]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "typescript", ts(as = "Option<String>", optional))]
    pub suffix: String,
    #[serde(default)]
    pub date_style: DateStyle,
    #[serde(default)]
    pub date_source: DateSource,
}

/// How `{date}` is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum DateStyle {
    /// `20261015`
    #[default]
    Compact,
    /// `261015`
    Short,
    /// `2026-10-15`
    Iso,
}

/// Which date `{date}` refers to. Explicit so outputs are predictable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
pub enum DateSource {
    /// When the batch started: every file in a run gets the same date.
    #[default]
    BatchStart,
    /// The source file's last-modified date.
    SourceModified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Format,
    OriginalExt,
    Date,
    Counter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Literal(String),
    Token(Token),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error(
        "unknown token `{{{0}}}`; expected one of {{format}}, {{original_ext}}, {{date}}, {{counter}}"
    )]
    UnknownToken(String),
    #[error("unclosed `{{`; write `{{{{` for a literal brace")]
    Unclosed,
    #[error("unmatched `}}`; write `}}}}` for a literal brace")]
    Unmatched,
    #[error("contains {0}, which is not allowed in file names")]
    ForbiddenChar(String),
}

impl NamingRule {
    /// True when the rule leaves names untouched (only the extension changes).
    pub fn is_identity(&self) -> bool {
        self.prefix.is_empty() && self.suffix.is_empty()
    }

    pub fn validate(&self, v: &mut Validator) {
        for (field, template) in [("prefix", &self.prefix), ("suffix", &self.suffix)] {
            if let Err(err) = parse_template(template) {
                v.at(field, |v| v.issue(err.to_string()));
            }
        }
    }
}

/// Splits a template into literal text and tokens.
pub fn parse_template(template: &str) -> Result<Vec<Segment>, TemplateError> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '}' => return Err(TemplateError::Unmatched),
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => return Err(TemplateError::Unclosed),
                    }
                }
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                segments.push(Segment::Token(parse_token(&name)?));
            }
            c => {
                check_file_name_char(c)?;
                literal.push(c);
            }
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    Ok(segments)
}

fn parse_token(name: &str) -> Result<Token, TemplateError> {
    match name {
        "format" => Ok(Token::Format),
        "original_ext" => Ok(Token::OriginalExt),
        "date" => Ok(Token::Date),
        "counter" => Ok(Token::Counter),
        other => Err(TemplateError::UnknownToken(other.to_owned())),
    }
}

/// Rejects characters Windows forbids in file names (and path separators
/// everywhere), so a template can never escape the destination folder.
fn check_file_name_char(c: char) -> Result<(), TemplateError> {
    if c.is_control() {
        return Err(TemplateError::ForbiddenChar(format!(
            "control character {c:?}"
        )));
    }
    if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
        return Err(TemplateError::ForbiddenChar(format!("`{c}`")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal(s: &str) -> Segment {
        Segment::Literal(s.to_owned())
    }

    #[test]
    fn parses_literals_and_tokens() {
        assert_eq!(
            parse_template("archive_{date}-{counter}").unwrap(),
            [
                literal("archive_"),
                Segment::Token(Token::Date),
                literal("-"),
                Segment::Token(Token::Counter),
            ]
        );
    }

    #[test]
    fn doubled_braces_are_literal() {
        assert_eq!(
            parse_template("{{v2}}_{format}").unwrap(),
            [literal("{v2}_"), Segment::Token(Token::Format)]
        );
    }

    #[test]
    fn empty_template_has_no_segments() {
        assert!(parse_template("").unwrap().is_empty());
    }

    #[test]
    fn rejects_bad_templates() {
        assert_eq!(
            parse_template("{name}"),
            Err(TemplateError::UnknownToken("name".into()))
        );
        assert_eq!(parse_template("x_{date"), Err(TemplateError::Unclosed));
        assert_eq!(
            parse_template("{date_{date}"),
            Err(TemplateError::UnknownToken("date_{date".into()))
        );
        assert_eq!(parse_template("x}"), Err(TemplateError::Unmatched));
        assert!(matches!(
            parse_template("../escape"),
            Err(TemplateError::ForbiddenChar(_))
        ));
        assert!(matches!(
            parse_template("a:b"),
            Err(TemplateError::ForbiddenChar(_))
        ));
    }

    #[test]
    fn validate_reports_each_bad_field() {
        let rule = NamingRule {
            prefix: "{oops}".into(),
            suffix: "a/b".into(),
            ..NamingRule::default()
        };
        let mut v = Validator::new();
        rule.validate(&mut v);
        let paths: Vec<_> = v
            .finish()
            .unwrap_err()
            .issues
            .into_iter()
            .map(|i| i.path)
            .collect();
        assert_eq!(paths, ["prefix", "suffix"]);
    }

    #[test]
    fn default_rule_is_identity() {
        assert!(NamingRule::default().is_identity());
    }
}
