<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Process

This document describes the steps Ferroclass performs to produce the final merged
output for a node, in the order they are applied.

## 1. Configuration

CLI arguments are parsed and merged with an optional `reclass-config.yml` configuration file.
Config files are searched in this order: current directory, `$HOME/.config/reclass`,
`/etc/reclass`. CLI arguments take precedence over file configuration.

The result determines the storage backend, base directory, nodes/classes directories,
and the output format.

## 2. Discovery

**File-system backend**: The classes directory and nodes directory are walked recursively.
Only files with a `.yml` or `.yaml` extension are considered. The name of each element
is derived from its path following the rules described in [Naming](../README.md#naming).

**Single-file backend**: A single YAML file is read and split on document separators
(`---`). Each document must contain a `name` key. Documents with `type: node` are parsed
as nodes; all others are parsed as classes.

## 3. Parsing

Each discovered YAML file (or document) is parsed into its constituent parts:

- `classes` — ordered list of class names to inherit from
- `environment` — a string (defaults to `base`)
- `parameters` — a hash of configuration values
- `applications` — an ordered list of application names
- `exports` — a hash of exported values (used by inventory queries)

Unknown keys produce an error.

After parsing, reference patterns are detected in string values:

- **Interpolation references** (`${...}`) — preserved for later resolution
- **Inventory query expressions** (`$[...]`) — preserved for later resolution

## 4. Inheritance Chain Resolution & Merging

For a given node, the inheritance chain is built and merged in a single pass using a
**depth-first** traversal. The result is a flat, ordered list of classes from the
deepest ancestor to the node itself, with the node as the final element. Each element
is merged into an accumulator as it is encountered.

The pipeline, from lowest to highest priority (later entries overwrite earlier ones):

1. **Class mappings** — glob/regex patterns matched against the node name produce a
   list of auto-included classes. Regex capture groups in class names are substituted
   from the match. These are resolved first, building an initial accumulator.
2. **Input data** — if `set_input_data()` was called (used by the Salt adapter), the
   input data parameters are merged on top. These provide low-priority defaults that
   classes and node parameters override by design.
3. **Automatic parameters** — `_reclass_` metadata (`name.full`, `name.short`,
   `environment`) are merged on top.
4. **Node classes** — the node's own `classes` list is resolved recursively. For each
   class, its own `classes` list is resolved **before** the class is merged into the
   accumulator. Entries may contain `${...}` references (interpolated against the
   accumulator's already-merged parameters) and relative class names (`.foo`, `..bar`).
   A class encountered a second time is ignored (no duplicate entries).
5. **Node entity** — the node's own parameters, classes, applications, environment,
   and exports are merged on top with the highest priority.

A recursive/circular inheritance is a non-recoverable error. See
[Inheritance Chain](../README.md#inheritance-chain) for an example.

The merge follows the rules described in [Merging Values](../README.md#merging-values) and
[Merging Elements](../README.md#merging-elements). In summary:

| Field        | Rule                                                                         |
|--------------|------------------------------------------------------------------------------|
| Classes      | Accumulated (parent list + child name appended)                              |
| Environment  | Child wins if non-empty; otherwise parent is kept                            |
| Applications | Concatenated (parent first, child appended)                                  |
| Parameters   | Deep-merged (hashes recursive, lists appended, scalars overwritten by child) |

When a reference value collides with another value during merging and the type of the
referenced value cannot yet be determined (because it hasn't been resolved), the merge
is **deferred**: the conflicting values are collected into a deferred-merge list to be
resolved during interpolation.

Prefix markers modify merge behavior:

- **`~key`** (override prefix) — the value replaces the existing key entirely (no deep merge).
- **`=key`** (constant prefix) — the value is locked; later classes cannot change it. In
  strict mode (the default), attempting to change a constant raises an error.

## 5. Interpolation

After merging, references are resolved by looking up parameter paths in the merged node's
own parameters:

- **`${...}` references** are replaced by the value they point to, preserving type (a
  reference to a list becomes a list, a reference to a hash becomes a hash).
- **Mixed strings** containing both literal text and `${...}` references are resolved
  by concatenation, producing a single string.
- **Deferred merges** are resolved by first resolving all references, then merging the
  resulting values left to right using the normal merge rules.

Circular references (e.g., `a: ${b}`, `b: ${a}`) are detected and reported as errors.

### Inventory Queries

When a node's parameters or exports contain inventory query expressions (`$[...]`), a
two-pass rendering is used:

1. **Pass 1** — all nodes are merged and interpolated to build an inventory map of exports.
2. **Pass 2** — nodes with queries are re-interpolated using that inventory map.

Query types: `VALUE` (look up an export path), `TEST` (logical/equality test), and
`LIST_TEST` (query over all matching nodes).

Nodes can use `+AllEnvs` to query exports across environments, and `+IgnoreErrors` to
suppress individual query resolution failures (removing the key instead of propagating
the error).

## 6. Output

The merged and interpolated results are serialized to the requested format (YAML or JSON).
