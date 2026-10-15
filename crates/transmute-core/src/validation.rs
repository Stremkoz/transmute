//! Collecting validation problems with the location they occurred at.
//!
//! Validation reports *every* problem in one pass rather than stopping at the
//! first, so a user fixing a preset by hand sees the whole list at once.

use std::fmt;

/// One problem, located by a JSON-style path such as `outputs[1].codec.quality`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// All problems found while validating a value. Never empty.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ValidationError {
    pub issues: Vec<ValidationIssue>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.issues.len();
        write!(f, "{count} problem{}:", if count == 1 { "" } else { "s" })?;
        for issue in &self.issues {
            write!(f, "\n  - {issue}")?;
        }
        Ok(())
    }
}

/// Accumulates issues while walking a structure, tracking the current path.
#[derive(Debug, Default)]
pub struct Validator {
    path: Vec<String>,
    issues: Vec<ValidationIssue>,
}

impl Validator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `check` with `segment` appended to the current path.
    /// Use `"name"` for fields and `"[3]"` for list indices.
    pub fn at(&mut self, segment: impl Into<String>, check: impl FnOnce(&mut Self)) {
        self.path.push(segment.into());
        check(self);
        self.path.pop();
    }

    pub fn issue(&mut self, message: impl Into<String>) {
        self.issues.push(ValidationIssue {
            path: self.current_path(),
            message: message.into(),
        });
    }

    /// Records an issue at `field` if `value` lies outside `range`.
    pub fn check_range<T>(&mut self, field: &str, value: T, range: std::ops::RangeInclusive<T>)
    where
        T: PartialOrd + fmt::Display + Copy,
    {
        if !range.contains(&value) {
            let (low, high) = (*range.start(), *range.end());
            self.at(field, |v| {
                v.issue(format!("must be between {low} and {high} (got {value})"));
            });
        }
    }

    pub fn finish(self) -> Result<(), ValidationError> {
        if self.issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError {
                issues: self.issues,
            })
        }
    }

    fn current_path(&self) -> String {
        let mut path = String::new();
        for segment in &self.path {
            if !path.is_empty() && !segment.starts_with('[') {
                path.push('.');
            }
            path.push_str(segment);
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_join_fields_and_indices() {
        let mut v = Validator::new();
        v.at("outputs", |v| {
            v.at("[1]", |v| {
                v.at("codec", |v| v.check_range("quality", 120, 0..=100))
            })
        });
        let err = v.finish().unwrap_err();
        assert_eq!(err.issues[0].path, "outputs[1].codec.quality");
        assert_eq!(err.issues[0].message, "must be between 0 and 100 (got 120)");
    }

    #[test]
    fn collects_every_issue_and_formats_a_list() {
        let mut v = Validator::new();
        v.at("name", |v| v.issue("must not be empty"));
        v.check_range("method", 9, 0..=6);
        let err = v.finish().unwrap_err();
        assert_eq!(
            err.to_string(),
            "2 problems:\n  - name: must not be empty\n  - method: must be between 0 and 6 (got 9)"
        );
    }

    #[test]
    fn no_issues_is_ok() {
        assert!(Validator::new().finish().is_ok());
    }
}
