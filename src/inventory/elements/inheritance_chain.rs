// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

pub fn resolve_relative_class_name(reference: &str, entity_name: &str) -> String {
    let leading_dots = reference.chars().take_while(|c| *c == '.').count();
    if leading_dots == 0 {
        return reference.to_string();
    }

    let parts: Vec<&str> = entity_name.split('.').collect();
    let available_levels = parts.len().saturating_sub(1);

    if leading_dots > available_levels {
        let stripped = reference.trim_start_matches('.');
        stripped.to_string()
    } else {
        let parent_levels = parts.len() - leading_dots;
        let parent: String = parts[..parent_levels].join(".");
        let remainder = &reference[leading_dots..];
        format!("{}.{}", parent, remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_relative_no_dots() {
        assert_eq!(resolve_relative_class_name("base", "one.alpha"), "base");
    }

    #[test]
    fn test_resolve_relative_one_dot() {
        assert_eq!(
            resolve_relative_class_name(".beta", "one.alpha"),
            "one.beta"
        );
    }

    #[test]
    fn test_resolve_relative_two_dots() {
        assert_eq!(resolve_relative_class_name("..four", "one.alpha"), "four");
    }

    #[test]
    fn test_resolve_relative_two_dots_three_parts() {
        assert_eq!(resolve_relative_class_name("..gamma", "a.b.c"), "a.gamma");
    }

    #[test]
    fn test_resolve_relative_three_dots_three_parts() {
        assert_eq!(resolve_relative_class_name("...bar", "a.b.c"), "bar");
    }

    #[test]
    fn test_resolve_relative_dots_exceed_depth() {
        assert_eq!(resolve_relative_class_name("...deep", "one.alpha"), "deep");
    }

    #[test]
    fn test_resolve_relative_dot_on_root() {
        assert_eq!(resolve_relative_class_name(".child", "root"), "child");
    }

    #[test]
    fn test_resolve_relative_multi_part_reference() {
        assert_eq!(
            resolve_relative_class_name("..two.gamma", "one.alpha"),
            "two.gamma"
        );
    }

    #[test]
    fn test_resolve_relative_one_dot_multi_part() {
        assert_eq!(
            resolve_relative_class_name(".sub.class", "one.alpha"),
            "one.sub.class"
        );
    }
}
