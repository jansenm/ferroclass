// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use regex::Regex;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct MergeConfig {
    pub value_override_prefix: Option<String>,
    pub value_constant_prefix: Option<String>,
    pub feature_value_override: bool,
    pub feature_value_constant: bool,
    pub strict_constant_parameters: bool,
    pub automatic_parameters: bool,
    pub allow_none_override: bool,
    pub ignore_class_notfound: bool,
    pub ignore_class_notfound_regexp: Vec<String>,
    pub ignore_class_notfound_warning: bool,
    /// Compiled regex for class-not-found matching, wrapped in `Arc<Mutex<...>>`
    /// so that `MergeConfig` is `Send + Sync`.
    pub compiled_class_notfound_regexp: Arc<Mutex<Option<Regex>>>,
    pub group_errors: bool,
    pub ignore_overwritten_missing_references: bool,
    pub inventory_ignore_failed_node: bool,
    pub inventory_ignore_failed_render: bool,
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            value_override_prefix: Some("~".to_string()),
            value_constant_prefix: Some("=".to_string()),
            feature_value_override: true,
            feature_value_constant: true,
            strict_constant_parameters: true,
            automatic_parameters: true,
            allow_none_override: false,
            ignore_class_notfound: false,
            ignore_class_notfound_regexp: vec![".*".to_string()],
            ignore_class_notfound_warning: true,
            compiled_class_notfound_regexp: Arc::new(Mutex::new(None)),
            group_errors: true,
            ignore_overwritten_missing_references: true,
            inventory_ignore_failed_node: false,
            inventory_ignore_failed_render: false,
        }
    }
}

impl MergeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn value_override_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.value_override_prefix = Some(prefix.into());
        self
    }

    pub fn feature_value_override(mut self, strip: bool) -> Self {
        self.feature_value_override = strip;
        self
    }

    pub fn feature_value_constant(mut self, enabled: bool) -> Self {
        self.feature_value_constant = enabled;
        self
    }

    pub fn strict_constant_parameters(mut self, enabled: bool) -> Self {
        self.strict_constant_parameters = enabled;
        self
    }

    pub fn automatic_parameters(mut self, enabled: bool) -> Self {
        self.automatic_parameters = enabled;
        self
    }

    pub fn allow_none_override(mut self, enabled: bool) -> Self {
        self.allow_none_override = enabled;
        self
    }

    pub fn ignore_class_notfound(mut self, enabled: bool) -> Self {
        self.ignore_class_notfound = enabled;
        self
    }

    pub fn ignore_class_notfound_regexp(mut self, patterns: Vec<String>) -> Self {
        self.ignore_class_notfound_regexp = patterns;
        self
    }

    pub fn ignore_class_notfound_warning(mut self, enabled: bool) -> Self {
        self.ignore_class_notfound_warning = enabled;
        self
    }

    pub fn group_errors(mut self, enabled: bool) -> Self {
        self.group_errors = enabled;
        self
    }

    pub fn ignore_overwritten_missing_references(mut self, enabled: bool) -> Self {
        self.ignore_overwritten_missing_references = enabled;
        self
    }

    pub fn inventory_ignore_failed_node(mut self, enabled: bool) -> Self {
        self.inventory_ignore_failed_node = enabled;
        self
    }

    pub fn inventory_ignore_failed_render(mut self, enabled: bool) -> Self {
        self.inventory_ignore_failed_render = enabled;
        self
    }

    pub fn disabled() -> Self {
        Self {
            value_override_prefix: None,
            value_constant_prefix: None,
            feature_value_override: false,
            feature_value_constant: false,
            strict_constant_parameters: true,
            automatic_parameters: true,
            allow_none_override: false,
            ignore_class_notfound: false,
            ignore_class_notfound_regexp: vec![".*".to_string()],
            ignore_class_notfound_warning: true,
            compiled_class_notfound_regexp: Arc::new(Mutex::new(None)),
            group_errors: true,
            ignore_overwritten_missing_references: true,
            inventory_ignore_failed_node: false,
            inventory_ignore_failed_render: false,
        }
    }

    pub fn compile_regexps(&mut self) {
        if self.ignore_class_notfound && !self.ignore_class_notfound_regexp.is_empty() {
            let pattern = self.ignore_class_notfound_regexp.join("|");
            *self.compiled_class_notfound_regexp.lock().unwrap() = Regex::new(&pattern).ok();
        } else {
            *self.compiled_class_notfound_regexp.lock().unwrap() = None;
        }
    }

    pub fn should_ignore_class(&self, class_name: &str) -> bool {
        if !self.ignore_class_notfound {
            return false;
        }
        let guard = self.compiled_class_notfound_regexp.lock().unwrap();
        match guard.as_ref() {
            Some(re) => re.is_match(class_name),
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MergeConfig::default();
        assert_eq!(config.value_override_prefix, Some("~".to_string()));
        assert_eq!(config.value_constant_prefix, Some("=".to_string()));
        assert!(config.feature_value_override);
        assert!(config.feature_value_constant);
        assert!(config.strict_constant_parameters);
        assert!(config.automatic_parameters);
        assert!(!config.allow_none_override);
        assert!(!config.ignore_class_notfound);
        assert!(config.group_errors);
        assert!(config.ignore_overwritten_missing_references);
        assert!(!config.inventory_ignore_failed_node);
        assert!(!config.inventory_ignore_failed_render);
    }

    #[test]
    fn test_new_is_default() {
        let new_config = MergeConfig::new();
        let default_config = MergeConfig::default();
        assert_eq!(
            new_config.value_override_prefix,
            default_config.value_override_prefix
        );
        assert_eq!(
            new_config.feature_value_override,
            default_config.feature_value_override
        );
        assert_eq!(
            new_config.ignore_class_notfound,
            default_config.ignore_class_notfound
        );
        assert_eq!(new_config.group_errors, default_config.group_errors);
    }

    #[test]
    fn test_disabled() {
        let config = MergeConfig::disabled();
        assert!(config.value_override_prefix.is_none());
        assert!(config.value_constant_prefix.is_none());
        assert!(!config.feature_value_override);
        assert!(!config.feature_value_constant);
        assert!(config.strict_constant_parameters);
        assert!(config.automatic_parameters);
        assert!(!config.allow_none_override);
        assert!(!config.ignore_class_notfound);
        assert!(config.group_errors);
    }

    #[test]
    fn test_compile_regexps_with_ignore() {
        let mut config = MergeConfig::default()
            .ignore_class_notfound(true)
            .ignore_class_notfound_regexp(vec!["^foo".to_string(), "^bar".to_string()]);
        config.compile_regexps();
        let guard = config.compiled_class_notfound_regexp.lock().unwrap();
        let re = guard.as_ref().unwrap();
        assert!(re.is_match("foo-test"));
        assert!(!re.is_match("baz-test"));
    }

    #[test]
    fn test_compile_regexps_without_ignore() {
        let mut config =
            MergeConfig::default().ignore_class_notfound_regexp(vec!["^foo".to_string()]);
        config.compile_regexps();
        let guard = config.compiled_class_notfound_regexp.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn test_compile_regexps_empty_patterns() {
        let mut config = MergeConfig::default().ignore_class_notfound(true);
        config.ignore_class_notfound_regexp = vec![];
        config.compile_regexps();
        let guard = config.compiled_class_notfound_regexp.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn test_should_ignore_class_disabled() {
        let config = MergeConfig::default();
        assert!(!config.should_ignore_class("anything"));
    }

    #[test]
    fn test_should_ignore_class_enabled_with_regex() {
        let mut config = MergeConfig::default()
            .ignore_class_notfound(true)
            .ignore_class_notfound_regexp(vec!["^my\\.".to_string()]);
        config.compile_regexps();
        assert!(config.should_ignore_class("my.service"));
        assert!(!config.should_ignore_class("other.service"));
    }

    #[test]
    fn test_should_ignore_class_enabled_no_regex() {
        let mut config = MergeConfig::default().ignore_class_notfound(true);
        config.compile_regexps();
        assert!(config.should_ignore_class("anything"));
    }

    #[test]
    fn test_builder_chain() {
        let config = MergeConfig::new()
            .value_override_prefix("^^")
            .feature_value_override(false)
            .feature_value_constant(false)
            .strict_constant_parameters(false)
            .automatic_parameters(false)
            .allow_none_override(true)
            .ignore_class_notfound(true)
            .group_errors(false)
            .ignore_overwritten_missing_references(false)
            .inventory_ignore_failed_node(true)
            .inventory_ignore_failed_render(true);
        assert_eq!(config.value_override_prefix, Some("^^".to_string()));
        assert!(!config.feature_value_override);
        assert!(!config.feature_value_constant);
        assert!(!config.strict_constant_parameters);
        assert!(!config.automatic_parameters);
        assert!(config.allow_none_override);
        assert!(config.ignore_class_notfound);
        assert!(!config.group_errors);
        assert!(!config.ignore_overwritten_missing_references);
        assert!(config.inventory_ignore_failed_node);
        assert!(config.inventory_ignore_failed_render);
    }

    #[test]
    fn test_merge_config_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MergeConfig>();
    }
}
