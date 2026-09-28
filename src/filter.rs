use regex::Regex;

use crate::error::CreditError;
use crate::git::FileDelta;

/// A compiled set of glob exclusion patterns.
pub struct ExclusionFilter {
    patterns: Vec<Regex>,
}

impl ExclusionFilter {
    /// Compile a set of glob patterns into an exclusion filter.
    pub fn new(patterns: &[String]) -> Result<Self, CreditError> {
        let compiled = patterns
            .iter()
            .map(|glob| {
                let re = glob_to_regex(glob);
                Regex::new(&re).map_err(|_| CreditError::InvalidGlob {
                    pattern: glob.clone(),
                    reason: format!("failed to compile as regex: {re}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { patterns: compiled })
    }

    /// Returns true if the given file path should be excluded.
    pub fn is_excluded(&self, path: &str) -> bool {
        self.patterns.iter().any(|re| re.is_match(path))
    }

    /// Sum the additions and deletions of the non-excluded deltas.
    pub fn line_totals(&self, deltas: &[FileDelta]) -> (u64, u64) {
        deltas
            .iter()
            .filter(|d| !self.is_excluded(&d.path))
            .fold((0, 0), |(a, d), f| (a + f.additions, d + f.deletions))
    }
}

/// Translate a glob into a regex matching the whole path.
///
/// `*` and `?` match within one path component, `**/` matches zero or more
/// directories, any other `**` matches anything. All other characters, `[` included,
/// are literal.
fn glob_to_regex(glob: &str) -> String {
    let mut regex = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '*' if i + 1 < chars.len() && chars[i + 1] == '*' => {
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
        assert!(!filter.is_excluded("anything"));
    }

    #[test]
    fn line_totals_skips_excluded() {
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
        assert_eq!(filter.line_totals(&deltas), (10, 5));
    }
}
