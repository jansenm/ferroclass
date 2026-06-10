<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Interface Design: TUI, Web UI, and MCP Server

This document brainstorms the features each interface should expose
and the library API changes needed to support them.

## Common Requirements

All three interfaces share these needs:

| Need                          | Current Gap                                    | API Change Required                    |
|-------------------------------|------------------------------------------------|---------------------------------------|
| Load inventory once, query many times | Adapters re-load from disk on every call  | Decouple adapters from loading        |
| Search and filter nodes       | Only `get_node(name)` and `node_names()`      | Query API with indexing               |
| Thread-safe inventory handle  | `Rc<Value>` is `!Send + !Sync`                | Switch to `Arc<Value>`               |
| Progress reporting during load| No callback or event system                    | Tracing spans or callback hook        |
| Unified error handling        | Fragmented error types                         | Single `ferroclass::Error`            |

---

## TUI (Terminal User Interface)

A TUI lets operators explore inventory data interactively — browse nodes,
inspect classes, trace merge order, debug interpolation — without writing
commands or scripts.

### Target Users

- Operations engineers debugging why a node has certain parameters
- Developers checking class inheritance chains
- Anyone wanting quick inventory exploration without remembering CLI flags

### Core Features

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

#### Merge Inspector

- For a given node, show the resolution order of classes
- Highlight which class "wins" for a given parameter key
- Show interpolation references (`${...}`) before and after resolution
- Flag circular references or missing references

#### Configuration View

- Show current `Options` (storage type, inventory base URI, merge config)
- Show class mappings (glob/regex patterns and their target classes)
- Allow toggling `automatic_parameters`, `class_mappings_match_path`

#### Search

- Full-text search across all node parameters
- Find nodes where a specific key is set (or unset)
- Find nodes matching a parameter value pattern
- Regex and glob support

### API Requirements

| TUI Feature                    | Library API Needed                                          |
|--------------------------------|-------------------------------------------------------------|
| Node list with filters         | `find_nodes(pattern)`, `find_nodes_by_class(name)`, `find_nodes_by_environment(env)` |
| Merged node parameters         | `inventory.merge_node(name)` (already exists)               |
| Merge order / provenance        | `inventory.merge_node_with_provenance(name)` → returns each key with its source class |
| Class list and lookup           | `inventory.class_names()`, `inventory.get_class(name)` (already exists) |
| Class inverse lookup            | `find_nodes_by_class(name)`                                  |
| Unresolved interpolation view   | Access to `Value::Reference`/`Value::StringWithReference` before interpolation |
| Circular reference detection    | Already detected during merge; expose as structured data     |
| Full-text parameter search      | `search_parameters(query)` returning node names + matching keys |
| Reload inventory from disk      | `inventory.reload()` or `load()` again (needs decoupled adapters) |

### Interaction Model

The TUI runs an event loop (e.g., ratatui/crossterm). Inventory is loaded
once at startup (or on user command), then queried repeatedly:

```
┌─ Node List ─────────────────┐ ┌─ Node Detail: web01 ─────────────┐
│ ▸ web01                      │ │ Classes: [default, web, apache]  │
│   web02                      │ │ Applications: [apache]           │
│   db01                       │ │ Parameters:                      │
│ ▸ db02                       │ │   hostname: web01               │
│   monitor01                  │ │   role: webserver               │
│                              │ │   apache::                       │
│ [Search: role=web]           │ │     port: 8080                  │
└──────────────────────────────┘ │     vhosts: [...]               │
                                  │ [Merge Order] [Raw] [Search]   │
                                  └──────────────────────────────────┘
```

### Dependencies

- `ratatui` + `crossterm` for rendering
- Thread-safe inventory handle (Phase 1)
- Query API (Phase 2)
- Merge provenance tracking (new)

---

## Web UI

A web application providing the same exploration capabilities as the TUI
but in a browser, with the added benefit of URL-shareable views and
multi-user access.

### Target Users

- Teams sharing inventory exploration
- Operators who prefer browser over terminal
- CI/CD pipelines exposing inventory state as a service

### Core Features

All TUI features, plus:

#### REST API

The web UI is backed by a JSON API that can also be used programmatically:

| Endpoint                                    | Description                                    |
|---------------------------------------------|------------------------------------------------|
| `GET /api/v1/nodes`                         | List all nodes (with optional filters)          |
| `GET /api/v1/nodes/:name`                   | Get merged node detail                         |
| `GET /api/v1/nodes/:name/parameters`        | Get merged parameters only                      |
| `GET /api/v1/nodes/:name/classes`           | Get resolved class list                         |
| `GET /api/v1/nodes/:name/merge-order`       | Get merge provenance (which class contributed what) |
| `GET /api/v1/nodes/:name/raw`               | Get node before interpolation/merge             |
| `GET /api/v1/classes`                        | List all classes                                |
| `GET /api/v1/classes/:name`                  | Get class detail                                |
| `GET /api/v1/classes/:name/usage`            | Which nodes use this class                      |
| `GET /api/v1/search?q=pattern`               | Full-text search across nodes and parameters    |
| `GET /api/v1/inventory/config`               | Current inventory configuration                 |
| `POST /api/v1/inventory/reload`              | Reload inventory from disk                       |
| `GET /api/v1/formats/ansible?node=name`      | Ansible-formatted output                        |
| `GET /api/v1/formats/salt?node=name`         | Salt-formatted output                            |

#### WebSocket Updates

- Notify connected clients when inventory files change on disk (via inotify/polling)
- Push incremental merge results as they complete
- Show merge progress bar for large inventories

#### Multi-User Concerns

- Read-only by default (inventory is files on disk, not edited through the API)
- Optional write endpoint for creating/modifying node and class YAML files (future scope)
- Authentication via API key or HTTP basic auth (simple)

### API Requirements

Same as TUI, plus:

| Web Feature                     | Library API Needed                                          |
|---------------------------------|-------------------------------------------------------------|
| JSON serialization of all types | `Serialize` already exists for most types; verify completeness |
| Streaming merge progress        | Callback or channel-based progress reporting                |
| File watch + reload             | `inventory.reload()` or `load()` again; decouple from adapter |
| Concurrent read access          | `Arc<Inventory>` (Phase 1: Send + Sync)                     |
| Ansible/Salt format endpoints   | Decoupled adapter functions (Phase 0)                        |

### Technology Choices

- **Backend**: `axum` or `actix-web` — both async, well-maintained
- **Frontend**: Could be server-rendered (askama/templates) or SPA (React/Vue)
  - Start with server-rendered HTML for simplicity
  - Add JSON API endpoints for programmatic access from day one
- **File watching**: `notify` crate for inotify/FSEvents
- **Concurrency**: `Arc<Inventory>` behind `RwLock` for read-heavy workloads

---

## MCP (Model Context Protocol) Server

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
| `ferroclass://nodes/{name}/raw`        | Node before merge/interpolation           |
| `ferroclass://classes`                  | All class names                           |
| `ferroclass://classes/{name}`           | Class detail                              |
| `ferroclass://config`                   | Inventory configuration                   |

#### MCP Prompts

Pre-configured prompt templates:

| Prompt Name                    | Description                                              |
|--------------------------------|----------------------------------------------------------|
| `debug_node`                   | "Investigate why node X has parameter Y set to Z"        |
| `compare_nodes`                | "Compare the parameters of node A and node B"           |
| `trace_class`                  | "Show the full class inheritance chain for class X"      |
| `find_unused_classes`          | "Find classes not included by any node"                 |

### API Requirements

Same as TUI, plus:

| MCP Feature                     | Library API Needed                                          |
|----------------------------------|-------------------------------------------------------------|
| JSON-serializable all types      | `Serialize` on `Node`, `Class`, `Inventory`, `Value`       |
| Provenance tracking              | `merge_node_with_provenance(name)` → key → source class    |
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

### Should Have (for TUI and MCP)

| Change                                   | Interface | Effort | Impact                     |
|------------------------------------------|-----------|--------|----------------------------|
| Merge provenance tracking                | TUI, MCP  | High   | "Which class set this key" |
| Interpolation source tracing             | TUI, MCP  | Medium | Debug reference chains      |
| `explain_merge()` human-readable output  | MCP       | Low    | Agent-friendly explanations |
| Lazy/incremental inventory loading       | Web, MCP  | High   | Fast startup for large inv |

### Nice to Have

| Change                                   | Interface | Effort | Impact                     |
|------------------------------------------|-----------|--------|----------------------------|
| File watching + auto-reload              | Web, TUI  | Medium | Live updates               |
| Write endpoints (create/modify nodes)    | Web       | Very High | Full lifecycle management |
| Requirements/prerequisites validation    | All       | High   | Policy enforcement         |
| Early node values during merge           | All       | Very High | Complex merge strategy    |

---

## Open Questions

1. **Merge provenance** — Should provenance be optional (behind a flag) to
   avoid overhead for callers that don't need it? The merge algorithm would
   need to track source class for each key-value pair.

2. **Interpolation before/after** — Should we expose both the unresolved
   (references visible) and resolved (interpolated) views of a node?
   Currently `merge_node()` always interpolates. We'd need a
   `merge_node_raw()` that skips interpolation.

3. **Incremental loading** — For large inventories, should we support loading
   nodes on demand (lazy) rather than all at once? This would require
   restructuring `Inventory::load()` significantly.

4. **Web UI: read-only or read-write?** — A read-only web UI is
   straightforward. A read-write UI needs YAML file generation, validation,
   and conflict resolution. Start read-only.

5. **MCP: local or remote?** — Local (stdio) is simpler and matches how
   agents currently use MCP. Remote (HTTP/SSE) enables shared instances.
   Start local.

6. **Authentication** — The web UI needs at least API key auth. The MCP
   server (stdio) inherits the agent's local access. How much auth
   infrastructure do we build?

7. **Binary or library?** — Should the TUI, web server, and MCP server be
   separate binaries (`ferroclass-tui`, `ferroclass-web`, `ferroclass-mcp`)
   or subcommands of the `reclass` binary? Separate binaries align with
   the architecture rule that binaries are thin wrappers over the library.