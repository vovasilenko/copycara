//! `.copycara/.ignore` — user-configurable file exclusion from public (shadow) commits.
//!
//! Works like `.gitignore` but applies during DLP shadow commit creation.
//! By default, `.copycara/` itself is excluded to hide the fact Copycara is in use.
//! Users can add additional patterns (secrets, keys, binaries) to prevent them
//! from ever reaching public remotes, even in cleaned form.

use std::fs;
use std::path::Path;

const IGNORE_FILE: &str = ".copycara/.ignore";

const DEFAULT_RULES: &str = "\
# Copycara DLP ignore file — files matching these patterns are excluded from public (shadow) commits.\n\
# Works like .gitignore: one pattern per line, # for comments, * wildcard.\n\
#\n\
# Default: exclude .copycara/ so nobody knows Copycara is used.\n\
/.copycara/\n";

#[derive(Debug, Clone)]
pub struct IgnoreRules {
    patterns: Vec<String>,
}

impl IgnoreRules {
    pub fn load() -> Self {
        let content = fs::read_to_string(IGNORE_FILE).unwrap_or_else(|_| DEFAULT_RULES.to_string());
        Self::parse(&content)
    }

    fn parse(content: &str) -> Self {
        let patterns = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect();

        Self { patterns }
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        let normalized = normalize_path(path);
        if normalized.is_empty() {
            return false;
        }

        self.patterns.iter().any(|p| pattern_matches(p, &normalized))
    }

    pub fn default_content() -> &'static str {
        DEFAULT_RULES
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().trim_start_matches("./").trim_end_matches('/').to_string()
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches('/').trim_end_matches('/');

    if pattern == path {
        return true;
    }

    if pattern.contains('*') {
        glob_match(pattern, path)
    } else {
        // Literal prefix or suffix match
        path == pattern || path.ends_with(&format!("/{pattern}")) || path.starts_with(pattern)
    }
}

/// Simple glob matching supporting `*` (matches any chars except `/`).
fn glob_match(glob: &str, text: &str) -> bool {
    let parts: Vec<&str> = glob.split('*').collect();

    if parts.len() == 1 {
        return text == parts[0];
    }

    // Anchored at start: first part must match prefix
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            // Last part: must match suffix
            let remaining = &text[pos..];
            if !remaining.ends_with(part) {
                return false;
            }
        } else {
            // Middle part: find next occurrence
            if let Some(idx) = text[pos..].find(part) {
                pos += idx + part.len();
            } else {
                return false;
            }
        }
    }

    // If last part was empty (* at end), anything matches
    if parts.last().is_some_and(|p| p.is_empty()) {
        return true;
    }

    // Ensure full match
    parts.last().is_none_or(|p| p.is_empty()) || pos <= text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rules_ignore_copycara() {
        let rules = IgnoreRules::parse(DEFAULT_RULES);
        assert!(rules.is_ignored(Path::new(".copycara/config.toml")));
        assert!(rules.is_ignored(Path::new(".copycara/.ignore")));
        assert!(rules.is_ignored(Path::new(".copycara/mirror/some.py")));
        assert!(!rules.is_ignored(Path::new("src/main.rs")));
        assert!(!rules.is_ignored(Path::new("copycara.py")));
    }

    #[test]
    fn test_empty_patterns_ignore_nothing() {
        let rules = IgnoreRules::parse("");
        assert!(!rules.is_ignored(Path::new(".copycara/config.toml")));
        assert!(!rules.is_ignored(Path::new("src/main.py")));
    }

    #[test]
    fn test_comments_and_blanks_ignored() {
        let rules = IgnoreRules::parse("# comment\n\n*.pem\n# another\n\n*.key\n");
        assert!(rules.is_ignored(Path::new("secret.pem")));
        assert!(rules.is_ignored(Path::new("nested/secret.pem")));
        assert!(rules.is_ignored(Path::new("any/nested/secret.key")));
        assert!(!rules.is_ignored(Path::new("src/main.rs")));
    }

    #[test]
    fn test_extension_pattern() {
        let rules = IgnoreRules::parse("*.log");
        assert!(rules.is_ignored(Path::new("debug.log")));
        assert!(rules.is_ignored(Path::new("logs/error.log")));
        assert!(!rules.is_ignored(Path::new("log.py")));
    }

    #[test]
    fn test_literal_pattern() {
        let rules = IgnoreRules::parse("secrets/api-key.txt");
        assert!(rules.is_ignored(Path::new("secrets/api-key.txt")));
        assert!(!rules.is_ignored(Path::new("other/api-key.txt")));
    }

    #[test]
    fn test_anchored_pattern() {
        let rules = IgnoreRules::parse("/.copycara/");
        assert!(rules.is_ignored(Path::new(".copycara/config.toml")));
        assert!(rules.is_ignored(Path::new(".copycara/")));
        assert!(!rules.is_ignored(Path::new("src/.copycara/file")));
    }
}
