// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use std::fmt;

const NEGATION_PREFIX: char = '~';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applications {
    items: Vec<String>,
    negations: Vec<String>,
}

impl Applications {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            negations: Vec::new(),
        }
    }

    pub fn from_vec(items: Vec<String>) -> Self {
        let mut apps = Self::new();
        for item in items {
            apps.append_if_new(&item);
        }
        apps
    }

    pub fn as_list(&self) -> &[String] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.negations.is_empty()
    }

    pub fn append_if_new(&mut self, item: &str) {
        if let Some(name) = item.strip_prefix(NEGATION_PREFIX) {
            self.negations.push(name.to_string());
            if let Some(pos) = self.items.iter().position(|x| x == name) {
                self.items.remove(pos);
            }
        } else {
            if !self.negations.contains(&item.to_string())
                && !self.items.contains(&item.to_string())
            {
                self.items.push(item.to_string());
            }
        }
    }

    pub fn merge_unique(&mut self, other: &Applications) {
        for negation in &other.negations {
            if let Some(pos) = self.items.iter().position(|x| x == negation) {
                self.items.remove(pos);
            }
            if !self.negations.contains(negation) {
                self.negations.push(negation.clone());
            }
        }
        for item in &other.items {
            self.append_if_new(item);
        }
    }
}

impl Default for Applications {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Applications {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", item)?;
        }
        write!(f, "]")
    }
}

impl serde::Serialize for Applications {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.items.len()))?;
        for item in &self.items {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let apps = Applications::new();
        assert!(apps.is_empty());
        assert!(apps.as_list().is_empty());
    }

    #[test]
    fn test_from_vec_basic() {
        let apps = Applications::from_vec(vec!["app1".to_string(), "app2".to_string()]);
        assert_eq!(apps.as_list(), &["app1", "app2"]);
    }

    #[test]
    fn test_from_vec_dedup() {
        let apps = Applications::from_vec(vec![
            "app1".to_string(),
            "app2".to_string(),
            "app1".to_string(),
        ]);
        assert_eq!(apps.as_list(), &["app1", "app2"]);
    }

    #[test]
    fn test_from_vec_negation() {
        let apps = Applications::from_vec(vec![
            "app1".to_string(),
            "~app1".to_string(),
            "app2".to_string(),
        ]);
        assert_eq!(apps.as_list(), &["app2"]);
    }

    #[test]
    fn test_negation_removes_existing() {
        let mut apps = Applications::from_vec(vec!["app1".to_string(), "app2".to_string()]);
        apps.append_if_new("~app1");
        assert_eq!(apps.as_list(), &["app2"]);
    }

    #[test]
    fn test_negation_prevents_future_addition() {
        let mut apps = Applications::from_vec(vec!["~app1".to_string()]);
        assert!(apps.as_list().is_empty());
        apps.append_if_new("app1");
        assert!(apps.as_list().is_empty());
    }

    #[test]
    fn test_negation_no_effect_if_not_present() {
        let mut apps = Applications::from_vec(vec!["app1".to_string()]);
        apps.append_if_new("~app2");
        assert_eq!(apps.as_list(), &["app1"]);
    }

    #[test]
    fn test_merge_unique_basic() {
        let mut left = Applications::from_vec(vec!["app1".to_string()]);
        let right = Applications::from_vec(vec!["app2".to_string()]);
        left.merge_unique(&right);
        assert_eq!(left.as_list(), &["app1", "app2"]);
    }

    #[test]
    fn test_merge_unique_dedup() {
        let mut left = Applications::from_vec(vec!["app1".to_string()]);
        let right = Applications::from_vec(vec!["app1".to_string(), "app2".to_string()]);
        left.merge_unique(&right);
        assert_eq!(left.as_list(), &["app1", "app2"]);
    }

    #[test]
    fn test_merge_unique_with_negation() {
        let mut left = Applications::from_vec(vec!["app1".to_string(), "app2".to_string()]);
        let right = Applications::from_vec(vec!["~app1".to_string(), "app3".to_string()]);
        left.merge_unique(&right);
        assert_eq!(left.as_list(), &["app2", "app3"]);
    }

    #[test]
    fn test_merge_unique_negation_persists() {
        let mut left = Applications::from_vec(vec!["~app1".to_string()]);
        let right = Applications::from_vec(vec!["app1".to_string(), "app2".to_string()]);
        left.merge_unique(&right);
        assert_eq!(left.as_list(), &["app2"]);
    }

    #[test]
    fn test_merge_unique_negation_propagates() {
        let mut left = Applications::from_vec(vec!["app1".to_string(), "app2".to_string()]);
        let right = Applications::from_vec(vec!["~app1".to_string()]);
        left.merge_unique(&right);
        assert_eq!(left.as_list(), &["app2"]);

        let third = Applications::from_vec(vec!["app1".to_string(), "app3".to_string()]);
        left.merge_unique(&third);
        assert_eq!(left.as_list(), &["app2", "app3"]);
    }

    #[test]
    fn test_merge_unique_both_sides_negate() {
        let mut left = Applications::from_vec(vec!["~app1".to_string()]);
        let right = Applications::from_vec(vec!["~app2".to_string()]);
        left.merge_unique(&right);
        assert!(left.as_list().is_empty());
    }

    #[test]
    fn test_multiple_negations_in_merge() {
        let mut left = Applications::from_vec(vec![
            "app1".to_string(),
            "app2".to_string(),
            "app3".to_string(),
        ]);
        let right = Applications::from_vec(vec![
            "~app1".to_string(),
            "~app3".to_string(),
            "app4".to_string(),
        ]);
        left.merge_unique(&right);
        assert_eq!(left.as_list(), &["app2", "app4"]);
    }
}
