# Ferroclass

![CI](https://github.com/jansenm/ferroclass/actions/workflows/ci.yml/badge.svg)

Ferroclass is a lightweight configuration management database (CMDB) implementation. It is a
reimplementation of [reclass](https://reclass.pantsfullofunix.net/) in Rust, with full CLI
compatibility for the `reclass`, `reclass-ansible`, and `reclass-salt` commands.

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

Binaries: `reclass`, `reclass-ansible`, `reclass-salt`
Man pages: `man reclass`, `man reclass-ansible`, `man reclass-salt`

### RPM Packages

```shell
make dist                    # Create source + vendor tarballs
make packaging               # Build RPM packages
```

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
reclass --nodeinfo web --inventory-base-uri ./inventory
reclass --inventory --output json --inventory-base-uri ./inventory
reclass-ansible --list --inventory-base-uri ./inventory
reclass-salt --top --inventory-base-uri ./inventory
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
man reclass
man reclass-ansible
man reclass-salt
```

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

SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
SPDX-License-Identifier: MPL-2.0

[reclass]: https://reclass.pantsfullofunix.net/
