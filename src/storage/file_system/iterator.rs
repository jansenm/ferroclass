// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::storage::file_system::Error;
use std::path::{Path, PathBuf};

pub struct Iterator {
    iterator: walkdir::IntoIter,
}

impl Iterator {
    pub fn new(path: &Path) -> Self {
        let iterator = walkdir::WalkDir::new(path).into_iter();
        Self { iterator }
    }
}

impl std::iter::Iterator for Iterator {
    type Item = Result<PathBuf, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.iterator.next()?;
        if let Err(error) = result {
            let path = error
                .path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            return Some(Err(Error::Io {
                source: error.into_io_error().unwrap(),
                path,
            }));
        };
        let entry = result.unwrap();
        if entry.file_type().is_dir() {
            return self.next();
        };

        Some(Ok(entry.into_path()))
    }
}
