<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Interface Design: Explorer, LSP, and MCP Server

This document defines three interfaces for ferroclass, each serving a
different interaction model. TUI and Web are the same interface with
different renderers — they share features, API requirements, and data
models. The LSP and MCP serve fundamentally different use cases.

## The Killer Feature: Merge Replay

All three interfaces share one feature that makes ferroclass genuinely
useful beyond what the CLI provides: **merge replay** — the ability to
select a node, see its inheritance chain, and click through it step by
step while watching parameters evolve.

This is how you answer "why does this node have this value?" You walk
the chain: "after `base`, port was 80. After `webserver`, port became
8080. After `production`, the `${env}` reference resolved to `prod`."
Each step shows the complete parameter state at that point in the merge.

The library needs a `merge_node_with_replay()` API that returns not just
the final merged node, but every intermediate `MergeStep` — the class
that was merged, and the full parameter state after it was applied.
See `docs/api-review.md` Phase 3 for the proposed data structures.

This feature is so central that it should be designed and built first,
before the interface shells.

## Common Requirements

All three interfaces share these needs:

| Need                                  | Current Gap                              | API Change Required            |
|---------------------------------------|------------------------------------------|--------------------------------|
| Load inventory once, query many times | Adapters re-load from disk on every call | Decouple adapters from loading |
| Search and filter nodes               | Only `get_node(name)` and `node_names()` | Query API with indexing        |
| Thread-safe inventory handle          | `Rc<Value>` is `!Send + !Sync`           | Switch to `Arc<Value>`         |
| Progress reporting during load        | No callback or event system              | Tracing spans or callback hook |
| Unified error handling                | Fragmented error types                   | Single `ferroclass::Error`     |

---

## 1. Explorer (TUI + Web UI)

The Explorer is the interactive inventory browser. It has two frontends —
a terminal UI and a web UI — sharing the same features and data model.
The TUI is for operators who live in the terminal. The Web UI adds
shareable URLs, collaborative access, and richer rendering.

### Target Users

- Operations engineers debugging why a node has certain parameters
- Developers checking class inheritance chains
- Teams sharing inventory exploration via browser
- Anyone wanting quick inventory exploration without remembering CLI flags

### Core Features

#### Merge Replay (the main feature)

- Select a node → see its full inheritance chain (class by class)
- Click on any step in the chain → see the complete parameter state at
  that point in the merge
- See which class contributed which keys (new keys highlighted, changed
  keys marked)
- Toggle between pre-interpolation view (raw `${...}` references visible)
  and post-interpolation view (all references resolved)
- Step forward/backward through the merge like a debugger
- Diff any two steps to see exactly what changed

```
┌─ Merge Replay: web01 ──────────────────────────────────────────┐
│ Step 1/5: ClassMapping "default"                               │
│ Step 2/5: AutoParameters                                       │
│ ▸ Step 3/5: Class "base"                    ◄── selected       │
│   Step 4/5: Class "webserver"                                   │
│   Step 5/5: Node fields                                        │
├────────────────────────────────────────────────────────────────┤
│ Parameters after step 3 (class "base"):                        │
│   hostname: web01                                             │
│   role: base                                                  │
│ + base_setting: true           ◄── new key from this step     │
│ + log_level: INFO              ◄── new key from this step     │
│                                                                │
│ [Pre-interpolation] [Post-interpolation] [Diff from step 2]  │
└────────────────────────────────────────────────────────────────┘
```

#### Node Browser

- List all nodes with optional filtering (by name pattern, class, environment)
- Select a node → see merged parameters, classes, applications, exports
- Expand/collapse nested parameter values
- Show the merge order: which class contributed which parameter

#### Class Browser

- List all classes
- Select a class → see its parameters, applications, environment
- Show which nodes include a given class
- Trace class inheritance (class A includes class B includes class C)

#### Search

- Full-text search across all node parameters
- Find nodes where a specific key is set (or unset)
- Find nodes matching a parameter value pattern
- Regex and glob support

#### Configuration View

- Show current `Options` (storage type, inventory base URI, merge config)
- Show class mappings (glob/regex patterns and their target classes)
- Allow toggling `automatic_parameters`, `class_mappings_match_path`

### TUI Specifics

The TUI runs an event loop (e.g., ratatui/crossterm). Inventory is loaded
once at startup (or on user command), then queried repeatedly:

```
┌─ Node List ─────────────────┐ ┌─ Node Detail: web01 ─────────────┐
│ ▸ web01                      │ │ Step 3/5: Class "webserver"       │
│   web02                      │ │ + port: 8080                     │
│   db01                       │ │ + role: webserver                │
│ ▸ db02                       │ │                                   │
│   monitor01                  │ │ [◀ Step 2] [Step 4 ▶]            │
│                              │ │ [Pre-interp] [Post-interp] [Diff] │
│ [Search: role=web]           │ └──────────────────────────────────┘
└──────────────────────────────┘
```

**Dependencies**: `ratatui` + `crossterm`, thread-safe inventory (Phase 1),
query API (Phase 2), merge replay (Phase 3).

### Web UI Specifics

The web version of merge replay is especially powerful because clicking
through steps is a natural browser interaction. Each step in the chain
is a clickable element; the parameter panel updates live.

- Step-by-step navigation with prev/next buttons and clickable step list
- Diff view: highlight keys added, changed, or overridden at each step
- Pre/post interpolation toggle: show raw `${...}` references or resolved
  values with a single click
- Shareable URLs: `/nodes/web01/replay?step=3` links directly to a
  specific merge step
- Side-by-side diff of any two steps

#### REST API

The web UI is backed by a JSON API that can also be used programmatically:

| Endpoint                                   | Description                                         |
|--------------------------------------------|-----------------------------------------------------|
| `GET /api/v1/nodes`                        | List all nodes (with optional filters)              |
| `GET /api/v1/nodes/:name`                  | Get merged node detail                              |
| `GET /api/v1/nodes/:name/parameters`       | Get merged parameters only                          |
| `GET /api/v1/nodes/:name/classes`          | Get resolved class list                             |
| `GET /api/v1/nodes/:name/replay`           | Full merge replay: all steps with intermediate state |
| `GET /api/v1/nodes/:name/replay/:step`     | Single merge step (for step-by-step navigation)     |
| `GET /api/v1/nodes/:name/raw`              | Get node before interpolation/merge                 |
| `GET /api/v1/classes`                       | List all classes                                    |
| `GET /api/v1/classes/:name`                | Get class detail                                    |
| `GET /api/v1/classes/:name/usage`           | Which nodes use this class                          |
| `GET /api/v1/search?q=pattern`             | Full-text search across nodes and parameters        |
| `GET /api/v1/inventory/config`              | Current inventory configuration                     |
| `POST /api/v1/inventory/reload`             | Reload inventory from disk                          |
| `GET /api/v1/formats/ansible?node=name`    | Ansible-formatted output                            |
| `GET /api/v1/formats/salt?node=name`        | Salt-formatted output                               |

#### WebSocket Updates

- Notify connected clients when inventory files change on disk (via inotify/polling)
- Push incremental merge results as they complete
- Show merge progress bar for large inventories

#### Multi-User Concerns

- Read-only by default (inventory is files on disk, not edited through the API)
- Optional write endpoint for creating/modifying node and class YAML files (future scope)
- Authentication via API key or HTTP basic auth (simple)

### API Requirements (Explorer)

| Explorer Feature                | Library API Needed                                                                    |
|---------------------------------|---------------------------------------------------------------------------------------|
| Node list with filters          | `find_nodes(pattern)`, `find_nodes_by_class(name)`, `find_nodes_by_environment(env)`  |
| Merged node parameters          | `inventory.merge_node(name)` (already exists)                                         |
| Merge replay                    | `inventory.merge_node_with_replay(name)` → `MergeReplay` with `Vec<MergeStep>`        |
| Class list and lookup           | `inventory.class_names()`, `inventory.get_class(name)` (already exists)               |
| Class inverse lookup            | `find_nodes_by_class(name)`                                                           |
| Unresolved interpolation view   | Pre-interpolation state from `MergeReplay::pre_interpolation_parameters`              |
| Circular reference detection    | Already detected during merge; expose as structured data                              |
| Full-text parameter search      | `search_parameters(query)` returning node names + matching keys                       |
| Reload inventory from disk      | `inventory.reload()` or `load()` again (needs decoupled adapters)                     |
| JSON serialization of all types | `Serialize` already exists for most types; verify completeness                       |
| Streaming merge progress        | Callback or channel-based progress reporting                                           |
| File watch + reload             | `inventory.reload()` or `load()` again; decouple from adapter                          |
| Concurrent read access          | `Arc<Inventory>` (Phase 1: Send + Sync)                                               |
| Ansible/Salt format endpoints   | Decoupled adapter functions (Phase 0)                                                 |

### Technology Choices

**TUI**: `ratatui` + `crossterm` for rendering. Single binary, no network.

**Web UI**: `axum` or `actix-web` for the backend. Start with
server-rendered HTML (askama/templates) for simplicity, add JSON API
endpoints from day one. File watching via `notify` crate.
`Arc<Inventory>` behind `RwLock` for concurrent reads.

---

## 2. LSP (Language Server Protocol)

An LSP meets users where they already work — their editor. Inventory
YAML files reference other YAML files (classes, nodes). The LSP turns
those references into navigable, validated, auto-completable links,
exactly like an LSP does for code.

### Target Users

- Operations engineers editing inventory YAML in VS Code, Neovim, or any LSP-aware editor
- Developers writing new classes or nodes who need autocompletion
- Anyone wanting real-time validation of class references and interpolation

### Core Features

#### Tier 1: Immediately Useful

| Feature          | LSP Method                      | What it does                                                                                                    |
| ---------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Diagnostics      | `textDocument/publishDiagnostics` | Red-squiggly on missing classes, circular references, unresolved `${...}`, invalid `~`/`=` usage, YAML errors      |
| Go-to-definition | `textDocument/definition`         | Click a class name in a node file → jump to the class YAML file. Click a node name → jump to the node file          |
| Find references  | `textDocument/references`         | Right-click a class → find all nodes that include it. Right-click a node → find all places it's referenced          |
| Hover            | `textDocument/hover`              | Hover over a class name in a node → see the merged parameters that class contributes. Hover over `${foo:bar}` → see the resolved value |

#### Tier 2: Very Useful

| Feature           | LSP Method                      | What it does                                                                                                                          |
| ----------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Autocomplete      | `textDocument/completion`         | Type a class name → get suggestions from all classes in the inventory. Type `${` → suggest parameter paths. Type `~` or `=` → suggest override/constant markers |
| Document symbols  | `textDocument/documentSymbol`     | Outline view showing classes, parameters, applications, exports in the current file                                                    |
| Workspace symbols | `workspace/symbol`                | Fuzzy search for class or node names across the entire inventory                                                                       |
| Merge replay      | CodeLens or custom notification   | Click a "show merge steps" lens on a node → open a panel showing step-by-step merge (the killer feature, surfaced via LSP)           |

#### Tier 3: Nice to Have

| Feature          | LSP Method                       | What it does                                                                                                    |
| ---------------- | -------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Rename           | `textDocument/rename`              | Rename a class → update all nodes that reference it                                                            |
| Code actions     | `textDocument/codeAction`          | "Add missing class" quick fix, "Extract parameters to new class"                                                |
| Folding          | `textDocument/foldingRange`        | Fold classes, parameters, exports sections                                                                     |
| Semantic tokens  | `textDocument/semanticTokens/full` | Color class references differently from strings, highlight `${...}` interpolation differently from plain strings |
| File watching    | `workspace/didChangeWatchedFiles`  | Re-merge inventory when any file changes, push updated diagnostics                                             |

### How the LSP Maps to the Library

```
Diagnostics ──────→ inventory.load() + inventory.merge_node()
                     catch Error variants, map to LSP DiagnosticSeverity

Go-to-definition ──→ storage layer: find which file defines a class/node
                     (needs: file path → line number mapping from parser)

Find references ───→ query API: find_nodes_by_class(name)
                     + text search for class name in node files

Hover ─────────────→ merge_node_with_replay(name) → show step parameters
                     or merge_node(name) → show final merged parameters

Autocomplete ──────→ inventory.class_names() / inventory.node_names()
                     + parameter key names from merged node

Merge replay ──────→ merge_node_with_replay(name) → CodeLens / custom notification
```

### What the LSP Needs from the Library

| Need                          | Current state                                | What's needed                                                                                     |
| ----------------------------- | -------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Source location mapping       | Parser discards file/line info after parsing | Store `SourceLocation { file, line, col }` on each `Class`, `Node`, and `Value` during parsing    |
| File path to entity mapping   | `Inventory` stores entities by name, not file | A `HashMap<String, PathBuf>` mapping class/node names to their YAML files                          |
| Incremental re-parse          | `load()` re-reads everything from disk        | Watch for file changes, re-parse only affected files, re-merge affected nodes                      |
| Query API                     | Only exact name lookup and full iteration     | `find_nodes_by_class()`, `find_nodes_by_environment()`, `search_nodes()`                           |
| Merge replay                  | Not yet implemented                          | `merge_node_with_replay()` from Phase 3 in `api-review.md`                                         |
| Thread-safe inventory         | `Rc<Value>` is `!Send + !Sync`               | Phase 1 `Arc` migration                                                                            |

**Source location tracking** is the one thing not yet in the roadmap
that the LSP specifically needs. It's also useful for error messages —
currently errors say "class not found" but not *where* the reference is.

### Architecture

```
┌─────────────────────────────────────────────────────┐
│            Editor (VS Code, Neovim, etc.)            │
│            ↕ LSP protocol (JSON-RPC over stdio)      │
├─────────────────────────────────────────────────────┤
│              ferroclass-lsp (binary)                  │
│                                                      │
│  ┌──────────────────┐  ┌──────────────────────────┐  │
│  │  LSP Protocol     │  │  Inventory Manager        │  │
│  │  Handler          │  │  (owns Arc<Inventory>)     │  │
│  │  (tower-lsp)      │  │  - file watching (notify)  │  │
│  │                   │  │  - incremental re-parse    │  │
│  │                   │  │  - merge on change          │  │
│  └──────────────────┘  └──────────────────────────┘  │
│           │                        │                  │
│           │     ┌──────────────────┘                  │
│           ▼     ▼                                     │
│  ┌────────────────────────┐                          │
│  │  ferroclass (library)    │                          │
│  │  - load()               │                          │
│  │  - merge_node()         │                          │
│  │  - merge_node_with_     │                          │
│  │    replay()             │                          │
│  │  - query API            │                          │
│  │  - source locations     │                          │
│  └────────────────────────┘                          │
└─────────────────────────────────────────────────────┘
```

### Technology Choices

- **LSP framework**: `tower-lsp` — standard Rust LSP framework, handles JSON-RPC transport, protocol types, request routing
- **Protocol types**: `lsp-types` crate — all LSP protocol definitions
- **File watching**: `notify` crate — cross-platform inotify/FSEvents/kqueue
- **Transport**: stdio (standard for LSP servers)
- **Concurrency**: `Arc<Inventory>` behind `RwLock`; background re-merge on file change

### Effort Estimate

| Component                                              | Effort    | Notes                                         |
| ------------------------------------------------------ | --------- | --------------------------------------------- |
| LSP skeleton (tower-lsp, JSON-RPC, basic lifecycle)    | 1-2 weeks | Boilerplate, well-documented                  |
| Diagnostics (push errors on file open/save)             | 1 week    | Map existing error types to LSP diagnostics   |
| Go-to-definition                                       | 2-3 weeks | Requires source location tracking in parser   |
| Find references                                        | 1 week    | Requires query API                            |
| Hover                                                  | 1 week    | Requires merge replay or at least merge_node  |
| Autocomplete                                           | 1-2 weeks | Needs query API + parameter key extraction    |
| Merge replay via CodeLens                              | 2-3 weeks | Requires full merge replay API                |
| Incremental re-parse                                   | 2-3 weeks | Hard; can start with full re-parse + debounce |
| File watching + auto-diagnose                          | 1 week    | `notify` crate                                  |

**Total: ~12-16 weeks** for a usable LSP with diagnostics, go-to-definition,
references, hover, and autocomplete. Merge replay and incremental parsing
add another 4-6 weeks.

### Dependencies on Library Phases

| LSP Feature          | Depends on (from api-review.md)                         |
| -------------------- | ------------------------------------------------------- |
| Diagnostics          | Phase 0: unified error types                            |
| Go-to-definition     | New: source location tracking in parser                 |
| Find references      | Phase 2: query API                                      |
| Hover / merge replay | Phase 3: merge replay API                               |
| Autocomplete         | Phase 0: public `class_names()`/`node_names()`          |
| Thread safety        | Phase 1: `Arc<Value>` for concurrent LSP request handling |
| File watching        | Phase 2: incremental load/reload                        |

---

## 3. MCP (Model Context Protocol) Server

An MCP server exposes inventory data to AI agents, allowing them to
answer questions like "which nodes are web servers?" or "what parameters
does db01 inherit from the postgres class?" without loading raw YAML files.

### Target Users

- AI coding assistants (via MCP client integration)
- Automation tools that query inventory programmatically
- ChatOps bots

### Core Features

#### MCP Tools

The server exposes these tools following the MCP specification:

| Tool Name                          | Parameters                        | Returns                                    |
|------------------------------------|------------------------------------|--------------------------------------------|
| `list_nodes`                       | `filter?: string`, `class?: string`, `environment?: string` | List of node names with summary |
| `get_node`                         | `name: string`                     | Full merged node (parameters, classes, applications, exports) |
| `get_node_replay`                  | `name: string`                     | Merge replay: all steps with intermediate parameter state |
| `get_node_parameters`              | `name: string`                     | Merged parameters only (as dict)            |
| `get_node_classes`                  | `name: string`                     | Resolved class list with merge order        |
| `get_node_merge_provenance`        | `name: string`, `key?: string`     | Which class contributed each parameter value |
| `list_classes`                     | `filter?: string`                  | List of class names                         |
| `get_class`                        | `name: string`                     | Class detail (parameters, applications)     |
| `find_class_usage`                 | `name: string`                     | Nodes that include this class               |
| `search_parameters`                | `query: string`, `value?: string`  | Nodes matching key/value pattern            |
| `get_inventory_config`             | —                                  | Current options, storage type, class mappings |
| `reload_inventory`                 | —                                  | Reload from disk                            |
| `get_interpolation_sources`       | `node: string`, `key: string`      | Show reference chain for an interpolated value |
| `explain_merge`                    | `node: string`                     | Human-readable explanation of merge order and conflicts |

#### MCP Resources

Static data exposed as URI-addressable resources:

| URI Pattern                            | Description                              |
|----------------------------------------|------------------------------------------|
| `ferroclass://nodes`                   | All node names                            |
| `ferroclass://nodes/{name}`            | Merged node detail                        |
| `ferroclass://nodes/{name}/replay`      | Merge replay: step-by-step merge history  |
| `ferroclass://nodes/{name}/raw`         | Node before merge/interpolation           |
| `ferroclass://classes`                  | All class names                           |
| `ferroclass://classes/{name}`           | Class detail                              |
| `ferroclass://config`                   | Inventory configuration                   |

#### MCP Prompts

Pre-configured prompt templates:

| Prompt Name                    | Description                                              |
|--------------------------------|----------------------------------------------------------|
| `debug_node`                   | "Investigate why node X has parameter Y set to Z"        |
| `trace_merge`                  | "Walk the merge chain for node X step by step"           |
| `compare_nodes`                | "Compare the parameters of node A and node B"           |
| `trace_class`                  | "Show the full class inheritance chain for class X"      |
| `find_unused_classes`          | "Find classes not included by any node"                 |

### API Requirements (MCP)

| MCP Feature                     | Library API Needed                                          |
|----------------------------------|-------------------------------------------------------------|
| JSON-serializable all types      | `Serialize` on `Node`, `Class`, `Inventory`, `Value`       |
| Merge replay                     | `merge_node_with_replay(name)` → `MergeReplay` with `Vec<MergeStep>` |
| Human-readable merge explanation | `explain_merge(name)` → formatted string                     |
| Reference chain tracing          | `get_interpolation_sources(node, key)` → list of references |
| Lightweight startup              | Lazy loading or incremental inventory construction          |
| stdio transport                  | MCP servers communicate over stdin/stdout (no HTTP needed) |

### Technology Choices

- **MCP SDK**: `rmcp` crate (Rust MCP implementation) or `mcp-rust-sdk`
- **Transport**: stdio (for local agent integration) or HTTP/SSE (for remote)
- **No web framework needed**: MCP is a JSON-RPC protocol over stdio
- **Inventory as `Arc<Inventory>`**: loaded once, queried many times
- **Schema**: Define MCP tool schemas as JSON Schema matching the parameter
  types above

---

## API Changes Needed (Summary)

### Must Have (for all three interfaces)

| Change                                   | Phase  | Effort | Impact                     |
|------------------------------------------|--------|--------|----------------------------|
| Decouple adapters from loading           | 0      | Medium | Enables all interfaces      |
| Unify error types                         | 0      | Low    | Cleaner public API          |
| Make `add_node`/`add_class` public       | 0      | Trivial| Enables programmatic construction |
| Switch `Rc` → `Arc` in `Value`           | 1      | Medium | Enables thread safety       |
| Query API (filter, search)               | 2      | Medium | Enables all interfaces      |

### Should Have (the core differentiator)

| Change                                   | Interface      | Effort | Impact                                          |
|------------------------------------------|----------------|--------|-------------------------------------------------|
| Merge replay API                         | All            | Medium | Step-by-step merge inspection — the killer feature |
| Pre/post interpolation toggle            | All            | Low    | Show raw `${...}` refs vs resolved values        |
| Source location tracking in parser       | LSP            | Medium | Enables go-to-definition, better error messages |
| Interpolation source tracing             | Explorer, MCP  | Medium | Debug reference chains                          |
| `explain_merge()` human-readable output  | MCP            | Low    | Agent-friendly explanations                     |
| Lazy/incremental inventory loading        | Explorer, MCP  | High   | Fast startup for large inventories              |

### Nice to Have

| Change                                   | Interface | Effort | Impact                     |
|------------------------------------------|-----------|--------|----------------------------|
| File watching + auto-reload              | Explorer  | Medium | Live updates               |
| Write endpoints (create/modify nodes)    | Explorer  | Very High | Full lifecycle management |
| Requirements/prerequisites validation    | All       | High   | Policy enforcement         |
| Rename refactoring                        | LSP       | Medium | Rename class → update all refs |
| Code actions (extract class, add missing) | LSP       | High   | Quick fixes and refactors  |

---

## Open Questions

1. **Merge replay: snapshot granularity** — Should each step snapshot the
   full `MergeAccumulator` (parameters, classes, applications, environment,
   exports), or just parameters? Full snapshots cost more memory but enable
   "show me which classes were resolved at step 3." Parameter-only snapshots
   are lighter but lose that context.

2. **Merge replay: depth-first visibility** — When class A inherits from B
   which inherits from C, the depth-first resolution merges C → B → A. Should
   the replay show each parent class as a separate step (C step, then B step,
   then A step), or collapse the entire inheritance resolution into one step?
   Showing each parent class is more useful for debugging but produces more
   steps.

3. **Interpolation before/after** — Should we expose both the unresolved
   (references visible) and resolved (interpolated) views of a node?
   The `MergeReplay` design captures `pre_interpolation_parameters` which
   contains `Value::Reference`, `Value::DeferredMerge`, etc. The interface
   needs to render these meaningfully — e.g., `${host:name}` shown as
   "reference to host:name → resolved to 'web01'".

4. **Incremental loading** — For large inventories, should we support loading
   nodes on demand (lazy) rather than all at once? This would require
   restructuring `Inventory::load()` significantly.

5. **Explorer: read-only or read-write?** — A read-only explorer is
   straightforward. A read-write explorer needs YAML file generation,
   validation, and conflict resolution. Start read-only.

6. **MCP: local or remote?** — Local (stdio) is simpler and matches how
   agents currently use MCP. Remote (HTTP/SSE) enables shared instances.
   Start local.

7. **Authentication** — The Explorer (web frontend) needs at least API key
   auth. The MCP server (stdio) inherits the agent's local access. The LSP
   inherits the editor's local access. How much auth infrastructure do we
   build?

8. **Binary layout** — Should each interface be a separate binary
   (`ferroclass-tui`, `ferroclass-web`, `ferroclass-lsp`, `ferroclass-mcp`)
   or subcommands of the `reclass` binary? Separate binaries align with the
   architecture rule that binaries are thin wrappers over the library.

9. **Diff computation** — For the "diff any two steps" feature in merge
   replay, do we need a structural diff on `ParametersType` (showing added,
   removed, and changed keys with old/new values)? This is similar to
   `serde_json` diffing. Could use a simple key-by-key comparison since
   `ParametersType` is a `LinkedHashMap<Key, Value>`.

10. **LSP incremental re-parse** — For large inventories, re-parsing
    everything on every keystroke is too slow. Options: (a) full re-parse
    with 500ms debounce, (b) incremental re-parse of only changed files,
    (c) background re-merge thread. Start with (a), evolve to (b).

11. **LSP source locations** — The parser currently discards file/line info.
    Adding `SourceLocation { file: PathBuf, line: usize, col: usize }` to
    `Class`, `Node`, and key `Value` entries is needed for go-to-definition
    and better error messages. How much of the parser needs to change?