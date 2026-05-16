// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::Node;
use ferroclass::inventory::load_from_yaml_string;
use ferroclass::inventory::options::ParameterKeyStyle;
use ferroclass::inventory::value::{Key, Value};
use indoc::indoc;
use std::rc::Rc;

fn key_order(a: &Key, b: &Key) -> std::cmp::Ordering {
    format!("{:?}", a).cmp(&format!("{:?}", b))
}

fn compare_values(path: String, expected: &Value, actual: &Value) -> Option<ValueMismatch> {
    match (expected, actual) {
        (Value::Array(exp), Value::Array(act)) => {
            if exp.len() != act.len() {
                return Some(ValueMismatch {
                    path,
                    expected: expected.clone(),
                    actual: actual.clone(),
                });
            }
            for (i, (e, a)) in exp.iter().zip(act.iter()).enumerate() {
                if let Some(mismatch) = compare_values(format!("{}[{}]", path, i), e, a) {
                    return Some(mismatch);
                }
            }
            None
        }
        (Value::Hash(exp), Value::Hash(act)) => {
            if exp.len() != act.len() {
                return Some(ValueMismatch {
                    path,
                    expected: expected.clone(),
                    actual: actual.clone(),
                });
            }
            let mut exp_keys: Vec<_> = exp.keys().collect();
            let mut act_keys: Vec<_> = act.keys().collect();
            exp_keys.sort_by(|a, b| key_order(a, b));
            act_keys.sort_by(|a, b| key_order(a, b));
            if exp_keys != act_keys {
                return Some(ValueMismatch {
                    path,
                    expected: expected.clone(),
                    actual: actual.clone(),
                });
            }
            for k in exp_keys {
                let e = exp.get(k).unwrap();
                let a = act.get(k).unwrap();
                if let Some(mismatch) = compare_values(format!("{}.{:?}", path, k), e, a) {
                    return Some(mismatch);
                }
            }
            None
        }
        _ if expected == actual => None,
        _ => Some(ValueMismatch {
            path,
            expected: expected.clone(),
            actual: actual.clone(),
        }),
    }
}

fn compare_nodes_by_sorted_params(
    expected: &Node,
    actual: &Node,
) -> Result<(), Vec<ValueMismatch>> {
    let expected_params = expected.parameters();
    let actual_params = actual.parameters();

    if expected_params.len() != actual_params.len() {
        return Err(vec![ValueMismatch {
            path: "parameters".to_string(),
            expected: Value::Hash(Rc::new(expected_params.clone())),
            actual: Value::Hash(Rc::new(actual_params.clone())),
        }]);
    }

    let mut expected_keys: Vec<_> = expected_params.keys().collect();
    let mut actual_keys: Vec<_> = actual_params.keys().collect();

    expected_keys.sort_by(|a, b| key_order(a, b));
    actual_keys.sort_by(|a, b| key_order(a, b));

    if expected_keys != actual_keys {
        return Err(vec![ValueMismatch {
            path: "parameters".to_string(),
            expected: Value::Hash(Rc::new(expected_params.clone())),
            actual: Value::Hash(Rc::new(actual_params.clone())),
        }]);
    }

    let mut mismatches = Vec::new();
    for key in expected_keys {
        let expected_value = expected_params.get(key).unwrap();
        let actual_value = actual_params.get(key).unwrap();
        if let Some(mismatch) = compare_values(format!("{:?}", key), expected_value, actual_value) {
            mismatches.push(mismatch);
        }
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

#[derive(Debug)]
struct ValueMismatch {
    path: String,
    expected: Value,
    actual: Value,
}

impl std::fmt::Display for ValueMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: content does not match", self.path)?;
        writeln!(f, "  left:  {:?}", self.expected)?;
        writeln!(f, "  right: {:?}", self.actual)
    }
}

#[test]
fn test_class_merge() {
    const TEST_INVENTORY: &str = indoc!(
        r#"---
           ---
           name: all
           parameters:
           ---
           name: base1
           classes:
               - all
           parameters:
               base1:
                   list:
                       - 1
                   hash: {}
               base2:
                   value: we will override this
           ---
           name: base2
           classes:
               - base1
           parameters:
               base1:
                   list:
                       - 2
               base2:
                   list:
                       - 1
                   hash: {}
           ---
           name: node
           type: node
           classes:
                   - base2
                   - base1
           parameters:
               node: "hello world"
           "#
    );

    const EXPECTED_INVENTORY: &str = indoc!(
        r#"---
           ---
             name: node
             type: node
             classes:
                - all
                - base1
                - base2
             environment: "base"
             parameters:
               _reclass_:
                 name:
                   full: node
                   short: node
                 environment: "base"
               base1:
                  list:
                    - 1
                    - 2
                  hash: {}
               base2:
                  value: we will override this
                  list:
                    - 1
                  hash: {}
               node: "hello world"
           "#
    );

    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
        .expect("failed to parse inventory");

    let expected_inventory = load_from_yaml_string(EXPECTED_INVENTORY, &ParameterKeyStyle::Ansible)
        .expect("failed to parse expected inventory");

    for expected_node in expected_inventory.nodes_iter() {
        let node_name = expected_node.name();
        let actual_node = inventory
            .merge_node(node_name)
            .unwrap_or_else(|_| panic!("Failed to merge node {}", node_name));

        assert_eq!(&expected_node.name(), &actual_node.name());
        assert_eq!(&expected_node.classes(), &actual_node.classes());
        assert_eq!(&expected_node.environment(), &actual_node.environment());

        if let Err(mismatches) = compare_nodes_by_sorted_params(expected_node, &actual_node) {
            let mut msg = format!("node {} parameters do not match:\n", node_name);
            for mismatch in &mismatches {
                msg.push_str(&format!("  {}\n", mismatch));
            }
            panic!("{}", msg);
        }
    }
}
