// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use super::iterator::Iterator;
use crate::inventory::elements::Class;
use crate::storage::file_system::repository::FileFormat;
use crate::storage::file_system::{Error, YamlFsRepository};

pub struct ClassesIterator<'a> {
    iterator: Iterator,
    repository: &'a YamlFsRepository,
}

impl<'a> ClassesIterator<'a> {
    pub fn new(repository: &'a YamlFsRepository) -> Self {
        let iterator = Iterator::new(repository.classes_uri());
        Self {
            iterator,
            repository,
        }
    }
}

impl<'a> std::iter::Iterator for ClassesIterator<'a> {
    type Item = Result<Class, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.iterator.next()?;
        let Ok(path) = result else {
            return Some(Err(result.err().unwrap()));
        };

        let suffix = path.extension();
        let Some(suffix) = suffix else {
            return self.next();
        };

        match suffix.to_str() {
            Some("yml") | Some("yaml") => match self.repository.load_class(&path, FileFormat::Yaml)
            {
                Ok(class) => Some(Ok(class)),
                Err(error) => Some(Err(error)),
            },

            // We ignore all other files
            _ => {
                tracing::debug!("ignoring file {}", path.to_string_lossy());
                self.next()
            }
        }
    }
}
