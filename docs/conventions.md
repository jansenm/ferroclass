<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Development Conventions

This document records project coding conventions — rules, patterns, and practices
that go beyond style into design decisions. It is the authoritative reference for
error handling, data modeling, and other cross-cutting concerns.

Agents: see [AGENTS.md](../AGENTS.md) for architecture rules and agent workflow.
Contributors: see [CONTRIBUTING.md](../CONTRIBUTING.md) for build, test, and release process.

## Error Handling

All error types use [`snafu`](https://docs.rs/snafu) with `#[derive(Debug, Snafu)]`.
The project follows the **funnel principle**: every line in an error chain adds exactly
one new fact, narrowing from general context to the specific problem.

### The Funnel Principle

When a user sees an error, they should be able to read top-to-bottom and understand
what went wrong, where it went wrong, and why — with no noise, no repetition, and no
gaps.

```
error loading repository at '/srv/inventory'
  caused by: class 'apache' not found
```

Each line adds one fact. The top line establishes *where* (the repository). The
second line identifies *what* (the missing class). No line is redundant.

### Display Rules

1. **Short sentences.** Lowercase after the first word unless it's a proper noun.
   ```rust
   // Good
   #[snafu(display("class '{class_name}' not found"))]

   // Bad — preamble adds nothing
   #[snafu(display("failed to find class '{class_name}'"))]
   ```

2. **Precise terminology.** Use "mapping" (not "hash" or "dict"), "list" (not "array"),
   "top level key" (not "root key" or "field").
   ```rust
   // Good
   #[snafu(display("top level key '{key}' is not supported"))]

   // Bad
   #[snafu(display("unsupported root field '{key}'"))]
   ```

3. **No redundant information.** Never repeat context already present elsewhere in
   the chain. If the layer above says "repository at '/srv'", the layer below should
   not repeat the path.

4. **No preamble.** Never start a message with "Failed to...", "Error:", or
   "Unable to...". The `Err` context already conveys failure; the message should
   state the problem directly.

5. **Quotes around user-provided names.** Class names, node names, file paths, and
   config keys appear in single quotes.
   ```rust
   #[snafu(display("node '{node_name}' not found"))]
   ```

### Source Chain Discipline

1. **Always use `source:` not `detail: e.to_string()`.** The `source` field preserves
   the error chain so `std::error::Error::source()` can walk it. Stringifying discards
   the chain — the user sees only one line and loses all context below.
   ```rust
   // Good — chain is preserved
   #[snafu(display("class mapping error"))]
   ClassMapping { source: class_mapping::Error }

   // Bad — chain is broken, downstream context lost
   #[snafu(display("class mapping error: {detail}"))]
   ClassMapping { detail: String }
   ```

2. **Never discard errors with `_|_`.** If you have a match arm that swallows an
   error variant, map it to a meaningful variant instead. Every `Err` the user might
   see should have a useful chain.

3. **Never break the chain with `.to_string()`.** If you need to store error text
   in a non-`source` field, reconsider — you almost certainly want `source:` instead.

### Pass-through Layers and `#[snafu(transparent)]`

A **pass-through layer** wraps an inner error without adding any information. Example:

```rust
// Pass-through — display just says "interpolation error"
#[snafu(display("interpolation error"))]
InterpolationError { source: interpolation::Error }
```

This variant adds nothing — "interpolation error" is already conveyed by the inner
type. It is noise in the chain.

**Eliminate pass-throughs with `#[snafu(transparent)]`:**

```rust
#[snafu(transparent)]
Interpolation { source: interpolation::Error }
```

When a variant is transparent:
- `Display` delegates to the source type — the variant is invisible in the output.
- `std::error::Error::source()` returns `None` — the variant disappears from the chain.
- The inner error becomes the outer error for all practical purposes.

**When a layer IS justified.** If the variant adds context — a name, a path, a phase —
it is not a pass-through:

```rust
// Justified — adds the node name
#[snafu(display("error merging node '{node_name}'"))]
Merge { source: inv::Error, node_name: String }

// Justified — adds the repository path
#[snafu(display("error loading repository at '{base_uri}'"))]
Repository { source: file_system::Error, base_uri: String }
```

**Practical notes on transparent variants:**

- Snafu does not generate context selectors (`*Snafu`) for transparent variants.
  Use `.map_err(TargetError::from)` instead of `.context(TargetSnafu {})`.
- When multiple error types have `From<InnerError>` impls, you must specify the
  target type explicitly: `.map_err(AnsibleInventoryError::from)`, not
  `.map_err(Into::into)`.
- Transparent variants participate in `From` impls normally — `From<Source> for
  Outer` is still generated.

### Dead Variants

Remove unused error enum variants immediately. A variant that is defined but never
constructed is dead code. It clutters the enum, confuses readers, and forces
downstream `match` arms to handle phantoms.

### Changing Error Variants — Checklist

When renaming or removing a variant, update:

1. The variant definition in the error enum.
2. All construction sites (`Error::Variant { ... }`).
3. All `match` arms that destructure the variant.
4. All `From<...>` impls that map to/from the variant.
5. Panic messages in tests that reference the variant by name.
6. Run `cargo test` — compiler errors will flag any missed sites.

### Current Error Architecture

The error types form a layered hierarchy. Each layer adds context as errors
propagate upward:

```
value_merge::Error           (leaf: type mismatch details)
  ↑
interpolation::Error         (+ "type conflict while resolving references")
  ↑
merge::Error                 (+ class name / merge phase context)
  ↑
inventory::Error             (+ repository path, node duplication)
  ↑
output adapter errors        (+ adapter-specific context when justified)
  ↑
cmd / binary errors           (+ node name for merge failures, transparent otherwise)
```

Layers that add information have explicit display messages. Layers that merely
re-wrap are marked `#[snafu(transparent)]` and vanish from the chain.