use regex::Regex;

use crate::error::CreditError;
use crate::git::FileDelta;

/// A compiled set of glob exclusion patterns.
pub struct ExclusionFilter {
    patterns: Vec<(String, Regex)>,
}

impl ExclusionFilter {
    /// Compile a set of glob patterns into an exclusion filter.
    pub fn new(patterns: &[String]) -> Result<Self, CreditError> {
        let compiled = patterns
            .iter()
            .map(|glob| {
                let re = glob_to_regex(glob);
                Regex::new(&re)
                    .map(|r| (glob.clone(), r))
                    .map_err(|_| CreditError::InvalidGlob {
                        pattern: glob.clone(),
                        reason: format!("failed to compile as regex: {re}"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { patterns: compiled })
    }

    /// Returns true if there are no exclusion patterns.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Returns true if the given file path should be excluded.
    pub fn is_excluded(&self, path: &str) -> bool {
        self.patterns.iter().any(|(_, re)| re.is_match(path))
    }

    /// Filter a list of file deltas, removing excluded files.
    pub fn filter_deltas(&self, deltas: Vec<FileDelta>) -> Vec<FileDelta> {
        if self.is_empty() {
            return deltas;
        }
        deltas
            .into_iter()
            .filter(|d| !self.is_excluded(&d.path))
            .collect()
    }
}

/// Convert a glob pattern to a regex pattern.
///
/// Supported syntax:
/// - `*` matches any sequence of non-`/` characters
/// - `**` matches any sequence of characters including `/`
/// - `?` matches any single non-`/` character
/// - All other characters are escaped as regex literals
fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                // ** matches everything including /
                if i + 2 < chars.len() && chars[i + 2] == '/' {
                    regex.push_str("(.*/)?");
                    i += 3;
                } else {
                    regex.push_str(".*");
                    i += 2;
                }
            }
            '*' => {
                regex.push_str("[^/]*");
                i += 1;
            }
            '?' => {
                regex.push_str("[^/]");
                i += 1;
            }
            c => {
                if is_regex_meta(c) {
                    regex.push('\\');
                }
                regex.push(c);
                i += 1;
            }
        }
    }

    regex.push('$');
    regex
}

fn is_regex_meta(c: char) -> bool {
    matches!(
        c,
        '\\' | '.' | '+' | '^' | '$' | '|' | '(' | ')' | '[' | ']' | '{' | '}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star_matches_filename() {
        let filter = ExclusionFilter::new(&["*.lock".into()]).unwrap();
        assert!(filter.is_excluded("Cargo.lock"));
        assert!(filter.is_excluded("uv.lock"));
        assert!(!filter.is_excluded("src/main.rs"));
        assert!(!filter.is_excluded("locks/file.txt"));
    }

    #[test]
    fn glob_double_star_matches_nested() {
        let filter = ExclusionFilter::new(&["**/*.generated.rs".into()]).unwrap();
        assert!(filter.is_excluded("src/deep/file.generated.rs"));
        assert!(filter.is_excluded("file.generated.rs"));
        assert!(!filter.is_excluded("src/main.rs"));
    }

    #[test]
    fn glob_directory_prefix() {
        let filter = ExclusionFilter::new(&["docs/*".into()]).unwrap();
        assert!(filter.is_excluded("docs/README.md"));
        assert!(!filter.is_excluded("src/docs/foo"));
    }

    #[test]
    fn glob_question_mark() {
        let filter = ExclusionFilter::new(&["file?.txt".into()]).unwrap();
        assert!(filter.is_excluded("file1.txt"));
        assert!(filter.is_excluded("fileA.txt"));
        assert!(!filter.is_excluded("file10.txt"));
    }

    #[test]
    fn multiple_patterns() {
        let filter = ExclusionFilter::new(&["*.lock".into(), "docs/*".into()]).unwrap();
        assert!(filter.is_excluded("Cargo.lock"));
        assert!(filter.is_excluded("docs/index.html"));
        assert!(!filter.is_excluded("src/main.rs"));
    }

    #[test]
    fn empty_filter_excludes_nothing() {
        let filter = ExclusionFilter::new(&[]).unwrap();
        assert!(filter.is_empty());
        assert!(!filter.is_excluded("anything"));
    }

    #[test]
    fn filter_deltas_removes_excluded() {
        let filter = ExclusionFilter::new(&["*.lock".into()]).unwrap();
        let deltas = vec![
            FileDelta {
                path: "src/main.rs".into(),
                additions: 10,
                deletions: 5,
            },
            FileDelta {
                path: "Cargo.lock".into(),
                additions: 100,
                deletions: 50,
            },
        ];
        let filtered = filter.filter_deltas(deltas);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, "src/main.rs");
    }
}
