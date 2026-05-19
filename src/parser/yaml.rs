// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::value::Value;
use snafu::prelude::*;
use yaml_rust2::{ScanError, YamlLoader};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("[{}:{}] {}", source.marker().line(), source.marker().col(), source.info()))]
    InvalidYamlError { source: ScanError },
    #[snafu(display("multi-document YAML is not supported"))]
    MultiDocumentError {},
}

pub trait Parser {
    fn parse(&self, definition: &str) -> Result<Value, Error>;
}

#[derive(Debug)]
pub struct YamlParser {}

impl YamlParser {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for YamlParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser for YamlParser {
    fn parse(&self, definition: &str) -> Result<Value, Error> {
        match YamlLoader::load_from_str(definition) {
            Err(err) => Err(Error::InvalidYamlError { source: err }),
            Ok(documents) => match documents.len() {
                0 => Ok(Value::Null),
                1 => Ok(documents[0].clone().into()),
                _ => Err(Error::MultiDocumentError {}),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::value::{Key, Value};
    use indoc::indoc;
    use yaml_rust2::{Yaml, YamlEmitter};

    #[test]
    fn empty_document() {
        let p = YamlParser::new();
        let value = p.parse("---").unwrap();
        assert_eq!(value, Value::Null);

        let value = p.parse("").unwrap();
        assert_eq!(value, Value::Null);

        let value = p.parse(&String::from("null")).unwrap();
        assert_eq!(value, Value::Null);
    }

    #[test]
    fn top_level_list() {
        let p = YamlParser::new();
        let document = indoc! {"
            ---
            - 1
            - 2.0
            - 3.147.2
            "};

        let Value::Array(arr) = p.parse(&String::from(document)).unwrap() else {
            panic!("needs to match")
        };
        assert_eq!(
            arr.as_slice(),
            &[
                Value::Integer(1),
                Value::Real("2.0".into()),
                Value::String("3.147.2".into())
            ]
        );
    }

    #[test]
    fn top_level_hash() {
        let p = YamlParser::new();
        let document = indoc! {"
            ---
            a: 1
            b: 2
            c: 3
        "};
        let Value::Hash(hash) = p.parse(&String::from(document)).unwrap() else {
            panic!("needs to match")
        };
        assert_eq!(
            hash.iter().collect::<Vec<(&Key, &Value)>>(),
            [
                (&Key::String("a".into()), &Value::Integer(1)),
                (&Key::String("b".into()), &Value::Integer(2)),
                (&Key::String("c".into()), &Value::Integer(3)),
            ]
        );
    }

    #[test]
    fn top_level_scalar() {
        let p = YamlParser::new();
        let document = indoc! {r#"
        ---
        "hello"
    "#};
        let Value::String(string) = p.parse(&String::from(document)).unwrap() else {
            panic!("needs to match")
        };
        assert_eq!(string, "hello");
    }

    fn dump(value: &Value) -> String {
        let mut out = String::new();
        let mut emitter = YamlEmitter::new(&mut out);
        let yaml = Yaml::from(value);
        emitter.dump(&yaml).unwrap();
        out
    }

    #[test]
    fn dump_values() {
        let p = YamlParser::new();
        let document = indoc! {r#"
                ---
                a: 1
                b:     a
                c: 0_0.03.75
            "#};
        let hash = p.parse(&String::from(document)).unwrap();

        assert_eq!(
            dump(&hash),
            indoc! {r#"
                ---
                a: 1
                b: a
                c: 0_0.03.75"#
            }
        )
    }

    #[test]
    fn invalid_document() {
        let p = YamlParser::new();
        let document = indoc! {r#"
                ---
                a: 1
                - a
            "#};
        let err: Error = p.parse(&String::from(document)).err().unwrap();
        match err {
            Error::InvalidYamlError { .. } => assert_eq!(
                err.to_string(),
                "[3:2] while parsing a block mapping, did not find expected key"
            ),
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn multi_document_error() {
        let p = YamlParser::new();
        let document = indoc! {r#"
                ---
                a: 1
                ---
                b: 2
            "#};
        let err: Error = p.parse(&String::from(document)).err().unwrap();
        match err {
            Error::MultiDocumentError {} => (),
            _ => panic!("wrong error"),
        }
    }

    #[test]
    fn utf8_string() {
        let p = YamlParser::new();
        let document = "name: René Müller\n";
        let Value::Hash(hash) = p.parse(document).unwrap() else {
            panic!("expected hash");
        };
        let name = hash.get(&Key::String("name".into())).unwrap();
        assert_eq!(name, &Value::String("René Müller".into()));
    }
}
