// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use super::iterator::Iterator as FsIterator;
use crate::inventory::elements::Node;
use crate::storage::file_system::repository::FileFormat;
use crate::storage::file_system::{Error, YamlFsRepository};

pub struct NodesIterator<'a> {
    iterator: FsIterator,
    repository: &'a YamlFsRepository,
}

impl<'a> NodesIterator<'a> {
    pub fn new(repository: &'a YamlFsRepository) -> Self {
        let iterator = FsIterator::new(repository.nodes_uri());
        Self {
            iterator,
            repository,
        }
    }
}

impl<'a> Iterator for NodesIterator<'a> {
    type Item = Result<Node, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let result = self.iterator.next()?;
            let path = match result {
                Ok(path) => path,
                Err(error) => return Some(Err(error)),
            };

            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };

            match ext {
                "yml" | "yaml" => {
                    return Some(self.repository.load_node(&path, FileFormat::Yaml));
                }

                // We ignore all other files
                _ => {
                    tracing::debug!("ignoring file {}", path.to_string_lossy());
                    continue;
                }
            }
        }
    }
}
