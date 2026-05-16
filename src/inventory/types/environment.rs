// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use serde::de::{Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Environment(String);

impl Default for Environment {
    fn default() -> Self {
        Environment("base".to_string())
    }
}

impl PartialEq<str> for Environment {
    fn eq(&self, other: &str) -> bool {
        self.0.as_str() == other
    }
}

impl PartialEq<String> for Environment {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl<'a> PartialEq<&'a str> for Environment {
    fn eq(&self, other: &&'a str) -> bool {
        self.0.as_str() == *other
    }
}

impl Environment {
    pub fn new(s: impl Into<String>) -> Self {
        Environment(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for Environment {
    fn from(s: String) -> Self {
        Environment(s)
    }
}

impl From<&str> for Environment {
    fn from(s: &str) -> Self {
        Environment(s.to_string())
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Environment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Environment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Environment(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_base() {
        let env = Environment::default();
        assert_eq!(env.as_str(), "base");
    }

    #[test]
    fn test_from_string() {
        let env: Environment = "production".to_string().into();
        assert_eq!(env.as_str(), "production");
    }

    #[test]
    fn test_from_str() {
        let env: Environment = "staging".into();
        assert_eq!(env.as_str(), "staging");
    }

    #[test]
    fn test_is_empty() {
        let empty = Environment("".to_string());
        let non_empty = Environment("test".to_string());
        assert!(empty.is_empty());
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_display() {
        let env = Environment::new("test");
        assert_eq!(format!("{}", env), "test");
    }

    #[test]
    fn test_serialization() {
        use serde_json;
        let env = Environment::new("production");
        let json = serde_json::to_string(&env).unwrap();
        assert_eq!(json, "\"production\"");
    }

    #[test]
    fn test_deserialization() {
        use serde_json;
        let env: Environment = serde_json::from_str("\"staging\"").unwrap();
        assert_eq!(env.as_str(), "staging");
    }
}
