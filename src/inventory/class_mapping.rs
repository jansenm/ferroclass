// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use globset::GlobBuilder;
use regex::Regex;
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Invalid mapping format: {message}"))]
    InvalidFormat { message: String },
    #[snafu(display("Invalid glob pattern '{pattern}'"))]
    InvalidGlob {
        pattern: String,
        source: globset::Error,
    },
    #[snafu(display("Invalid regex pattern '{pattern}'"))]
    InvalidRegex {
        pattern: String,
        source: regex::Error,
    },
}

#[derive(Debug, Clone)]
pub enum MappingPattern {
    Glob(globset::GlobMatcher),
    Regex(Regex),
}

#[derive(Debug, Clone)]
pub struct ClassMapping {
    pattern: MappingPattern,
    class_names: Vec<String>,
}

impl ClassMapping {
    pub fn parse(input: &str) -> Result<Self, Error> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(Error::InvalidFormat {
                message: "mapping string is empty".to_string(),
            });
        }

        let (pattern, class_names) = if trimmed.starts_with('/') {
            parse_regex_mapping(trimmed)?
        } else {
            parse_glob_mapping(trimmed)?
        };

        if class_names.is_empty() {
            return Err(Error::InvalidFormat {
                message: format!("no class names in mapping: {input}"),
            });
        }

        Ok(Self {
            pattern,
            class_names,
        })
    }

    pub fn matches(&self, name: &str) -> Option<Vec<String>> {
        match &self.pattern {
            MappingPattern::Glob(matcher) => {
                if matcher.is_match(name) {
                    Some(self.class_names.clone())
                } else {
                    None
                }
            }
            MappingPattern::Regex(re) => {
                if let Some(caps) = re.captures(name) {
                    let expanded: Vec<String> = self
                        .class_names
                        .iter()
                        .map(|template| expand_backreferences(template, &caps))
                        .collect();
                    Some(expanded)
                } else {
                    None
                }
            }
        }
    }

    pub fn pattern_debug(&self) -> String {
        match &self.pattern {
            MappingPattern::Glob(m) => m.glob().to_string(),
            MappingPattern::Regex(re) => re.as_str().to_string(),
        }
    }

    pub fn class_names(&self) -> &[String] {
        &self.class_names
    }
}

fn parse_regex_mapping(input: &str) -> Result<(MappingPattern, Vec<String>), Error> {
    let close_slash = input[1..].find('/').ok_or_else(|| Error::InvalidFormat {
        message: format!("missing closing / in regex mapping: {input}"),
    })?;
    let pattern_str = &input[1..close_slash + 1];
    let rest = &input[close_slash + 2..];

    let re = Regex::new(pattern_str).map_err(|e| Error::InvalidRegex {
        pattern: pattern_str.to_string(),
        source: e,
    })?;

    let class_names: Vec<String> = rest.split_whitespace().map(|s| s.to_string()).collect();

    Ok((MappingPattern::Regex(re), class_names))
}

fn parse_glob_mapping(input: &str) -> Result<(MappingPattern, Vec<String>), Error> {
    let mut parts = input.split_whitespace();
    let pattern_str = parts
        .next()
        .ok_or_else(|| Error::InvalidFormat {
            message: format!("no glob pattern in mapping: {input}"),
        })?
        .to_string();

    let class_names: Vec<String> = parts.map(|s| s.to_string()).collect();

    let glob = GlobBuilder::new(&pattern_str)
        .literal_separator(true)
        .build()
        .map_err(|e| Error::InvalidGlob {
            pattern: pattern_str.clone(),
            source: e,
        })?;

    Ok((MappingPattern::Glob(glob.compile_matcher()), class_names))
}

fn expand_backreferences(template: &str, caps: &regex::Captures<'_>) -> String {
    let mut result = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_digit() {
                    chars.next();
                    let group_index = next.to_digit(10).unwrap() as usize;
                    if let Some(m) = caps.get(group_index) {
                        result.push_str(m.as_str());
                    }
                } else {
                    result.push(next);
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn resolve_class_mappings(
    mappings: &[ClassMapping],
    node_name: &str,
    node_pathname: Option<&str>,
    match_path: bool,
) -> Vec<String> {
    let matchname = if match_path {
        node_pathname.unwrap_or(node_name)
    } else {
        node_name
    };

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for mapping in mappings {
        if let Some(matched_classes) = mapping.matches(matchname) {
            for class_name in matched_classes {
                if seen.insert(class_name.clone()) {
                    result.push(class_name);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_glob_mapping() {
        let mapping = ClassMapping::parse("* default").unwrap();
        assert_eq!(mapping.class_names(), &["default"]);
    }

    #[test]
    fn test_parse_glob_mapping_multiple_classes() {
        let mapping = ClassMapping::parse("*.ch hosted@switzerland another_class").unwrap();
        assert_eq!(
            mapping.class_names(),
            &["hosted@switzerland", "another_class"]
        );
    }

    #[test]
    fn test_parse_regex_mapping() {
        let mapping = ClassMapping::parse("/^www\\d+$/ webserver").unwrap();
        assert_eq!(mapping.class_names(), &["webserver"]);
    }

    #[test]
    fn test_parse_regex_mapping_multiple_classes() {
        let mapping = ClassMapping::parse("/^db-.+$/ database backup").unwrap();
        assert_eq!(mapping.class_names(), &["database", "backup"]);
    }

    #[test]
    fn test_parse_empty_mapping() {
        assert!(ClassMapping::parse("").is_err());
        assert!(ClassMapping::parse("   ").is_err());
    }

    #[test]
    fn test_parse_mapping_no_class_names() {
        assert!(ClassMapping::parse("justapattern").is_err());
    }

    #[test]
    fn test_parse_regex_unclosed_slash() {
        assert!(ClassMapping::parse("/^www webserver").is_err());
    }

    #[test]
    fn test_glob_match_star() {
        let mapping = ClassMapping::parse("node* two").unwrap();
        assert_eq!(mapping.matches("node1"), Some(vec!["two".to_string()]));
        assert_eq!(mapping.matches("node2"), Some(vec!["two".to_string()]));
        assert_eq!(mapping.matches("alpha1"), None);
    }

    #[test]
    fn test_glob_match_path_pattern() {
        let mapping = ClassMapping::parse("alpha/node* three").unwrap();
        assert_eq!(
            mapping.matches("alpha/node1"),
            Some(vec!["three".to_string()])
        );
        assert_eq!(mapping.matches("node1"), None);
    }

    #[test]
    fn test_glob_no_match() {
        let mapping = ClassMapping::parse("*.ch hosted").unwrap();
        assert_eq!(mapping.matches("node1.de"), None);
    }

    #[test]
    fn test_regex_match() {
        let mapping = ClassMapping::parse("/^www\\d+$/ webserver").unwrap();
        assert_eq!(mapping.matches("www1"), Some(vec!["webserver".to_string()]));
        assert_eq!(
            mapping.matches("www42"),
            Some(vec!["webserver".to_string()])
        );
        assert_eq!(mapping.matches("www"), None);
        assert_eq!(mapping.matches("wwwx"), None);
    }

    #[test]
    fn test_regex_partial_match() {
        let mapping = ClassMapping::parse("/\\.ch$/ swiss").unwrap();
        assert_eq!(
            mapping.matches("myhost.ch"),
            Some(vec!["swiss".to_string()])
        );
        assert_eq!(mapping.matches("myhost.de"), None);
    }

    #[test]
    fn test_regex_backreference() {
        let mapping = ClassMapping::parse("/\\.(\\S+)$/ tld-\\1").unwrap();
        let result = mapping.matches("myhost.ch");
        assert!(result.is_some());
        let classes = result.unwrap();
        assert_eq!(classes[0], "tld-ch");
    }

    #[test]
    fn test_regex_backreference_numbered() {
        let mapping = ClassMapping::parse("/^([^.]+)\\.(.+)$/ sub-\\1 tld-\\2").unwrap();
        let result = mapping.matches("www.example.com");
        assert!(result.is_some());
        let classes = result.unwrap();
        assert_eq!(classes[0], "sub-www");
        assert_eq!(classes[1], "tld-example.com");
    }

    #[test]
    fn test_glob_literal_separator_true() {
        let mapping = ClassMapping::parse("alpha/node* three").unwrap();
        assert!(mapping.matches("alpha/node1").is_some());
        assert!(mapping.matches("alpha/node2").is_some());
        assert!(mapping.matches("alphaXnode1").is_none());
    }

    #[test]
    fn test_resolve_class_mappings_basic() {
        let m1 = ClassMapping::parse("node* two").unwrap();
        let m2 = ClassMapping::parse("alpha/node* three").unwrap();

        let result = resolve_class_mappings(&[m1, m2], "node1", None, false);
        assert_eq!(result, vec!["two"]);
    }

    #[test]
    fn test_resolve_class_mappings_match_path() {
        let m1 = ClassMapping::parse("node* two").unwrap();
        let m2 = ClassMapping::parse("alpha/node* three").unwrap();

        let result = resolve_class_mappings(&[m1, m2], "node1", Some("alpha/node1"), true);
        assert_eq!(result, vec!["three"]);
    }

    #[test]
    fn test_resolve_class_mappings_match_path_false() {
        let m1 = ClassMapping::parse("node* two").unwrap();
        let m2 = ClassMapping::parse("alpha/node* three").unwrap();

        let result = resolve_class_mappings(&[m1, m2], "node1", Some("alpha/node1"), false);
        assert_eq!(result, vec!["two"]);
    }

    #[test]
    fn test_resolve_class_mappings_dedup() {
        let m1 = ClassMapping::parse("node* two").unwrap();
        let m2 = ClassMapping::parse("n* two three").unwrap();

        let result = resolve_class_mappings(&[m1, m2], "node1", None, false);
        assert_eq!(result, vec!["two", "three"]);
    }

    #[test]
    fn test_resolve_class_mappings_no_match() {
        let m1 = ClassMapping::parse("*.ch swiss").unwrap();

        let result = resolve_class_mappings(&[m1], "node1.de", None, false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_class_mappings_empty() {
        let result = resolve_class_mappings(&[], "node1", None, false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_resolve_class_mappings_multiple_regex() {
        let m1 = ClassMapping::parse("/^www/ web").unwrap();
        let m2 = ClassMapping::parse("/\\d+$/ numbered").unwrap();

        let result = resolve_class_mappings(&[m1, m2], "www1", None, false);
        assert_eq!(result, vec!["web", "numbered"]);
    }

    #[test]
    fn test_resolve_class_mappings_match_path_fallback_to_name() {
        let m1 = ClassMapping::parse("node* two").unwrap();

        let result = resolve_class_mappings(&[m1], "node1", None, true);
        assert_eq!(result, vec!["two"]);
    }
}
