# Ferroclass

![CI](https://github.com/jansenm/ferroclass/actions/workflows/ci.yml/badge.svg)

Ferroclass is a lightweight configuration management database (CMDB) implementation. It is a
reimplementation of [reclass](https://reclass.pantsfullofunix.net/) in Rust, with full CLI
compatibility for the `reclass`, `reclass-ansible`, and `reclass-salt` commands.

The installed binaries are `ferroclass`, `ferroclass-ansible`, and `ferroclass-salt`,
allowing coexistence with the Python reclass package on the same system.

Common use cases include replacing the built-in inventory of Ansible, acting as an external node
classifier for Puppet, or managing configuration for any system that needs hierarchical data with
inheritance and interpolation.

## Installation

### From Source

```shell
cargo build --release
make install                 # Installs to /usr/local by default
make install DESTDIR=/tmp/pkg  # For packaging
```

Binaries: `ferroclass`, `ferroclass-ansible`, `ferroclass-salt`
Man pages: `man ferroclass`, `man ferroclass-ansible`, `man ferroclass-salt`

### RPM Packages

```shell
make dist                    # Create source + vendor tarballs
make packaging               # Build RPM packages
```

### Open Build Service (OBS)

OBS builds binary RPM packages for multiple distributions. The project is
configured for openSUSE Tumbleweed, Rocky Linux 9, and Rocky Linux 10 (x86_64
and aarch64).

```shell
make osc-sync                # Sync spec/changes/_service to OBS checkout
make osc-build               # Build for openSUSE Tumbleweed (default)
make osc-build-rocky9        # Build for Rocky Linux 9
make osc-build-rocky10       # Build for Rocky Linux 10
```

The `OBS_PROJECT` variable is auto-detected from your `~/.config/osc/oscrc`.
Override it or other variables as needed:

```shell
make osc-build OBS_PROJECT=home:mjansen1972:ferroclass
```

See `make -C packaging/obs help` for all OBS targets and variables.

## Releases

Ferroclass uses a hybrid release strategy: source tarballs and checksums are
published on GitHub Releases, while binary RPM packages are built and distributed
through the Open Build Service.

### Release Artifacts

| Artifact                                        | Location         | Purpose                        |
|-------------------------------------------------|------------------|--------------------------------|
| `ferroclass-X.Y.Z.tar.gz`                      | GitHub Releases  | Source tarball                  |
| `ferroclass-X.Y.Z-vendor.tar.gz`                | GitHub Releases  | Vendored Rust dependencies      |
| `ferroclass-X.Y.Z.tar.gz.sha256`                 | GitHub Releases  | SHA256 checksum                  |
| `ferroclass-X.Y.Z-vendor.tar.gz.sha256`          | GitHub Releases  | SHA256 checksum                  |
| `ferroclass-X.Y.Z.tar.gz.asc`                    | GitHub Releases  | GPG signature (when available)   |
| `ferroclass-X.Y.Z-vendor.tar.gz.asc`             | GitHub Releases  | GPG signature (when available)   |
| Binary RPMs for Tumbleweed, Rocky 9, Rocky 10   | OBS repositories | Distro package installation     |

### Release Process

```shell
# 1. Bump version
make bump-version VERSION_NEW=X.Y.Z

# 2. Update CHANGELOG.md manually

# 3. Run quality gates and create release
make release

# 4. Sync to OBS and build
make osc-sync
cd ~/obs/home:mjansen1972:ferroclass/ferroclass && osc commit
make osc-build-rocky9
make osc-build
```

The `release` target runs: `commit` → `dist` → `checksums` → `tag` → `release-gh`
→ `osc-sync`. It creates a GitHub Release with source tarballs and SHA256
checksums, and syncs packaging files to the OBS checkout.

### GPG Signing

To add GPG signatures to release tarballs:

```shell
make sign GPG_KEY=<key-id>
gh release upload vX.Y.Z packaging/rpm/ferroclass-X.Y.Z.tar.gz.asc \
    packaging/rpm/ferroclass-X.Y.Z-vendor.tar.gz.asc
```

### Individual Make Targets

| Target           | Purpose                                                    |
|------------------|------------------------------------------------------------|
| `bump-version`   | Update version in spec file and Cargo.toml (VERSION_NEW=) |
| `dist`           | Create source and vendor tarballs                           |
| `checksums`      | Generate SHA256 checksums for tarballs                       |
| `sign`            | Sign tarballs with GPG (GPG_KEY=)                           |
| `tag`            | Create and push git tag                                     |
| `release-gh`     | Create GitHub Release with artifacts and changelog          |
| `release`         | Full release pipeline                                       |

## Quick Start

Create a minimal inventory:

```shell
mkdir -p inventory/classes inventory/nodes
```

```yaml
# inventory/classes/base.yml
parameters:
    timezone: UTC
    ntp:
        server: pool.ntp.org
```

```yaml
# inventory/classes/web.yml
classes:
    - base
parameters:
    web:
        port: 8080
```

```yaml
# inventory/nodes/web.yml
classes:
    - web
parameters:
    hostname: web-prod-01
```

Run it:

```shell
ferroclass --nodeinfo web --inventory-base-uri ./inventory
ferroclass --inventory --output json --inventory-base-uri ./inventory
ferroclass-ansible --list --inventory-base-uri ./inventory
ferroclass-salt --top --inventory-base-uri ./inventory
```

For ready-to-use minimal examples, see the [`inventories/example/`](inventories/example)
and [`inventories/example_file/`](inventories/example_file) directories in the source
tree. The former uses the directory-based storage format; the latter uses the single-file
format. Both contain the same logical data. A full-featured showcase inventory with
advanced features (interpolation, exports, inventory queries, etc.) is planned for a
future release.

## Concepts

### Node

A node is a concrete item. It represents all the concrete items you need to act upon. For example, a
host that should be deployed, an account on a host, or a piece of software you want to build.

### Class

A class is an abstract concept that you apply to nodes by inheritance. Similar concepts include
Role, Category, Marker, or Trait.

### Repository

A repository is one unit of configuration containing classes and nodes. It is a directory with two
subdirectories:

```shell
$ ls inventory/
classes/
nodes/
```

Optionally, a `reclass-config.yml` file in the repository root (or in the current
working directory) provides default settings.

### Inheritance

Nodes and classes can inherit from classes. The configuration of the child is merged
into the configuration of the base class following a clear set of rules leading to
reproducible and predictable results.

### Inheritance Chain

Ferroclass supports multiple inheritances. The inheritance chain is the resulting order
in which objects are merged, left to right.

### Interpolation

After the inheritance chain is determined and configurations are merged, interpolation
resolves cross-references to avoid duplication.

```yaml
parameters:
    host:
        name: myserver
        ip-address: 127.0.0.1
    motd: |-
        Welcome to ${host:name} ${host:ip-address}
```

After interpolation, the value of `motd` is `Welcome to myserver 127.0.0.1`.

### Class Name Interpolation

Class names in the `classes:` list can contain `${...}` references that are resolved
during the merge step, using the parameters accumulated from previously processed
ancestor classes as the resolution context.

```yaml
# class env_setup
parameters:
    environment: staging
```

```yaml
# class staging.prod
parameters:
    role: production
```

```yaml
# node test_node
classes:
    - env_setup
    - "${environment}.prod"
```

When processing `test_node`:

1. `env_setup` is processed first, setting `environment: staging`.
2. `${environment}.prod` resolves to `staging.prod`, which is looked up and merged.
3. `staging.prod` contributes `role: production`.

**Key behaviors:**

- Class name interpolation happens inline during the inheritance chain walk, before
  parameter interpolation. The resolved class feeds back into the accumulator.
- Only parameters from **previously-processed** classes are available. A class cannot
  reference parameters from itself or later classes in the list.
- Relative class names (`.foo`, `..bar`) are resolved **before** interpolation.
- Non-string parameter values are coerced to strings: `${num}` where `num: 42`
  resolves to `"42"`.
- If a reference cannot be resolved, an error is raised.

## Rules

### Naming

The name of an object is derived from its filesystem path.

**For classes**, the path under the classes directory becomes the name with all slashes
substituted with a dot.

| Path                                        | Name                      |
|---------------------------------------------|---------------------------|
| $REPO/classes/distribution/opensuse.yml     | distribution.opensuse     |
| $REPO/classes/domain/michael-jansen.biz.yml | domain.michael-jansen.biz |

The rule stems from reclass. I personally don't like it because, as the second
example shows, you can't infer the path from the resulting name.

**For nodes**, the filename becomes the name. Subdirectories under nodes are discarded.

| Path                                    | Name               |
|-----------------------------------------|--------------------|
| $REPO/nodes/host/michael-jansen.biz.yml | michael-jansen.biz |

The namespaces of nodes and classes are distinct. It is possible to have a node and
class with the same name.

### Inheritance Chain

The inheritance chain is determined according to the following rules:

- The classes are merged depth-first in the order they appear in the file.
- A class is ignored if it is encountered a second time.
- The inheritance chain of a class is inserted in front of the class itself.
- A recursive inheritance chain is a non-recoverable error.

Example:

```yaml
# class baseA
classes:
```

```yaml
# class baseB
classes:
    - baseA
```

```yaml
# node nodeA
classes:
    - baseB
    - baseA
```

Even if **nodeA** inherits *baseA* after *baseB*, the effective inheritance chain is
*baseA*, *baseB*, and then *nodeA* because *baseB* inherits *baseA*, effectively moving
*baseA* in front of itself.

### Merging Values

#### Lists are appended

```yaml
# class baseA
parameters:
    list:
        - A
```

```yaml
# class baseB
classes:
    - baseA
parameters:
    list:
        - B
```

```yaml
# node nodeA
classes:
    - baseB
    - baseA
parameters:
    list:
        - C
```

Result:

```yaml
parameters:
    list:
        - A
        - B
        - C
```

#### Maps are merged

```yaml
# class baseA
parameters:
    map:
        a: 1
```

```yaml
# class baseB
classes:
    - baseA
parameters:
    map:
        b: 2
```

```yaml
# node nodeA
classes:
    - baseB
    - baseA
parameters:
    map:
        c: 3
```

Result:

```yaml
parameters:
    map:
        b: 2
        a: 1
        c: 3
```

Ferroclass preserves insertion order for maps. While YAML itself makes no guarantees
about map key order, this implementation uses ordered collections internally, so the
output order matches the merge order.

#### Values with different data types overwrite

```yaml
# class baseA
parameters:
    map:
        a: 1
```

```yaml
# node nodeA
classes:
    - baseA
parameters:
    map: "A map"
```

Result:

```yaml
parameters:
    map: "A map"
```

#### Lists and maps can be overwritten

```yaml
# class baseA
parameters:
    list:
        - A
```

```yaml
# node nodeA
classes:
    - baseA
parameters:
    ~list:
        - C
```

Result:

```yaml
parameters:
    list:
        - C
```

A tilde (`~`) in front of a key tells Ferroclass to replace the existing value entirely
instead of merging.

The override prefix can be used with any value type:

| Syntax              | Effect                                       |
|---------------------|----------------------------------------------|
| `~key: {new: true}` | Replace dict entirely (no deep merge)        |
| `~key: []`          | Replace list entirely (no append)            |
| `~key: 443`         | Replace scalar value                         |
| `~key: null`        | Set to null (requires `allow_none_override`) |
| `~key: {}`          | Reset dict to empty                          |

The tilde override is independent of the `allow_none_override` setting. `~key` always
triggers override semantics. `allow_none_override` only controls whether `key: null`
(without a tilde) overwrites a dict or list instead of raising an error.

#### Values can be marked constant

```yaml
# class baseA
parameters:
    port: 80
```

```yaml
# class baseB
classes:
    - baseA
parameters:
    =port: 443
```

```yaml
# class baseC
classes:
    - baseB
parameters:
    port: 9090
```

An equal sign (`=`) in front of a key marks the value as constant. Any later class
attempting to change the parameter will either raise an error (strict mode, default)
or be silently ignored (non-strict mode).

In the example above, `baseC` tries to set `port: 9090` but `baseB` already locked it
to `443`. The final value is `443`.

Use constant parameters sparingly. They can be a sign that your configuration is
structured in a way that fights the inheritance model.

### Merging Elements

Merging two classes produces a class; merging a class and a node produces a node.

#### Classes

The classes in the result are the classes of the parent plus the name of the parent.

#### Environment

The rule is: the first one wins:

- child's environment
- parent's environment
- none

#### Parameters

The parameters are merged following the rules described in [Merging Values](#merging-values).

#### Exports

Exports allow nodes to publish values that other nodes can query using inventory queries
(`$[...]` syntax). See [Process](#process) for how exports and inventory queries work.

#### Applications

The applications are configured as a list. The child's applications are appended to
the parent's.

## Process

Ferroclass processes a node request in six steps:

1. **Configuration** — CLI arguments and an optional `reclass-config.yml` file are merged.
   Config file search order: current directory, `$HOME/.config/reclass`, `/etc/reclass`.
   CLI arguments take precedence.

2. **Discovery** — The configured directories are walked recursively to find all YAML
   class and node files (`.yml` or `.yaml` extensions).

3. **Parsing** — Each file is parsed into its constituent parts: classes, environment,
   parameters, applications, and exports. Reference patterns (`${...}`) and inventory
   query expressions (`$[...]`) are detected and preserved for later resolution.

4. **Inheritance chain resolution & merging** — For a given node, the inheritance chain
   is built and merged in a single pass using a depth-first traversal. Class mappings
   (glob/regex patterns) are applied to auto-include classes, relative class names
   (`.foo`, `..bar`) and class name interpolation (`${var}`) are resolved. Each class is
   merged into an accumulator as it is encountered, with the node merged last. When a
   reference value collides with another value and the type cannot be determined yet,
   the merge is deferred.

5. **Interpolation** — References are resolved by looking up parameter paths. Deferred
   merges are collapsed. For inventory queries, a two-pass rendering is used: all nodes
   are first merged and interpolated to build an inventory map, then nodes with queries
   are re-interpolated using that map. Circular references are detected and reported.

6. **Output** — The merged and interpolated results are serialized to YAML or JSON.

For a detailed description of each step, see [docs/process.md](docs/process.md).

## Configuration

Ferroclass reads configuration from an optional `reclass-config.yml` file. The file is
searched in this order:

1. Current working directory
2. `$HOME/.config/reclass`
3. `/etc/reclass`

CLI arguments take precedence over the config file.

### Key Options

| Option                 | CLI Flag                  | Config Key              | Default    |
|------------------------|---------------------------|-------------------------|------------|
| Inventory base URI     | `--inventory-base-uri`    | `inventory_base_uri`    | (required) |
| Nodes URI              | `--nodes-uri`             | `nodes_uri`             | `nodes`    |
| Classes URI            | `--classes-uri`           | `classes_uri`           | `classes`  |
| Output format          | `--output` (yaml/json)    | `output`                | `yaml`     |
| Pretty-print           | `--pretty-print`          | (always enabled)        | on         |
| Node info              | `--nodeinfo`              | —                       | —          |
| Inventory              | `--inventory`             | —                       | —          |
| Environment            | `--environment`           | `default_environment`   | `base`     |
| Compose node name      | `--compose-node-name`     | `compose_node_name`     | off        |
| Ignore class not found | `--ignore-class-notfound` | `ignore_class_notfound` | off        |
| Group errors           | `--group-errors`          | `group_errors`          | off        |

See the man pages for full reference:

```shell
man ferroclass
man ferroclass-ansible
man ferroclass-salt
```

## Salt Integration

Ferroclass can serve as a Salt `ext_pillar` and `master_tops` data source
through its Python bindings (PyO3 native extension).

### Prerequisites

Install the Python bindings:

```shell
pip install ferroclass
```

Or on RPM-based systems:

```shell
zypper install python3-ferroclass
```

### Install Adapter Modules

Salt discovers plugins by scanning its `extension_modules` directory (default:
`/var/cache/salt/master/extmods`). The ferroclass adapter modules must be
placed there — Salt does not load them from the Python package path.

If you installed the `ferroclass-salt-adapter` RPM package, the adapter files
are in `/usr/share/ferroclass/contrib/`. Copy or symlink them:

```shell
# Create the Salt plugin directories if they don't exist
mkdir -p /var/cache/salt/master/extmods/pillar
mkdir -p /var/cache/salt/master/extmods/tops

# Symlink (preferred — stays in sync with package updates)
ln -s /usr/share/ferroclass/contrib/pillar/ferroclass_adapter.py \
      /var/cache/salt/master/extmods/pillar/
ln -s /usr/share/ferroclass/contrib/tops/ferroclass_adapter.py \
      /var/cache/salt/master/extmods/tops/

# Or copy (works if extension_modules is on a different partition)
cp /usr/share/ferroclass/contrib/pillar/ferroclass_adapter.py \
   /var/cache/salt/master/extmods/pillar/
cp /usr/share/ferroclass/contrib/tops/ferroclass_adapter.py \
   /var/cache/salt/master/extmods/tops/
```

If you installed via pip, the adapter files are in the `contrib/` directory
of the source tree. Download them from
[GitHub](https://github.com/jansenm/ferroclass/tree/main/contrib) or find
them in your local source checkout.

### Configure the Salt Master

Add ferroclass to `/etc/salt/master`:

```yaml
ferroclass: &ferroclass
  storage_type: yaml_fs
  inventory_base_uri: /srv/salt

ext_pillar:
  - ferroclass: *ferroclass

master_tops:
  ferroclass: *ferroclass
```

Note the plugin name is `ferroclass` (not `reclass`). If you are migrating
from the Python reclass adapter, update the Salt master config from
`reclass:` to `ferroclass:`.

Restart the Salt master after configuration changes:

```shell
systemctl restart salt-master
```

### Supported Options

| Option                            | Default        | Description                                              |
|-----------------------------------|----------------|----------------------------------------------------------|
| `storage_type`                    | `yaml_fs`      | Storage backend type                                     |
| `inventory_base_uri`              | first file_root| Base directory for the inventory                         |
| `nodes_uri`                       | `nodes`        | Subdirectory for node definitions                        |
| `classes_uri`                     | `classes`      | Subdirectory for class definitions                       |
| `compose_node_name`               | `false`        | Compose node names from directory paths                  |
| `default_environment`             | `base`         | Default environment for nodes                            |
| `allow_adapter_env_override`      | `false`        | Allow `saltenv`/`pillarenv` to override node environment |
| `ignore_class_notfound`           | `false`        | Ignore missing classes instead of raising an error      |
| `propagate_pillar_data_to_reclass`| `false`        | Pass existing pillar data into ferroclass (not yet impl)|

If `inventory_base_uri` is not specified, it defaults to the first
`file_roots` entry of the `base` environment (matching the Python
reclass adapter behavior).

## Ansible Integration

Ferroclass provides a dynamic inventory script for Ansible via the
`ferroclass-ansible` binary. It implements Ansible's
[dynamic inventory protocol](https://docs.ansible.com/ansible/latest/dev_guide/developing_inventory.html):
`--list` returns the full inventory as JSON, and `--host HOSTNAME` returns
host variables for a specific node.

### Usage with Ansible

The simplest integration is a local `ansible.cfg` in your project
directory. Combine it with a `reclass-config.yml` to tell ferroclass
where to find the inventory:

```ini
# ansible.cfg — project-local Ansible configuration
[defaults]
inventory = /usr/bin/ferroclass-ansible
```

```yaml
# reclass-config.yml — ferroclass inventory configuration
storage_type: yaml_fs
inventory_base_uri: /srv/reclass
```

With this file in place, Ansible automatically uses ferroclass as the
inventory source — no `-i` flag needed:

```shell
ansible all --list-hosts
ansible all -m ping
ansible-playbook site.yml
ansible-playbook site.yml --limit webserver
```

Alternatively, use the `-i` flag for one-off invocations:

```shell
ansible -i /usr/bin/ferroclass-ansible all --list-hosts
ansible-playbook -i /usr/bin/ferroclass-ansible site.yml
```

Note: Ansible's `-i` flag and `inventory =` config setting only accept a
path to the script — they do not pass additional arguments. Use a
`reclass-config.yml` to configure the inventory location.

### Inventory Configuration

By default, `ferroclass-ansible` reads inventory from the directory where
the script is located (useful when symlinked). Specify the inventory
location explicitly with `--inventory-base-uri`.

Configuration can also be placed in a `reclass-config.yml` file, searched
in this order: current directory, `$HOME/.config/reclass`, `/etc/reclass`.

```yaml
# reclass-config.yml
storage_type: yaml_fs
inventory_base_uri: /srv/reclass
```

CLI arguments take precedence over the config file.

### Concept Mapping

Ferroclass and Ansible use different terminology for the same concepts:

| Ferroclass concept | Ansible concept | Notes                                                  |
|--------------------|-----------------|--------------------------------------------------------|
| Node               | Host            | Each node becomes a host in the inventory              |
| Class              | Group           | Each class in a node's ancestry becomes a group        |
| Application        | Group (with `_hosts` postfix) | e.g. `ssh.server` application → `ssh.server_hosts` group |
| Parameters         | Host vars       | Node parameters become `_meta.hostvars` entries        |

Classes and applications both become Ansible groups, but with a naming
convention: applications get a `_hosts` postfix (configurable via
`--applications-postfix`). This lets you write playbooks that target
an application group:

```yaml
- name: SSH server management
  hosts: ssh.server_hosts
  tasks:
    - name: install SSH package
      ...
```

### Supported Options

In addition to the common storage and output options:

| Option                  | CLI Flag                    | Default  | Description                                    |
|-------------------------|-----------------------------|----------|------------------------------------------------|
| Applications postfix     | `--applications-postfix`    | `_hosts` | Postfix appended to applications to form groups |
| Output format           | `--output`                  | `json`   | Output format (`json` or `yaml`)               |
| Pretty-print            | `--pretty-print`            | on       | Indented, human-readable output                |
| Parameter key style     | `--parameter-key-style`    | `none`   | Validate parameter keys (`none` or `ansible`) |

### Alternative: Symlink as Inventory Script

The Python reclass adapter is traditionally symlinked to `/etc/ansible/hosts`
so Ansible uses it as the default inventory. You can do the same:

```shell
# System-wide default inventory
ln -s /usr/bin/ferroclass-ansible /etc/ansible/hosts

# Or symlink into the inventory directory for auto-detection
ln -s /usr/bin/ferroclass-ansible /srv/reclass/hosts
```

This works because `ferroclass-ansible` resolves the symlink and uses the
target directory as the default `inventory_base_uri` — so it automatically
finds the `nodes/` and `classes/` subdirectories next to the symlink.

A project-local `ansible.cfg` is generally preferred over a system-wide
symlink because it keeps the inventory path explicit and versionable.

### Comparison with Python Reclass

The Python `reclass-ansible` adapter and `ferroclass-ansible` produce
identical output for the same inventory. Both implement the same
dynamic inventory protocol (`--list` / `--host`). The key differences:

- **Binary vs script**: `ferroclass-ansible` is a compiled binary;
  `reclass-ansible` is a Python script. Both are invoked the same way.
- **No YAML anchors**: Ferroclass never emits YAML anchors/aliases. The
  `--no-refs` flag is accepted but has no effect.
- **YAML 1.2**: Ferroclass treats `yes`/`no`/`on`/`off` as strings,
  not booleans (see [Reclass Compatibility](#reclass-compatibility)).

## Reclass Compatibility

Compatibility with the [salt-formulas/reclass](https://github.com/salt-formulas/reclass)
Python implementation is a core goal. Known deviations:

**YAML 1.1 vs YAML 1.2:** Python reclass uses PyYAML which follows YAML 1.1,
where `yes`/`no`/`on`/`off` are parsed as booleans. This implementation uses
yaml-rust2, which follows YAML 1.2, where these are plain strings. Inventory
files that rely on YAML 1.1 boolean coercion will produce different results.

**Wildcard/regexp class mappings** are not yet implemented. See [docs/TODO.md](docs/todo.md)
for planned features.

**YAML anchors/aliases never emitted:** Python reclass emits YAML anchors and
aliases (`&id001`, `*id001`) by default, and the `--no-refs` / `-r` flag disables
them. This implementation never emits anchors/aliases because the merge pipeline
produces owned values with no shared references. The `-r` / `--no-refs` flag is
accepted for CLI compatibility but has no effect (anchors are always suppressed).

**Class name interpolation does not have access to the node's own parameters:** The
salt-formulas/reclass README-extensions include an example implying that node-level
parameters are available for class name interpolation. This does not work in the Python
implementation either (it raises `ClassNameResolveError`). Class name interpolation
can only reference parameters from previously processed ancestor classes, not the
node's own parameters. The example in the reclass documentation is incorrect.

**`inventory_ignore_failed_node` skips all merge errors, not just YAML parse errors:**
Python reclass only skips nodes that raise `yaml.scanner.ScannerError` (malformed
YAML). This implementation skips nodes that fail for any reason during merge (class
not found, interpolation errors, type merge conflicts). Since Rust loads all YAML
upfront, parse errors are not per-node during iteration; the meaningful per-node
failures are merge errors, which this flag covers.

**`scalar_reclass_parameters` not implemented:** Python reclass supports a
`scalar_parameters` config option that promotes a designated parameter key to a higher
merge priority. This feature has zero test coverage, zero documentation, and no known
users. It will not be implemented.

**`--ignore-class-notfound` is a boolean flag, not a string parameter:** Python reclass
defines this as a string-valued option, meaning it requires a value like
`--ignore-class-notfound True`. However, downstream the value is only ever checked for
Python truthiness. This makes the string form a design bug: passing
`--ignore-class-notfound False` would set the value to the *string* `"False"`, which
Python evaluates as truthy — the opposite of the intended behavior. This implementation
treats it as a proper boolean flag (`--ignore-class-notfound`), which matches the actual
semantics and avoids the string-truthiness footgun.

**`inventory_ignore_failed_render` / `+IgnoreErrors` granularity:** Python reclass
deletes individual export keys that fail to resolve when `+IgnoreErrors` or
`inventory_ignore_failed_render` is set. This implementation instead skips the entire
node when the merge with inventory fails. Per-key deletion within a single node's
exports requires architectural changes to the merge pipeline (partial success from
interpolation). The `+IgnoreErrors` per-query flag is parsed, stored, and checked — keys
whose inv query values fail are removed from the hash. However, the most common failure
path (unresolvable `${...}` references within export values) causes the entire node merge
to fail rather than a per-key deletion, because the merge pipeline does not support
per-key error recovery.

## Roadmap

See [docs/TODO.md](docs/todo.md) for planned features and known incompatibilities.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for build instructions, code quality requirements,
and architecture guidelines.

## License

This project is licensed under the [Mozilla Public License 2.0](LICENSES/MPL-2.0.txt).

SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
SPDX-License-Identifier: MPL-2.0

[reclass]: https://reclass.pantsfullofunix.net/
