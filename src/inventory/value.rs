// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use hashlink::LinkedHashMap;
use serde::de::{Deserialize, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use snafu::Snafu;
use std::fmt;
use std::rc::Rc;
use yaml_rust2::Yaml as YamlValue;

pub use crate::inventory::types::Environment;
pub type ClassList = Vec<String>;
pub use crate::inventory::applications::Applications;
pub type ApplicationList = Applications;
pub type Array = Vec<Value>;
pub type Hash = LinkedHashMap<Key, Value>;
pub type ParametersType = Hash;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("environment has to be a string, got {:?}", environment))]
    InvalidEnvironment { environment: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Null,
    Boolean(bool),
    Integer(i64),
    String(String),
}

impl Key {
    pub fn to_yaml_value(&self) -> YamlValue {
        match self {
            Key::String(s) => YamlValue::String(s.clone()),
            Key::Integer(i) => YamlValue::Integer(*i),
            Key::Boolean(b) => YamlValue::Boolean(*b),
            Key::Null => YamlValue::Null,
        }
    }
}

impl From<&Key> for YamlValue {
    fn from(key: &Key) -> Self {
        key.to_yaml_value()
    }
}

impl From<String> for Key {
    fn from(s: String) -> Self {
        Key::String(s)
    }
}

impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Key::String(s.to_string())
    }
}

impl From<i64> for Key {
    fn from(i: i64) -> Self {
        Key::Integer(i)
    }
}

impl From<bool> for Key {
    fn from(b: bool) -> Self {
        Key::Boolean(b)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::String(s) => write!(f, "{}", s),
            Key::Integer(i) => write!(f, "{}", i),
            Key::Boolean(b) => write!(f, "{}", b),
            Key::Null => write!(f, "null"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd)]
pub enum ReferencePart {
    Literal(String),
    Reference(Vec<ReferencePathSegment>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryPart {
    Literal(String),
    InvQuery(crate::inventory::inv_query::InvQueryData),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd)]
pub enum ReferencePathSegment {
    Literal(String),
    Inner(Vec<ReferencePathSegment>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Array(Rc<Array>),
    Boolean(bool),
    Hash(Rc<Hash>),
    Integer(i64),
    Null,
    Real(String),
    String(String),
    Reference(Vec<ReferencePathSegment>),
    StringWithReference(Vec<ReferencePart>),
    InvQuery(crate::inventory::inv_query::InvQueryData),
    StringWithInvQuery(Vec<QueryPart>),
    DeferredMerge(Rc<Vec<Value>>),
    OverrideMarker(Rc<Value>),
    ConstantMarker(Rc<Value>),
}

impl From<YamlValue> for Key {
    fn from(value: YamlValue) -> Self {
        match value {
            YamlValue::String(s) => Key::String(s),
            YamlValue::Integer(i) => Key::Integer(i),
            YamlValue::Boolean(b) => Key::Boolean(b),
            YamlValue::Null => Key::Null,
            YamlValue::Real(_)
            | YamlValue::Array(_)
            | YamlValue::Hash(_)
            | YamlValue::Alias(_)
            | YamlValue::BadValue => {
                panic!("cannot use {:?} as hash key", value)
            }
        }
    }
}

impl From<YamlValue> for Value {
    fn from(value: YamlValue) -> Self {
        match value {
            YamlValue::Array(value) => {
                Value::Array(Rc::new(value.into_iter().map(|v| v.into()).collect()))
            }
            YamlValue::Alias(_) => panic!("cannot convert alias to value"),
            YamlValue::BadValue => panic!("cannot convert bad value to value"),
            YamlValue::Boolean(value) => Value::Boolean(value),
            YamlValue::Hash(value) => Value::Hash(Rc::new(
                value
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            )),
            YamlValue::Integer(value) => Value::Integer(value),
            YamlValue::Null => Value::Null,
            YamlValue::Real(value) => Value::Real(value),
            YamlValue::String(value) => Value::String(value),
        }
    }
}

impl From<&Value> for YamlValue {
    fn from(value: &Value) -> Self {
        value.to_yaml_value()
    }
}

impl Value {
    pub fn to_yaml_value(&self) -> YamlValue {
        self.to_yaml_value_sorted(false)
    }

    pub fn to_yaml_value_sorted(&self, sorted: bool) -> YamlValue {
        match self {
            Value::Array(value) => YamlValue::Array(
                value
                    .iter()
                    .map(|v| v.to_yaml_value_sorted(sorted))
                    .collect(),
            ),
            Value::Boolean(value) => YamlValue::Boolean(*value),
            Value::Integer(value) => YamlValue::Integer(*value),
            Value::Hash(value) => {
                let entries: Vec<_> = value
                    .iter()
                    .map(|(k, v)| (k.to_yaml_value(), v.to_yaml_value_sorted(sorted)))
                    .collect();
                let mut result = LinkedHashMap::new();
                if sorted {
                    let mut sorted_entries = entries;
                    sorted_entries.sort_by(|a, b| yaml_key_cmp(&a.0, &b.0));
                    for (k, v) in sorted_entries {
                        result.insert(k, v);
                    }
                } else {
                    for (k, v) in entries {
                        result.insert(k, v);
                    }
                }
                YamlValue::Hash(result)
            }
            Value::Null => YamlValue::Null,
            Value::Real(value) => YamlValue::Real(value.clone()),
            Value::String(value) => YamlValue::String(value.clone()),
            Value::Reference(segments) => {
                YamlValue::String(format!("${{{}}}", format_segments(segments)))
            }
            Value::StringWithReference(parts) => {
                let s = parts
                    .iter()
                    .map(|p| match p {
                        ReferencePart::Literal(lit) => lit.clone(),
                        ReferencePart::Reference(segments) => {
                            format!("${{{}}}", format_segments(segments))
                        }
                    })
                    .collect::<String>();
                YamlValue::String(s)
            }
            Value::DeferredMerge(values) => YamlValue::Array(
                values
                    .iter()
                    .map(|v| v.to_yaml_value_sorted(sorted))
                    .collect(),
            ),
            Value::OverrideMarker(value) => value.to_yaml_value_sorted(sorted),
            Value::ConstantMarker(value) => value.to_yaml_value_sorted(sorted),
            Value::InvQuery(data) => YamlValue::String(format_inv_query(data)),
            Value::StringWithInvQuery(parts) => {
                let s = parts
                    .iter()
                    .map(|p| match p {
                        QueryPart::Literal(lit) => lit.clone(),
                        QueryPart::InvQuery(data) => format_inv_query(data),
                    })
                    .collect::<String>();
                YamlValue::String(s)
            }
        }
    }

    pub fn as_array(&self) -> Option<&Array> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    pub fn as_hash(&self) -> Option<&Hash> {
        match self {
            Value::Hash(h) => Some(h),
            _ => None,
        }
    }

    pub fn make_array_mut(this: &mut Value) -> &mut Array {
        match this {
            Value::Array(rc) => Rc::make_mut(rc),
            _ => panic!("called make_array_mut on non-Array value"),
        }
    }

    pub fn make_hash_mut(this: &mut Value) -> &mut Hash {
        match this {
            Value::Hash(rc) => Rc::make_mut(rc),
            _ => panic!("called make_hash_mut on non-Hash value"),
        }
    }

    pub fn detect_references(&mut self) {
        match self {
            Value::String(s) => {
                if s.contains("$[") {
                    *self = parse_string_inv_queries(s);
                } else {
                    *self = parse_string_references(s);
                }
            }
            Value::Array(arr) => {
                let arr = Rc::make_mut(arr);
                for item in arr.iter_mut() {
                    item.detect_references();
                }
            }
            Value::Hash(hash) => {
                let hash = Rc::make_mut(hash);
                for (_k, v) in hash.iter_mut() {
                    v.detect_references();
                }
            }
            Value::DeferredMerge(values) => {
                let values = Rc::make_mut(values);
                for item in values.iter_mut() {
                    item.detect_references();
                }
            }
            Value::OverrideMarker(value) => {
                Rc::make_mut(value).detect_references();
            }
            Value::ConstantMarker(value) => {
                Rc::make_mut(value).detect_references();
            }
            Value::InvQuery(_) | Value::StringWithInvQuery(_) => {}
            _ => {}
        }
    }

    pub fn has_references(&self) -> bool {
        match self {
            Value::Reference(_)
            | Value::StringWithReference(_)
            | Value::InvQuery(_)
            | Value::StringWithInvQuery(_)
            | Value::DeferredMerge(_) => true,
            Value::OverrideMarker(v) => v.has_references(),
            Value::ConstantMarker(v) => v.has_references(),
            Value::Array(arr) => arr.iter().any(|v| v.has_references()),
            Value::Hash(hash) => hash.values().any(|v| v.has_references()),
            _ => false,
        }
    }

    pub fn has_inv_query(&self) -> bool {
        match self {
            Value::InvQuery(_) | Value::StringWithInvQuery(_) => true,
            Value::OverrideMarker(v) => v.has_inv_query(),
            Value::ConstantMarker(v) => v.has_inv_query(),
            Value::Array(arr) => arr.iter().any(|v| v.has_inv_query()),
            Value::Hash(hash) => hash.values().any(|v| v.has_inv_query()),
            Value::DeferredMerge(values) => values.iter().any(|v| v.has_inv_query()),
            _ => false,
        }
    }

    pub fn ignore_failed_render(&self) -> bool {
        match self {
            Value::InvQuery(data) => data.ignore_failed_render,
            Value::StringWithInvQuery(parts) => {
                let mut found = false;
                for part in parts {
                    if let QueryPart::InvQuery(data) = part {
                        found = true;
                        if !data.ignore_failed_render {
                            return false;
                        }
                    }
                }
                found
            }
            Value::OverrideMarker(v) => v.ignore_failed_render(),
            Value::ConstantMarker(v) => v.ignore_failed_render(),
            Value::Array(arr) => {
                let mut found = false;
                for v in arr.iter() {
                    if v.has_inv_query() {
                        found = true;
                        if !v.ignore_failed_render() {
                            return false;
                        }
                    }
                }
                found
            }
            Value::Hash(hash) => {
                let mut found = false;
                for v in hash.values() {
                    if v.has_inv_query() {
                        found = true;
                        if !v.ignore_failed_render() {
                            return false;
                        }
                    }
                }
                found
            }
            Value::DeferredMerge(values) => {
                let mut found = false;
                for v in values.iter() {
                    if v.has_inv_query() {
                        found = true;
                        if !v.ignore_failed_render() {
                            return false;
                        }
                    }
                }
                found
            }
            _ => false,
        }
    }

    pub fn value_to_string(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Integer(i) => i.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Real(s) => s.clone(),
            Value::Null => String::new(),
            other => format!("{:?}", other),
        }
    }
}

pub fn contains_interpolation(s: &str) -> bool {
    s.contains("${")
}

pub fn contains_inv_query(s: &str) -> bool {
    s.contains("$[")
}

fn parse_string_references(s: &str) -> Value {
    let mut parts: Vec<ReferencePart> = Vec::new();
    let mut literal_buf = String::new();
    let mut i = 0;
    let mut pure_reference = false;

    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        let c_len = c.len_utf8();

        if c == '\\' && i + c_len < s.len() {
            let next = s[i + c_len..].chars().next().unwrap();
            let next_len = next.len_utf8();
            if next == '\\' {
                literal_buf.push('\\');
                i += c_len + next_len;
                continue;
            }
            if next == '$' {
                literal_buf.push('$');
                i += c_len + next_len;
                continue;
            }
            literal_buf.push('\\');
            literal_buf.push(next);
            i += c_len + next_len;
            continue;
        }
        if c == '$'
            && i + c_len < s.len()
            && s[i + c_len..].starts_with('{')
            && let Some(ref_end) = find_closing_brace(s, i + c_len + 1)
        {
            let inner = &s[i + c_len + 1..ref_end];
            let segments = parse_reference_path(inner);

            if literal_buf.is_empty() && i == 0 && ref_end + 1 == s.len() {
                pure_reference = true;
            }

            if !literal_buf.is_empty() {
                parts.push(ReferencePart::Literal(literal_buf.clone()));
                literal_buf.clear();
            }
            parts.push(ReferencePart::Reference(segments));

            i = ref_end + 1;
            continue;
        }
        literal_buf.push(c);
        i += c_len;
    }

    if pure_reference {
        let segments = match &parts[0] {
            ReferencePart::Reference(segments) => segments.clone(),
            _ => unreachable!(),
        };
        return Value::Reference(segments);
    }

    if !literal_buf.is_empty() {
        parts.push(ReferencePart::Literal(literal_buf));
    }

    if parts.is_empty() {
        return Value::String(s.to_string());
    }

    if parts.len() == 1
        && let ReferencePart::Literal(lit) = &parts[0]
    {
        return Value::String(lit.clone());
    }

    Value::StringWithReference(parts)
}

fn parse_string_inv_queries(s: &str) -> Value {
    let mut parts: Vec<QueryPart> = Vec::new();
    let mut literal_buf = String::new();
    let mut i = 0;
    let mut pure_inv_query = false;

    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        let c_len = c.len_utf8();

        if c == '\\' && i + c_len < s.len() {
            let next = s[i + c_len..].chars().next().unwrap();
            let next_len = next.len_utf8();
            if next == '\\' {
                literal_buf.push('\\');
                i += c_len + next_len;
                continue;
            }
            if next == '$' {
                literal_buf.push('$');
                i += c_len + next_len;
                continue;
            }
            literal_buf.push('\\');
            literal_buf.push(next);
            i += c_len + next_len;
            continue;
        }
        if c == '$'
            && i + c_len < s.len()
            && s[i + c_len..].starts_with('[')
            && let Some(bracket_end) = find_closing_bracket(s, i + c_len + 1)
        {
            let inner = &s[i + c_len + 1..bracket_end];

            match crate::inventory::inv_query::parse_inv_query(inner) {
                Ok(query_data) => {
                    if literal_buf.is_empty() && i == 0 && bracket_end + 1 == s.len() {
                        pure_inv_query = true;
                    }

                    if !literal_buf.is_empty() {
                        let parsed_literal = parse_string_references(&literal_buf);
                        push_literal_as_query_part(&parsed_literal, &mut parts);
                        literal_buf.clear();
                    }
                    parts.push(QueryPart::InvQuery(query_data));

                    i = bracket_end + 1;
                    continue;
                }
                Err(_) => {
                    literal_buf.push(c);
                    i += c_len;
                    continue;
                }
            }
        }
        literal_buf.push(c);
        i += c_len;
    }

    if pure_inv_query {
        let data = match &parts[0] {
            QueryPart::InvQuery(data) => data.clone(),
            _ => unreachable!(),
        };
        return Value::InvQuery(data);
    }

    if !literal_buf.is_empty() {
        let parsed_literal = parse_string_references(&literal_buf);
        push_literal_as_query_part(&parsed_literal, &mut parts);
    }

    if parts.is_empty() {
        return Value::String(s.to_string());
    }

    if parts.len() == 1
        && let QueryPart::Literal(lit) = &parts[0]
    {
        return Value::String(lit.clone());
    }

    Value::StringWithInvQuery(parts)
}

fn push_literal_as_query_part(value: &Value, parts: &mut Vec<QueryPart>) {
    match value {
        Value::String(s) => {
            if !s.is_empty() {
                parts.push(QueryPart::Literal(s.clone()));
            }
        }
        Value::Reference(segments) => {
            parts.push(QueryPart::Literal(format!(
                "${{{}}}",
                format_segments(segments)
            )));
        }
        Value::StringWithReference(ref_parts) => {
            let s = ref_parts
                .iter()
                .map(|p| match p {
                    ReferencePart::Literal(lit) => lit.clone(),
                    ReferencePart::Reference(segments) => {
                        format!("${{{}}}", format_segments(segments))
                    }
                })
                .collect::<String>();
            if !s.is_empty() {
                parts.push(QueryPart::Literal(s));
            }
        }
        other => {
            parts.push(QueryPart::Literal(other.value_to_string()));
        }
    }
}

fn find_closing_bracket(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut depth = 1;
    let mut i = start;
    while i < len {
        if bytes[i] == b'\\' && i + 1 < len && (bytes[i + 1] == b'\\' || bytes[i + 1] == b'$') {
            i += 2;
            continue;
        }
        if bytes[i] == b'[' {
            depth += 1;
        }
        if bytes[i] == b']' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

struct ParseState<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> ParseState<'a> {
    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.remaining().starts_with(prefix)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn next_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance_one_char(&mut self) {
        if let Some(c) = self.next_char() {
            self.pos += c.len_utf8();
        }
    }
}

fn parse_reference_path(input: &str) -> Vec<ReferencePathSegment> {
    let state = &mut ParseState { input, pos: 0 };
    let mut segments: Vec<ReferencePathSegment> = Vec::new();
    let mut literal_buf = String::new();

    while !state.at_end() {
        if state.next_char() == Some('\\') && state.remaining().len() > 1 {
            let next_char = state.remaining().chars().nth(1).unwrap();
            if next_char == '\\' {
                literal_buf.push('\\');
            } else if next_char == '$' {
                literal_buf.push('$');
            } else {
                literal_buf.push('\\');
                literal_buf.push(next_char);
            }
            state.advance(1 + next_char.len_utf8());
        } else if state.starts_with("${") {
            if !literal_buf.is_empty() {
                segments.push(ReferencePathSegment::Literal(literal_buf.clone()));
                literal_buf.clear();
            }
            state.advance(2);
            let inner = parse_inner_reference(state);
            segments.push(ReferencePathSegment::Inner(inner));
        } else if state.next_char() == Some(':') {
            if !literal_buf.is_empty() {
                segments.push(ReferencePathSegment::Literal(literal_buf.clone()));
                literal_buf.clear();
            }
            state.advance_one_char();
        } else {
            literal_buf.push(state.next_char().unwrap());
            state.advance_one_char();
        }
    }

    if !literal_buf.is_empty() {
        segments.push(ReferencePathSegment::Literal(literal_buf));
    }

    if segments.is_empty() {
        vec![ReferencePathSegment::Literal(String::new())]
    } else {
        segments
    }
}

fn parse_inner_reference(state: &mut ParseState) -> Vec<ReferencePathSegment> {
    let mut segments: Vec<ReferencePathSegment> = Vec::new();
    let mut literal_buf = String::new();

    while !state.at_end() {
        if state.next_char() == Some('\\') && state.remaining().len() > 1 {
            let next_char = state.remaining().chars().nth(1).unwrap();
            if next_char == '\\' {
                literal_buf.push('\\');
            } else if next_char == '$' {
                literal_buf.push('$');
            } else {
                literal_buf.push('\\');
                literal_buf.push(next_char);
            }
            state.advance(1 + next_char.len_utf8());
        } else if state.starts_with("${") {
            if !literal_buf.is_empty() {
                segments.push(ReferencePathSegment::Literal(literal_buf.clone()));
                literal_buf.clear();
            }
            state.advance(2);
            let inner = parse_inner_reference(state);
            segments.push(ReferencePathSegment::Inner(inner));
        } else if state.next_char() == Some(':') {
            if !literal_buf.is_empty() {
                segments.push(ReferencePathSegment::Literal(literal_buf.clone()));
                literal_buf.clear();
            }
            state.advance_one_char();
        } else if state.next_char() == Some('}') {
            if !literal_buf.is_empty() {
                segments.push(ReferencePathSegment::Literal(literal_buf));
            }
            state.advance_one_char();
            return if segments.is_empty() {
                vec![ReferencePathSegment::Literal(String::new())]
            } else {
                segments
            };
        } else {
            literal_buf.push(state.next_char().unwrap());
            state.advance_one_char();
        }
    }

    if segments.is_empty() {
        vec![ReferencePathSegment::Literal(String::new())]
    } else {
        segments
    }
}

pub fn format_segments(segments: &[ReferencePathSegment]) -> String {
    segments
        .iter()
        .map(|seg| match seg {
            ReferencePathSegment::Literal(s) => s.clone(),
            ReferencePathSegment::Inner(inner) => {
                format!("${{{}}}", format_segments(inner))
            }
        })
        .collect::<Vec<_>>()
        .join(":")
}

pub fn format_inv_query(data: &crate::inventory::inv_query::InvQueryData) -> String {
    use crate::inventory::inv_query::{ComparisonOp, LogicalOp};
    let mut s = String::from("$[");
    if data.needs_all_envs {
        s.push_str("+AllEnvs ");
    }
    if data.ignore_failed_render {
        s.push_str("+IgnoreErrors ");
    }
    if let Some(ref path) = data.value_path {
        s.push_str("exports:");
        s.push_str(&path.join(":"));
    }
    if let Some(ref cond) = data.condition {
        s.push_str(" if ");
        for (i, test) in cond.tests.iter().enumerate() {
            if i > 0 {
                s.push_str(match cond.operators[i - 1] {
                    LogicalOp::And => " and ",
                    LogicalOp::Or => " or ",
                });
            }
            s.push_str(&format_operand(&test.left));
            s.push_str(match test.operator {
                ComparisonOp::Equal => "==",
                ComparisonOp::NotEqual => "!=",
            });
            s.push_str(&format_operand(&test.right));
        }
    }
    s.push(']');
    s
}

fn format_operand(op: &crate::inventory::inv_query::Operand) -> String {
    use crate::inventory::inv_query::Operand;
    match op {
        Operand::ExportPath(path) => format!("exports:{}", path.join(":")),
        Operand::SelfPath(path) => format!("self:{}", path.join(":")),
        Operand::Literal(val) => match val {
            Value::Boolean(true) => "true".to_string(),
            Value::Boolean(false) => "false".to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Real(r) => r.clone(),
            Value::String(s) => s.clone(),
            other => other.value_to_string(),
        },
    }
}

fn find_closing_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut depth = 1;
    let mut i = start;
    while i < len {
        if bytes[i] == b'\\' && i + 1 < len && (bytes[i + 1] == b'\\' || bytes[i + 1] == b'$') {
            i += 2;
            continue;
        }
        if bytes[i] == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
            depth += 1;
            i += 2;
            continue;
        }
        if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

impl From<Array> for Value {
    fn from(array: Array) -> Value {
        Value::Array(Rc::new(array))
    }
}

impl From<bool> for Value {
    fn from(boolean: bool) -> Value {
        Value::Boolean(boolean)
    }
}

impl From<Hash> for Value {
    fn from(hash: Hash) -> Value {
        Value::Hash(Rc::new(hash))
    }
}

impl From<i64> for Value {
    fn from(integer: i64) -> Value {
        Value::Integer(integer)
    }
}

impl From<String> for Value {
    fn from(string: String) -> Value {
        Value::String(string)
    }
}

impl TryInto<Environment> for Value {
    type Error = Error;
    fn try_into(self) -> Result<Environment, Self::Error> {
        match self {
            Value::String(value) => Ok(Environment::from(value)),
            Value::Null => Ok(Environment::default()),
            _ => Err(Error::InvalidEnvironment {
                environment: format!("{:?}", self),
            }),
        }
    }
}

impl Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Key::String(s) => s.serialize(serializer),
            Key::Integer(i) => i.serialize(serializer),
            Key::Boolean(b) => b.serialize(serializer),
            Key::Null => serializer.serialize_unit(),
        }
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Key, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct KeyVisitor;

        impl<'de> Visitor<'de> for KeyVisitor {
            type Value = Key;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a scalar value usable as a map key")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Key, E> {
                Ok(Key::Boolean(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Key, E> {
                Ok(Key::Integer(value))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Key, E> {
                Ok(Key::Integer(value as i64))
            }

            fn visit_str<E>(self, value: &str) -> Result<Key, E> {
                Ok(Key::String(value.to_string()))
            }

            fn visit_string<E>(self, value: String) -> Result<Key, E> {
                Ok(Key::String(value))
            }

            fn visit_none<E>(self) -> Result<Key, E> {
                Ok(Key::Null)
            }
        }

        deserializer.deserialize_any(KeyVisitor)
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::Array(arr) => arr.serialize(serializer),
            Value::Boolean(b) => b.serialize(serializer),
            Value::Integer(i) => i.serialize(serializer),
            Value::Null => serializer.serialize_unit(),
            Value::Real(s) => s.serialize(serializer),
            Value::String(s) => s.serialize(serializer),
            Value::Hash(map) => {
                let mut map_ser = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map.iter() {
                    let key_str = match k {
                        Key::String(s) => s.clone(),
                        Key::Integer(i) => i.to_string(),
                        Key::Boolean(b) => b.to_string(),
                        Key::Null => "null".to_string(),
                    };
                    map_ser.serialize_entry(&key_str, v)?;
                }
                map_ser.end()
            }
            Value::Reference(segments) => {
                format!("${{{}}}", format_segments(segments)).serialize(serializer)
            }
            Value::StringWithReference(parts) => {
                let s = parts
                    .iter()
                    .map(|p| match p {
                        ReferencePart::Literal(lit) => lit.clone(),
                        ReferencePart::Reference(segments) => {
                            format!("${{{}}}", format_segments(segments))
                        }
                    })
                    .collect::<String>();
                s.serialize(serializer)
            }
            Value::DeferredMerge(values) => values.serialize(serializer),
            Value::OverrideMarker(value) => value.serialize(serializer),
            Value::ConstantMarker(value) => value.serialize(serializer),
            Value::InvQuery(data) => format_inv_query(data).serialize(serializer),
            Value::StringWithInvQuery(parts) => {
                let s = parts
                    .iter()
                    .map(|p| match p {
                        QueryPart::Literal(lit) => lit.clone(),
                        QueryPart::InvQuery(data) => format_inv_query(data),
                    })
                    .collect::<String>();
                s.serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a YAML value")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
                Ok(Value::Boolean(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
                Ok(Value::Integer(value))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
                Ok(Value::Integer(value as i64))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
                Ok(Value::Real(value.to_string()))
            }

            fn visit_str<E>(self, value: &str) -> Result<Value, E> {
                Ok(Value::String(value.to_string()))
            }

            fn visit_string<E>(self, value: String) -> Result<Value, E> {
                Ok(Value::String(value))
            }

            fn visit_none<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element()? {
                    values.push(value);
                }
                Ok(Value::Array(Rc::new(values)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut hash: Hash = LinkedHashMap::new();
                while let Some((key, value)) = map.next_entry()? {
                    hash.insert(key, value);
                }
                Ok(Value::Hash(Rc::new(hash)))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

fn yaml_key_cmp(a: &yaml_rust2::Yaml, b: &yaml_rust2::Yaml) -> std::cmp::Ordering {
    match (a, b) {
        (yaml_rust2::Yaml::String(sa), yaml_rust2::Yaml::String(sb)) => sa.cmp(sb),
        (yaml_rust2::Yaml::Integer(ia), yaml_rust2::Yaml::Integer(ib)) => ia.cmp(ib),
        (yaml_rust2::Yaml::Real(ra), yaml_rust2::Yaml::Real(rb)) => ra.cmp(rb),
        (yaml_rust2::Yaml::Boolean(ba), yaml_rust2::Yaml::Boolean(bb)) => ba.cmp(bb),
        _ => a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_references_simple_path() {
        let mut v = Value::String("${host:name}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::Reference(vec![
                ReferencePathSegment::Literal("host".to_string()),
                ReferencePathSegment::Literal("name".to_string()),
            ])
        );
    }

    #[test]
    fn test_detect_references_top_level_key() {
        let mut v = Value::String("${name}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::Reference(vec![ReferencePathSegment::Literal("name".to_string())])
        );
    }

    #[test]
    fn test_detect_references_string_with_ref() {
        let mut v = Value::String("Hello ${name}!".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::StringWithReference(vec![
                ReferencePart::Literal("Hello ".to_string()),
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("name".to_string())]),
                ReferencePart::Literal("!".to_string()),
            ])
        );
    }

    #[test]
    fn test_detect_references_nested_inner() {
        let mut v = Value::String("${beta:${alpha:two}}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::Reference(vec![
                ReferencePathSegment::Literal("beta".to_string()),
                ReferencePathSegment::Inner(vec![
                    ReferencePathSegment::Literal("alpha".to_string()),
                    ReferencePathSegment::Literal("two".to_string()),
                ]),
            ])
        );
    }

    #[test]
    fn test_detect_references_multiple_refs_in_string() {
        let mut v = Value::String("${a} and ${b}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::StringWithReference(vec![
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("a".to_string())]),
                ReferencePart::Literal(" and ".to_string()),
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("b".to_string())]),
            ])
        );
    }

    #[test]
    fn test_detect_references_no_reference() {
        let mut v = Value::String("plain text".to_string());
        v.detect_references();
        assert_eq!(v, Value::String("plain text".to_string()));
    }

    #[test]
    fn test_detect_references_in_hash() {
        let mut params = LinkedHashMap::new();
        params.insert(
            Key::String("motd".to_string()),
            Value::String("Welcome to ${host:name}".to_string()),
        );
        params.insert(
            Key::String("simple".to_string()),
            Value::String("no refs".to_string()),
        );
        let mut v = Value::Hash(Rc::new(params));
        v.detect_references();
        match &v {
            Value::Hash(h) => {
                assert!(matches!(
                    h.get(&Key::String("motd".to_string())),
                    Some(Value::StringWithReference(_))
                ));
                assert!(matches!(
                    h.get(&Key::String("simple".to_string())),
                    Some(Value::String(_))
                ));
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_detect_references_in_array() {
        let mut v = Value::Array(Rc::new(vec![
            Value::String("${x}".to_string()),
            Value::String("plain".to_string()),
        ]));
        v.detect_references();
        match &v {
            Value::Array(arr) => {
                assert!(matches!(&arr[0], Value::Reference(_)));
                assert!(matches!(&arr[1], Value::String(_)));
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_has_references() {
        assert!(
            Value::Reference(vec![ReferencePathSegment::Literal("x".to_string())]).has_references()
        );
        assert!(
            Value::StringWithReference(vec![
                ReferencePart::Literal("a".to_string()),
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("x".to_string())]),
            ])
            .has_references()
        );
        assert!(Value::DeferredMerge(Rc::new(vec![Value::Integer(1)])).has_references());
        assert!(!Value::String("hello".to_string()).has_references());
        assert!(!Value::Integer(42).has_references());
        assert!(
            Value::Array(Rc::new(vec![Value::Reference(vec![
                ReferencePathSegment::Literal("x".to_string()),
            ])]))
            .has_references()
        );
        assert!(
            Value::Hash(Rc::new({
                let mut h = LinkedHashMap::new();
                h.insert(
                    Key::String("k".to_string()),
                    Value::Reference(vec![ReferencePathSegment::Literal("x".to_string())]),
                );
                h
            }))
            .has_references()
        );
    }

    #[test]
    fn test_format_segments_simple() {
        let segments = vec![
            ReferencePathSegment::Literal("host".to_string()),
            ReferencePathSegment::Literal("name".to_string()),
        ];
        assert_eq!(format_segments(&segments), "host:name");
    }

    #[test]
    fn test_format_segments_with_inner() {
        let segments = vec![
            ReferencePathSegment::Literal("beta".to_string()),
            ReferencePathSegment::Inner(vec![
                ReferencePathSegment::Literal("alpha".to_string()),
                ReferencePathSegment::Literal("two".to_string()),
            ]),
        ];
        assert_eq!(format_segments(&segments), "beta:${alpha:two}");
    }

    #[test]
    fn test_to_yaml_value_reference() {
        let v = Value::Reference(vec![
            ReferencePathSegment::Literal("host".to_string()),
            ReferencePathSegment::Literal("name".to_string()),
        ]);
        assert_eq!(
            v.to_yaml_value(),
            YamlValue::String("${host:name}".to_string())
        );
    }

    #[test]
    fn test_to_yaml_value_nested_reference() {
        let v = Value::Reference(vec![
            ReferencePathSegment::Literal("beta".to_string()),
            ReferencePathSegment::Inner(vec![
                ReferencePathSegment::Literal("alpha".to_string()),
                ReferencePathSegment::Literal("two".to_string()),
            ]),
        ]);
        assert_eq!(
            v.to_yaml_value(),
            YamlValue::String("${beta:${alpha:two}}".to_string())
        );
    }

    #[test]
    fn test_to_yaml_value_string_with_reference() {
        let v = Value::StringWithReference(vec![
            ReferencePart::Literal("Hello ".to_string()),
            ReferencePart::Reference(vec![
                ReferencePathSegment::Literal("host".to_string()),
                ReferencePathSegment::Literal("name".to_string()),
            ]),
        ]);
        assert_eq!(
            v.to_yaml_value(),
            YamlValue::String("Hello ${host:name}".to_string())
        );
    }

    #[test]
    fn test_deferred_merge_yaml_value() {
        let v = Value::DeferredMerge(Rc::new(vec![Value::Integer(1), Value::Integer(2)]));
        match v.to_yaml_value() {
            YamlValue::Array(arr) => {
                assert_eq!(arr.len(), 2);
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_escape_dollar_prevents_reference() {
        let mut v = Value::String(r"The colour is \${colour}".to_string());
        v.detect_references();
        assert_eq!(v, Value::String("The colour is ${colour}".to_string()));
    }

    #[test]
    fn test_escape_backslash() {
        let mut v = Value::String(r"The colour is \\${colour}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::StringWithReference(vec![
                ReferencePart::Literal(r"The colour is \".to_string()),
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("colour".to_string())]),
            ])
        );
    }

    #[test]
    fn test_escape_backslash_only() {
        let mut v = Value::String(r"path: \\server\\share".to_string());
        v.detect_references();
        assert_eq!(v, Value::String("path: \\server\\share".to_string()));
    }

    #[test]
    fn test_escape_unknown_keeps_backslash() {
        let mut v = Value::String(r"\n is a newline".to_string());
        v.detect_references();
        assert_eq!(v, Value::String(r"\n is a newline".to_string()));
    }

    #[test]
    fn test_escape_dollar_inside_reference_prevents_nesting() {
        let mut v = Value::String(r"${beta:\${alpha:two}}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::StringWithReference(vec![
                ReferencePart::Reference(vec![
                    ReferencePathSegment::Literal("beta".to_string()),
                    ReferencePathSegment::Literal("${alpha".to_string()),
                    ReferencePathSegment::Literal("two".to_string()),
                ]),
                ReferencePart::Literal("}".to_string()),
            ])
        );
    }

    #[test]
    fn test_escape_spec_example_unescaped() {
        let mut v = Value::String("The colour is ${colour}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::StringWithReference(vec![
                ReferencePart::Literal("The colour is ".to_string()),
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("colour".to_string())]),
            ])
        );
    }

    #[test]
    fn test_escape_spec_example_escaped() {
        let mut v = Value::String(r"The colour is \${colour}".to_string());
        v.detect_references();
        assert_eq!(v, Value::String("The colour is ${colour}".to_string()));
    }

    #[test]
    fn test_escape_spec_example_double_escaped() {
        let mut v = Value::String(r"The colour is \\${colour}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::StringWithReference(vec![
                ReferencePart::Literal("The colour is \\".to_string()),
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("colour".to_string())]),
            ])
        );
    }

    #[test]
    fn test_ignore_failed_render_inv_query() {
        use crate::inventory::inv_query::{InvQueryData, QueryType};

        let data = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: true,
            query_type: QueryType::Value,
            value_path: Some(vec!["a".to_string()]),
            condition: None,
        };
        let v = Value::InvQuery(data);
        assert!(v.ignore_failed_render());
    }

    #[test]
    fn test_ignore_failed_render_inv_query_false() {
        use crate::inventory::inv_query::{InvQueryData, QueryType};

        let data = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: false,
            query_type: QueryType::Value,
            value_path: Some(vec!["a".to_string()]),
            condition: None,
        };
        let v = Value::InvQuery(data);
        assert!(!v.ignore_failed_render());
    }

    #[test]
    fn test_ignore_failed_render_string_with_inv_query() {
        use crate::inventory::inv_query::{InvQueryData, QueryType};

        let data = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: true,
            query_type: QueryType::Value,
            value_path: Some(vec!["a".to_string()]),
            condition: None,
        };
        let v = Value::StringWithInvQuery(vec![
            QueryPart::Literal("prefix_".to_string()),
            QueryPart::InvQuery(data),
        ]);
        assert!(v.ignore_failed_render());
    }

    #[test]
    fn test_ignore_failed_render_hash_all_true() {
        use crate::inventory::inv_query::{InvQueryData, QueryType};

        let data1 = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: true,
            query_type: QueryType::Value,
            value_path: Some(vec!["a".to_string()]),
            condition: None,
        };
        let data2 = InvQueryData {
            needs_all_envs: true,
            ignore_failed_render: true,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: None,
        };
        let mut hash: Hash = LinkedHashMap::new();
        hash.insert(Key::String("k1".to_string()), Value::InvQuery(data1));
        hash.insert(Key::String("k2".to_string()), Value::InvQuery(data2));
        let v = Value::Hash(Rc::new(hash));
        assert!(v.ignore_failed_render());
    }

    #[test]
    fn test_ignore_failed_render_hash_mixed() {
        use crate::inventory::inv_query::{InvQueryData, QueryType};

        let data1 = InvQueryData {
            needs_all_envs: false,
            ignore_failed_render: true,
            query_type: QueryType::Value,
            value_path: Some(vec!["a".to_string()]),
            condition: None,
        };
        let data2 = InvQueryData {
            needs_all_envs: true,
            ignore_failed_render: false,
            query_type: QueryType::ListTest,
            value_path: None,
            condition: None,
        };
        let mut hash: Hash = LinkedHashMap::new();
        hash.insert(Key::String("k1".to_string()), Value::InvQuery(data1));
        hash.insert(Key::String("k2".to_string()), Value::InvQuery(data2));
        let v = Value::Hash(Rc::new(hash));
        assert!(!v.ignore_failed_render());
    }

    #[test]
    fn test_ignore_failed_render_non_inv_query() {
        let v = Value::String("hello".to_string());
        assert!(!v.ignore_failed_render());
    }
}
