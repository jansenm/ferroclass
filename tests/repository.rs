// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::options::StorageOptionsTrait;
use ferroclass::inventory::options::{StorageOptions, StorageType};
use ferroclass::storage::file_system::YamlFsRepository;

#[test]
fn test() {
    let options = StorageOptions::default();

    let _config = match options.storage_type {
        StorageType::YamlFs => {
            YamlFsRepository::new(&options.yaml_fs_options, options.parameter_key_style())
                .expect("Failed to load config")
        }
        StorageType::YamlFile => panic!("YamlFile requires a file path, not a directory"),
    };

    // :todo: remove or turn into a test
    // dbg!(&config);
}
