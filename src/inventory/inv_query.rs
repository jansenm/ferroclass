// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::types::Environment;
use crate::inventory::value::{Hash, Key, Value};
use hashlink::LinkedHashMap;
use snafu::Snafu;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct NodeInventory {
    pub items: Hash,
    pub environment: Environment,
}

pub type InventoryMap = LinkedHashMap<String, NodeInventory>;

#[derive(Debug, Clone, PartialEq)]
pub struct InvQueryData {
    pub needs_all_envs: bool,
    pub ignore_failed_render: bool,
    pub query_type: QueryType,
    pub value_path: Option<Vec<String>>,
    pub condition: Option<LogicTest>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryType {
    Value,
    Test,
    ListTest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogicTest {
    pub tests: Vec<EqualityTest>,
    pub operators: Vec<LogicalOp>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EqualityTest {
    pub left: Operand,
    pub operator: ComparisonOp,
    pub right: Operand,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonOp {
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    ExportPath(Vec<String>),
    SelfPath(Vec<String>),
    Literal(crate::inventory::value::Value),
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed to parse inventory query: {detail}"))]
    ParseError { detail: String },
}

pub fn parse_inv_query(input: &str) -> Result<InvQueryData, Error> {
    let tokens = tokenize(input)?;
    let mut pos = 0;

    let mut needs_all_envs = false;
    let mut ignore_failed_render = false;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Option(opt) => {
                match opt.to_lowercase().as_str() {
                    "+ignoreerrors" => ignore_failed_render = true,
                    "+allenvs" => needs_all_envs = true,
                    _ => {
                        return Err(Error::ParseError {
                            detail: format!("unknown option: {}", opt),
                        });
                    }
                }
                pos += 1;
            }
            _ => break,
        }
    }

    if pos >= tokens.len() {
        return Err(Error::ParseError {
            detail: "empty expression".to_string(),
        });
    }

    let (query_type, value_path, condition) = if matches!(&tokens[pos], Token::Keyword(k) if k.to_lowercase() == "if")
    {
        pos += 1;
        let condition = parse_test(&tokens, &mut pos)?;
        (QueryType::ListTest, None, Some(condition))
    } else {
        let path = parse_path(&tokens, &mut pos)?;

        if pos < tokens.len()
            && matches!(&tokens[pos], Token::Keyword(k) if k.to_lowercase() == "if")
        {
            pos += 1;
            let condition = parse_test(&tokens, &mut pos)?;
            (
                QueryType::Test,
                Some(strip_exports_prefix(&path)),
                Some(condition),
            )
        } else {
            (QueryType::Value, Some(strip_exports_prefix(&path)), None)
        }
    };

    if pos < tokens.len() {
        return Err(Error::ParseError {
            detail: format!("unexpected token after expression: {:?}", tokens[pos]),
        });
    }

    Ok(InvQueryData {
        needs_all_envs,
        ignore_failed_render,
        query_type,
        value_path,
        condition,
    })
}

impl InvQueryData {
    pub fn evaluate(
        &self,
        own_params: &Hash,
        inventory: &InventoryMap,
        own_environment: &Environment,
    ) -> Value {
        match self.query_type {
            QueryType::Value => self.evaluate_value(inventory, own_environment),
            QueryType::Test => self.evaluate_test(own_params, inventory, own_environment),
            QueryType::ListTest => self.evaluate_list_test(own_params, inventory, own_environment),
        }
    }

    fn evaluate_value(&self, inventory: &InventoryMap, own_environment: &Environment) -> Value {
        let path = match &self.value_path {
            Some(p) => p,
            None => return Value::Hash(Arc::new(LinkedHashMap::new())),
        };

        let mut results: LinkedHashMap<Key, Value> = LinkedHashMap::new();
        for (name, node_inv) in inventory {
            if !self.env_matches(own_environment, &node_inv.environment) {
                continue;
            }
            if let Some(val) = lookup_export_path(&node_inv.items, path) {
                results.insert(Key::String(name.clone()), val.clone());
            }
        }
        Value::Hash(Arc::new(results))
    }

    fn evaluate_test(
        &self,
        own_params: &Hash,
        inventory: &InventoryMap,
        own_environment: &Environment,
    ) -> Value {
        let path = match &self.value_path {
            Some(p) => p,
            None => return Value::Hash(Arc::new(LinkedHashMap::new())),
        };

        let condition = match &self.condition {
            Some(c) => c,
            None => return Value::Hash(Arc::new(LinkedHashMap::new())),
        };

        let mut results: LinkedHashMap<Key, Value> = LinkedHashMap::new();
        for (name, node_inv) in inventory {
            if !self.env_matches(own_environment, &node_inv.environment) {
                continue;
            }
            if evaluate_logic_test(condition, own_params, &node_inv.items)
                && let Some(val) = lookup_export_path(&node_inv.items, path)
            {
                results.insert(Key::String(name.clone()), val.clone());
            }
        }
        Value::Hash(Arc::new(results))
    }

    fn evaluate_list_test(
        &self,
        own_params: &Hash,
        inventory: &InventoryMap,
        own_environment: &Environment,
    ) -> Value {
        let condition = match &self.condition {
            Some(c) => c,
            None => return Value::Array(Arc::new(Vec::new())),
        };

        let mut results: Vec<String> = Vec::new();
        for (name, node_inv) in inventory {
            if !self.env_matches(own_environment, &node_inv.environment) {
                continue;
            }
            if evaluate_logic_test(condition, own_params, &node_inv.items) {
                results.push(name.clone());
            }
        }
        results.sort();
        Value::Array(Arc::new(results.into_iter().map(Value::String).collect()))
    }

    fn env_matches(&self, own_environment: &Environment, node_env: &Environment) -> bool {
        if self.needs_all_envs {
            return true;
        }
        own_environment == node_env
    }
}

fn evaluate_logic_test(test: &LogicTest, own_params: &Hash, exports: &Hash) -> bool {
    if test.tests.is_empty() {
        return true;
    }
    let mut result = evaluate_equality_test(&test.tests[0], own_params, exports);
    for (i, op) in test.operators.iter().enumerate() {
        let next = evaluate_equality_test(&test.tests[i + 1], own_params, exports);
        result = match op {
            LogicalOp::And => result && next,
            LogicalOp::Or => result || next,
        };
    }
    result
}

fn evaluate_equality_test(test: &EqualityTest, own_params: &Hash, exports: &Hash) -> bool {
    let left_val = resolve_operand(&test.left, own_params, exports);
    let right_val = resolve_operand(&test.right, own_params, exports);

    let left_val = match left_val {
        Some(v) => v,
        None => return false,
    };
    let right_val = match right_val {
        Some(v) => v,
        None => return false,
    };

    match test.operator {
        ComparisonOp::Equal => values_equal(&left_val, &right_val),
        ComparisonOp::NotEqual => !values_equal(&left_val, &right_val),
    }
}

fn resolve_operand(operand: &Operand, own_params: &Hash, exports: &Hash) -> Option<Value> {
    match operand {
        Operand::ExportPath(path) => lookup_export_path(exports, path).cloned(),
        Operand::SelfPath(path) => lookup_export_path(own_params, path).cloned(),
        Operand::Literal(val) => Some(val.clone()),
    }
}

fn lookup_export_path<'a>(hash: &'a Hash, path: &[String]) -> Option<&'a Value> {
    if path.is_empty() {
        return None;
    }
    let first = Key::String(path[0].clone());
    let mut current = hash.get(&first)?;
    for key_str in &path[1..] {
        match current {
            Value::Hash(h) => {
                current = h.get(&Key::String(key_str.clone()))?;
            }
            _ => return None,
        }
    }
    Some(current)
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(sa), Value::String(sb)) => sa == sb,
        (Value::Integer(ia), Value::Integer(ib)) => ia == ib,
        (Value::Boolean(ba), Value::Boolean(bb)) => ba == bb,
        (Value::Real(ra), Value::Real(rb)) => ra == rb,
        (Value::Null, Value::Null) => true,
        (Value::String(sa), Value::Integer(ib)) => sa == &ib.to_string(),
        (Value::Integer(ia), Value::String(sb)) => sb == &ia.to_string(),
        _ => false,
    }
}

fn strip_exports_prefix(path: &[String]) -> Vec<String> {
    if !path.is_empty() && path[0].to_lowercase() == "exports" {
        path[1..].to_vec()
    } else {
        path.to_vec()
    }
}

fn parse_path(tokens: &[Token], pos: &mut usize) -> Result<Vec<String>, Error> {
    if *pos >= tokens.len() {
        return Err(Error::ParseError {
            detail: "expected path, got end of expression".to_string(),
        });
    }

    match &tokens[*pos] {
        Token::Path(parts) => {
            let result = parts.clone();
            *pos += 1;
            Ok(result)
        }
        other => Err(Error::ParseError {
            detail: format!("expected path, got {:?}", other),
        }),
    }
}

fn parse_test(tokens: &[Token], pos: &mut usize) -> Result<LogicTest, Error> {
    let first_test = parse_equality_test(tokens, pos)?;
    let mut tests = vec![first_test];
    let mut operators = Vec::new();

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Keyword(k) if k.to_lowercase() == "and" => {
                operators.push(LogicalOp::And);
                *pos += 1;
                tests.push(parse_equality_test(tokens, pos)?);
            }
            Token::Keyword(k) if k.to_lowercase() == "or" => {
                operators.push(LogicalOp::Or);
                *pos += 1;
                tests.push(parse_equality_test(tokens, pos)?);
            }
            _ => break,
        }
    }

    Ok(LogicTest { tests, operators })
}

fn parse_equality_test(tokens: &[Token], pos: &mut usize) -> Result<EqualityTest, Error> {
    let left = parse_operand(tokens, pos)?;

    if *pos >= tokens.len() {
        return Err(Error::ParseError {
            detail: "expected comparison operator (== or !=), got end of expression".to_string(),
        });
    }

    let operator = match &tokens[*pos] {
        Token::Operator(op) => match op.as_str() {
            "==" => ComparisonOp::Equal,
            "!=" => ComparisonOp::NotEqual,
            _ => {
                return Err(Error::ParseError {
                    detail: format!("unknown comparison operator: {}", op),
                });
            }
        },
        other => {
            return Err(Error::ParseError {
                detail: format!("expected comparison operator, got {:?}", other),
            });
        }
    };
    *pos += 1;

    let right = parse_operand(tokens, pos)?;

    Ok(EqualityTest {
        left,
        operator,
        right,
    })
}

fn parse_operand(tokens: &[Token], pos: &mut usize) -> Result<Operand, Error> {
    if *pos >= tokens.len() {
        return Err(Error::ParseError {
            detail: "expected operand, got end of expression".to_string(),
        });
    }

    match &tokens[*pos] {
        Token::Path(parts) => {
            let path = parts.clone();
            *pos += 1;
            if path[0].to_lowercase() == "exports" {
                Ok(Operand::ExportPath(path[1..].to_vec()))
            } else if path[0].to_lowercase() == "self" {
                Ok(Operand::SelfPath(path[1..].to_vec()))
            } else if path[0].to_lowercase() == "true" {
                Ok(Operand::Literal(crate::inventory::value::Value::Boolean(
                    true,
                )))
            } else if path[0].to_lowercase() == "false" {
                Ok(Operand::Literal(crate::inventory::value::Value::Boolean(
                    false,
                )))
            } else {
                Ok(Operand::Literal(crate::inventory::value::Value::String(
                    parts.join(":"),
                )))
            }
        }
        Token::Integer(i) => {
            let val = *i;
            *pos += 1;
            Ok(Operand::Literal(crate::inventory::value::Value::Integer(
                val,
            )))
        }
        Token::Real(r) => {
            let r = r.clone();
            *pos += 1;
            Ok(Operand::Literal(crate::inventory::value::Value::Real(r)))
        }
        other => Err(Error::ParseError {
            detail: format!("expected operand, got {:?}", other),
        }),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Path(Vec<String>),
    Operator(String),
    Keyword(String),
    Option(String),
    Integer(i64),
    Real(String),
}

fn tokenize(input: &str) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        if c == '+' {
            let _start = i;
            chars.next();
            let mut opt = String::from("+");
            while let Some(&(_, cc)) = chars.peek() {
                if cc.is_whitespace() || cc == ']' {
                    break;
                }
                opt.push(cc);
                chars.next();
            }
            tokens.push(Token::Option(opt));
            continue;
        }

        if c == '=' {
            chars.next();
            if let Some(&(_, nc)) = chars.peek()
                && nc == '='
            {
                chars.next();
                tokens.push(Token::Operator("==".to_string()));
                continue;
            }
            return Err(Error::ParseError {
                detail: format!("unexpected character '=' at position {} (expected '==')", i),
            });
        }

        if c == '!' {
            chars.next();
            if let Some(&(_, nc)) = chars.peek()
                && nc == '='
            {
                chars.next();
                tokens.push(Token::Operator("!=".to_string()));
                continue;
            }
            return Err(Error::ParseError {
                detail: format!("unexpected character '!' at position {}", i),
            });
        }

        if c.is_ascii_digit()
            || (c == '-' && {
                chars.next();
                let is_neg_num = chars
                    .peek()
                    .map(|&(_, nc)| nc.is_ascii_digit())
                    .unwrap_or(false);
                if !is_neg_num {
                    return Err(Error::ParseError {
                        detail: format!("unexpected character '-' at position {}", i),
                    });
                }
                true
            })
        {
            let mut num = String::new();
            if c == '-' {
                num.push('-');
            } else {
                num.push(c);
            }
            if c != '-' {
                chars.next();
            }
            let mut is_real = false;
            while let Some(&(_, cc)) = chars.peek() {
                if cc.is_ascii_digit() {
                    num.push(cc);
                    chars.next();
                } else if cc == '.' && !is_real {
                    is_real = true;
                    num.push(cc);
                    chars.next();
                } else {
                    break;
                }
            }
            if is_real {
                tokens.push(Token::Real(num));
            } else {
                let val: i64 = num.parse().map_err(|e| Error::ParseError {
                    detail: format!("invalid integer '{}': {}", num, e),
                })?;
                tokens.push(Token::Integer(val));
            }
            continue;
        }

        let mut word = String::new();
        let _start = i;
        while let Some(&(_, cc)) = chars.peek() {
            if cc.is_whitespace() || cc == ']' || cc == '=' || cc == '!' {
                break;
            }
            word.push(cc);
            chars.next();
        }

        if word.is_empty() {
            return Err(Error::ParseError {
                detail: format!("unexpected character at position {}: {:?}", i, c),
            });
        }

        let lower = word.to_lowercase();
        if lower == "if" || lower == "and" || lower == "or" {
            tokens.push(Token::Keyword(word));
        } else if lower == "true" || lower == "false" {
            tokens.push(Token::Path(vec![word]));
        } else if word.contains(':') || lower.starts_with("exports") || lower.starts_with("self") {
            let parts: Vec<String> = word.split(':').map(String::from).collect();
            tokens.push(Token::Path(parts));
        } else {
            tokens.push(Token::Path(vec![word]));
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_value_query() {
        let q = parse_inv_query("exports:a").unwrap();
        assert_eq!(q.query_type, QueryType::Value);
        assert_eq!(q.value_path, Some(vec!["a".to_string()]));
        assert!(q.condition.is_none());
        assert!(!q.needs_all_envs);
        assert!(!q.ignore_failed_render);
    }

    #[test]
    fn test_parse_value_query_nested() {
        let q = parse_inv_query("exports:host:ip_address").unwrap();
        assert_eq!(q.query_type, QueryType::Value);
        assert_eq!(
            q.value_path,
            Some(vec!["host".to_string(), "ip_address".to_string()])
        );
    }

    #[test]
    fn test_parse_test_query() {
        let q = parse_inv_query("exports:a if exports:b == 2").unwrap();
        assert_eq!(q.query_type, QueryType::Test);
        assert_eq!(q.value_path, Some(vec!["a".to_string()]));
        let cond = q.condition.unwrap();
        assert_eq!(cond.tests.len(), 1);
        assert_eq!(cond.operators.len(), 0);
        assert_eq!(
            cond.tests[0].left,
            Operand::ExportPath(vec!["b".to_string()])
        );
        assert_eq!(cond.tests[0].operator, ComparisonOp::Equal);
        assert_eq!(
            cond.tests[0].right,
            Operand::Literal(crate::inventory::value::Value::Integer(2))
        );
    }

    #[test]
    fn test_parse_test_query_self_ref() {
        let q = parse_inv_query("exports:a if exports:b == self:test_value").unwrap();
        assert_eq!(q.query_type, QueryType::Test);
        let cond = q.condition.unwrap();
        assert_eq!(
            cond.tests[0].right,
            Operand::SelfPath(vec!["test_value".to_string()])
        );
    }

    #[test]
    fn test_parse_list_test_query() {
        let q = parse_inv_query("if exports:b == 0").unwrap();
        assert_eq!(q.query_type, QueryType::ListTest);
        assert!(q.value_path.is_none());
        let cond = q.condition.unwrap();
        assert_eq!(cond.tests[0].operator, ComparisonOp::Equal);
    }

    #[test]
    fn test_parse_options() {
        let q = parse_inv_query("+AllEnvs +IgnoreErrors exports:a").unwrap();
        assert!(q.needs_all_envs);
        assert!(q.ignore_failed_render);
        assert_eq!(q.query_type, QueryType::Value);
    }

    #[test]
    fn test_parse_and_condition() {
        let q = parse_inv_query("exports:a if exports:b == 0 and exports:c == green").unwrap();
        let cond = q.condition.unwrap();
        assert_eq!(cond.tests.len(), 2);
        assert_eq!(cond.operators.len(), 1);
        assert_eq!(cond.operators[0], LogicalOp::And);
    }

    #[test]
    fn test_parse_or_condition() {
        let q = parse_inv_query("exports:a if exports:b == 0 or exports:c == green").unwrap();
        let cond = q.condition.unwrap();
        assert_eq!(cond.tests.len(), 2);
        assert_eq!(cond.operators[0], LogicalOp::Or);
    }

    #[test]
    fn test_parse_not_equal() {
        let q = parse_inv_query("if exports:b != 0").unwrap();
        let cond = q.condition.unwrap();
        assert_eq!(cond.tests[0].operator, ComparisonOp::NotEqual);
    }

    #[test]
    fn test_parse_true_false_literals() {
        let q = parse_inv_query("if exports:flag == true").unwrap();
        let cond = q.condition.unwrap();
        assert_eq!(
            cond.tests[0].right,
            Operand::Literal(crate::inventory::value::Value::Boolean(true))
        );
    }

    #[test]
    fn test_parse_real_number() {
        let q = parse_inv_query("exports:a if exports:b == 3.14").unwrap();
        let cond = q.condition.unwrap();
        assert_eq!(
            cond.tests[0].right,
            Operand::Literal(crate::inventory::value::Value::Real("3.14".to_string()))
        );
    }

    #[test]
    fn test_parse_negative_integer() {
        let q = parse_inv_query("exports:a if exports:b == -1").unwrap();
        let cond = q.condition.unwrap();
        assert_eq!(
            cond.tests[0].right,
            Operand::Literal(crate::inventory::value::Value::Integer(-1))
        );
    }

    #[test]
    fn test_parse_case_insensitive_keywords() {
        let q = parse_inv_query("IF exports:b == 0 AND exports:c == green").unwrap();
        let cond = q.condition.unwrap();
        assert_eq!(cond.tests.len(), 2);
        assert_eq!(cond.operators[0], LogicalOp::And);
    }

    #[test]
    fn test_parse_case_insensitive_options() {
        let q = parse_inv_query("+allenvs +ignoreerrors exports:a").unwrap();
        assert!(q.needs_all_envs);
        assert!(q.ignore_failed_render);
    }

    #[test]
    fn test_parse_missing_if() {
        let result = parse_inv_query("exports:a exports:b == 2");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_operator() {
        let result = parse_inv_query("exports:a if exports:b self:test_value");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_right_operand() {
        let result = parse_inv_query("exports:a if exports:b == ");
        // Tokenizer should produce no token after ==
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_cluster_example() {
        let q = parse_inv_query("exports:host:ip_address if exports:cluster == self:cluster_name")
            .unwrap();
        assert_eq!(q.query_type, QueryType::Test);
        assert_eq!(
            q.value_path,
            Some(vec!["host".to_string(), "ip_address".to_string()])
        );
        let cond = q.condition.unwrap();
        assert_eq!(
            cond.tests[0].left,
            Operand::ExportPath(vec!["cluster".to_string()])
        );
        assert_eq!(
            cond.tests[0].right,
            Operand::SelfPath(vec!["cluster_name".to_string()])
        );
    }

    #[test]
    fn test_parse_combined_options() {
        let q = parse_inv_query("+AllEnvs +IgnoreErrors exports:test").unwrap();
        assert!(q.needs_all_envs);
        assert!(q.ignore_failed_render);
        assert_eq!(q.query_type, QueryType::Value);
        assert_eq!(q.value_path, Some(vec!["test".to_string()]));
    }

    fn make_hash(items: Vec<(&str, Value)>) -> Hash {
        let mut hash: Hash = LinkedHashMap::new();
        for (k, v) in items {
            hash.insert(Key::String(k.to_string()), v);
        }
        hash
    }

    fn make_inventory(nodes: Vec<(&str, Hash, &str)>) -> InventoryMap {
        let mut inventory = InventoryMap::new();
        for (name, exports, env) in nodes {
            inventory.insert(
                name.to_string(),
                NodeInventory {
                    items: exports,
                    environment: Environment::from(env.to_string()),
                },
            );
        }
        inventory
    }

    #[test]
    fn test_evaluate_value_query() {
        let node1_exports = make_hash(vec![("host", Value::String("10.0.0.1".to_string()))]);
        let node2_exports = make_hash(vec![("host", Value::String("10.0.0.2".to_string()))]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::Value,
            value_path: Some(vec!["host".to_string()]),
            condition: None,
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Hash(h) => {
                assert_eq!(h.len(), 2);
                assert_eq!(
                    h.get(&Key::String("node1".to_string())),
                    Some(&Value::String("10.0.0.1".to_string()))
                );
                assert_eq!(
                    h.get(&Key::String("node2".to_string())),
                    Some(&Value::String("10.0.0.2".to_string()))
                );
            }
            _ => panic!("Expected Hash, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_value_query_missing_path() {
        let node1_exports = make_hash(vec![("host", Value::String("10.0.0.1".to_string()))]);
        let node2_exports = make_hash(vec![("other", Value::Integer(42))]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::Value,
            value_path: Some(vec!["host".to_string()]),
            condition: None,
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Hash(h) => {
                assert_eq!(h.len(), 1);
                assert_eq!(
                    h.get(&Key::String("node1".to_string())),
                    Some(&Value::String("10.0.0.1".to_string()))
                );
            }
            _ => panic!("Expected Hash, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_test_query() {
        let node1_exports = make_hash(vec![
            ("cluster", Value::String("web".to_string())),
            ("ip", Value::String("10.0.0.1".to_string())),
        ]);
        let node2_exports = make_hash(vec![
            ("cluster", Value::String("db".to_string())),
            ("ip", Value::String("10.0.0.2".to_string())),
        ]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::Test,
            value_path: Some(vec!["ip".to_string()]),
            condition: Some(LogicTest {
                tests: vec![EqualityTest {
                    left: Operand::ExportPath(vec!["cluster".to_string()]),
                    operator: ComparisonOp::Equal,
                    right: Operand::SelfPath(vec!["my_cluster".to_string()]),
                }],
                operators: vec![],
            }),
        };

        let own_params = make_hash(vec![("my_cluster", Value::String("web".to_string()))]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Hash(h) => {
                assert_eq!(h.len(), 1);
                assert_eq!(
                    h.get(&Key::String("node1".to_string())),
                    Some(&Value::String("10.0.0.1".to_string()))
                );
            }
            _ => panic!("Expected Hash, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_list_test_query() {
        let node1_exports = make_hash(vec![("cluster", Value::String("web".to_string()))]);
        let node2_exports = make_hash(vec![("cluster", Value::String("db".to_string()))]);
        let node3_exports = make_hash(vec![("cluster", Value::String("web".to_string()))]);
        let inventory = make_inventory(vec![
            ("node3", node3_exports, "production"),
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: Some(LogicTest {
                tests: vec![EqualityTest {
                    left: Operand::ExportPath(vec!["cluster".to_string()]),
                    operator: ComparisonOp::Equal,
                    right: Operand::SelfPath(vec!["my_cluster".to_string()]),
                }],
                operators: vec![],
            }),
        };

        let own_params = make_hash(vec![("my_cluster", Value::String("web".to_string()))]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[0], Value::String("node1".to_string()));
                assert_eq!(arr[1], Value::String("node3".to_string()));
            }
            _ => panic!("Expected Array, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_environment_filtering() {
        let prod_exports = make_hash(vec![("host", Value::String("10.0.0.1".to_string()))]);
        let staging_exports = make_hash(vec![("host", Value::String("10.1.0.1".to_string()))]);
        let inventory = make_inventory(vec![
            ("prod_node", prod_exports, "production"),
            ("staging_node", staging_exports, "staging"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::Value,
            value_path: Some(vec!["host".to_string()]),
            condition: None,
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Hash(h) => {
                assert_eq!(h.len(), 1);
                assert_eq!(
                    h.get(&Key::String("prod_node".to_string())),
                    Some(&Value::String("10.0.0.1".to_string()))
                );
            }
            _ => panic!("Expected Hash, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_all_envs_option() {
        let prod_exports = make_hash(vec![("host", Value::String("10.0.0.1".to_string()))]);
        let staging_exports = make_hash(vec![("host", Value::String("10.1.0.1".to_string()))]);
        let inventory = make_inventory(vec![
            ("prod_node", prod_exports, "production"),
            ("staging_node", staging_exports, "staging"),
        ]);

        let query = InvQueryData {
            needs_all_envs: true,
            ignore_failed_render: false,
            query_type: QueryType::Value,
            value_path: Some(vec!["host".to_string()]),
            condition: None,
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Hash(h) => {
                assert_eq!(h.len(), 2);
            }
            _ => panic!("Expected Hash, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_and_condition() {
        let node1_exports = make_hash(vec![
            ("cluster", Value::String("web".to_string())),
            ("role", Value::String("primary".to_string())),
            ("ip", Value::String("10.0.0.1".to_string())),
        ]);
        let node2_exports = make_hash(vec![
            ("cluster", Value::String("web".to_string())),
            ("role", Value::String("secondary".to_string())),
            ("ip", Value::String("10.0.0.2".to_string())),
        ]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: Some(LogicTest {
                tests: vec![
                    EqualityTest {
                        left: Operand::ExportPath(vec!["cluster".to_string()]),
                        operator: ComparisonOp::Equal,
                        right: Operand::SelfPath(vec!["my_cluster".to_string()]),
                    },
                    EqualityTest {
                        left: Operand::ExportPath(vec!["role".to_string()]),
                        operator: ComparisonOp::Equal,
                        right: Operand::Literal(Value::String("primary".to_string())),
                    },
                ],
                operators: vec![LogicalOp::And],
            }),
        };

        let own_params = make_hash(vec![("my_cluster", Value::String("web".to_string()))]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0], Value::String("node1".to_string()));
            }
            _ => panic!("Expected Array, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_or_condition() {
        let node1_exports = make_hash(vec![("role", Value::String("web".to_string()))]);
        let node2_exports = make_hash(vec![("role", Value::String("db".to_string()))]);
        let node3_exports = make_hash(vec![("role", Value::String("cache".to_string()))]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
            ("node3", node3_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: Some(LogicTest {
                tests: vec![
                    EqualityTest {
                        left: Operand::ExportPath(vec!["role".to_string()]),
                        operator: ComparisonOp::Equal,
                        right: Operand::Literal(Value::String("web".to_string())),
                    },
                    EqualityTest {
                        left: Operand::ExportPath(vec!["role".to_string()]),
                        operator: ComparisonOp::Equal,
                        right: Operand::Literal(Value::String("db".to_string())),
                    },
                ],
                operators: vec![LogicalOp::Or],
            }),
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[0], Value::String("node1".to_string()));
                assert_eq!(arr[1], Value::String("node2".to_string()));
            }
            _ => panic!("Expected Array, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_not_equal() {
        let node1_exports = make_hash(vec![("role", Value::String("web".to_string()))]);
        let node2_exports = make_hash(vec![("role", Value::String("db".to_string()))]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: Some(LogicTest {
                tests: vec![EqualityTest {
                    left: Operand::ExportPath(vec!["role".to_string()]),
                    operator: ComparisonOp::NotEqual,
                    right: Operand::Literal(Value::String("web".to_string())),
                }],
                operators: vec![],
            }),
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0], Value::String("node2".to_string()));
            }
            _ => panic!("Expected Array, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_integer_comparison() {
        let node1_exports = make_hash(vec![("port", Value::Integer(80))]);
        let node2_exports = make_hash(vec![("port", Value::Integer(443))]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: Some(LogicTest {
                tests: vec![EqualityTest {
                    left: Operand::ExportPath(vec!["port".to_string()]),
                    operator: ComparisonOp::Equal,
                    right: Operand::Literal(Value::Integer(443)),
                }],
                operators: vec![],
            }),
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0], Value::String("node2".to_string()));
            }
            _ => panic!("Expected Array, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_missing_export_returns_false() {
        let node1_exports = make_hash(vec![("cluster", Value::String("web".to_string()))]);
        let node2_exports = make_hash(vec![("other_key", Value::String("db".to_string()))]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: Some(LogicTest {
                tests: vec![EqualityTest {
                    left: Operand::ExportPath(vec!["cluster".to_string()]),
                    operator: ComparisonOp::Equal,
                    right: Operand::Literal(Value::String("web".to_string())),
                }],
                operators: vec![],
            }),
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0], Value::String("node1".to_string()));
            }
            _ => panic!("Expected Array, got {:?}", result),
        }
    }

    #[test]
    fn test_evaluate_nested_path() {
        let node1_exports = make_hash(vec![(
            "host",
            Value::Hash(Arc::new(make_hash(vec![(
                "ip_address",
                Value::String("10.0.0.1".to_string()),
            )]))),
        )]);
        let node2_exports = make_hash(vec![(
            "host",
            Value::Hash(Arc::new(make_hash(vec![(
                "ip_address",
                Value::String("10.0.0.2".to_string()),
            )]))),
        )]);
        let inventory = make_inventory(vec![
            ("node1", node1_exports, "production"),
            ("node2", node2_exports, "production"),
        ]);

        let query = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::Value,
            value_path: Some(vec!["host".to_string(), "ip_address".to_string()]),
            condition: None,
        };

        let own_params = make_hash(vec![]);
        let own_env = Environment::from("production");
        let result = query.evaluate(&own_params, &inventory, &own_env);

        match result {
            Value::Hash(h) => {
                assert_eq!(h.len(), 2);
                assert_eq!(
                    h.get(&Key::String("node1".to_string())),
                    Some(&Value::String("10.0.0.1".to_string()))
                );
                assert_eq!(
                    h.get(&Key::String("node2".to_string())),
                    Some(&Value::String("10.0.0.2".to_string()))
                );
            }
            _ => panic!("Expected Hash, got {:?}", result),
        }
    }
}
