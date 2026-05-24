<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

---
name: opencode-configuration
description: Configure OpenCode agents, skills, commands, permissions, providers, and models. Covers opencode.json, markdown agent/command/skill files, provider setup, MCP servers, LSP, formatters, and all configuration options.
---

# OpenCode Configuration

This skill covers all aspects of configuring OpenCode: the `opencode.json` config file,
markdown-based agents/commands/skills, providers, models, permissions, LSP, MCP servers,
formatters, and more.

---

## File Locations

OpenCode loads configuration from multiple locations, merged in precedence order
(later overrides earlier):

| Priority | Location | Purpose |
|----------|----------|---------|
| 1 (lowest) | Remote `.well-known/opencode` | Organizational defaults |
| 2 | `~/.config/opencode/opencode.json` | Global config |
| 3 | `$OPENCODE_CONFIG` env var path | Custom config |
| 4 | `opencode.json` in project root | Project config |
| 5 | `.opencode/` directories | Agents, commands, skills, plugins |
| 6 | `$OPENCODE_CONFIG_CONTENT` env var | Runtime overrides |
| 7 (highest) | Managed preferences (macOS `.mobileconfig` / Linux `/etc/opencode/`) | Admin-enforced |

The `.opencode/` directory structure:

```
.opencode/
├── agents/          # Markdown agent definitions (e.g., build.md, plan.md)
├── commands/        # Markdown command definitions (e.g., test.md, commit.md)
├── skills/          # Skill directories (e.g., my-skill/SKILL.md)
├── plugins/         # Plugin files
└── package.json     # Plugin dependencies (for npm plugins)
```

Global equivalents:

```
~/.config/opencode/
├── opencode.json    # Global config
├── tui.json         # TUI-specific config
├── agents/          # Global agent definitions
├── commands/        # Global command definitions
├── skills/          # Global skill definitions
├── plugins/         # Global plugin files
└── themes/          # Custom themes
```

---

## opencode.json Schema

The root config file uses the schema at `https://opencode.ai/config.json`. It supports
both JSON and JSONC (JSON with comments).

### Top-Level Keys

| Key | Type | Description |
|-----|------|-------------|
| `$schema` | string | JSON schema URL: `https://opencode.ai/config.json` |
| `model` | string | Default model in `provider/model-id` format |
| `small_model` | string | Model for lightweight tasks (titles, summaries) |
| `default_agent` | string | Default primary agent (must be primary mode) |
| `agent` | object | Agent configurations (key = agent name) |
| `command` | object | Custom command configurations (key = command name) |
| `provider` | object | Custom provider configurations (key = provider name) |
| `permission` | object | Global permission rules |
| `instructions` | string[] | Instruction files/patterns to include |
| `lsp` | object/bool | LSP server configurations |
| `formatter` | object/bool | Formatter configurations |
| `mcp` | object | MCP server configurations |
| `plugin` | array | npm plugin packages |
| `tools` | object | Tool enable/disable (deprecated, use `permission`) |
| `share` | string | Sharing mode: `"manual"`, `"auto"`, `"disabled"` |
| `snapshot` | bool | Enable file change snapshots (default: true) |
| `autoupdate` | bool/string | Auto-update: `true`, `false`, or `"notify"` |
| `shell` | string | Default shell for terminal and bash tool |
| `server` | object | Server config for `opencode serve`/`opencode web` |
| `compaction` | object | Context compaction settings |
| `watcher` | object | File watcher ignore patterns |
| `experimental` | object | Experimental features |
| `disabled_providers` | string[] | Providers to disable |
| `enabled_providers` | string[] | Only these providers enabled |
| `tool_output` | object | Truncation thresholds: `max_lines`, `max_bytes` |
| `attachment` | object | Image attachment config |

### Variable Substitution

Use these in any string value:

- `{env:VARIABLE_NAME}` — Substitutes from environment variables
- `{file:path/to/file}` — Substitutes with file contents (relative to config or absolute)

```json
{
  "model": "{env:OPENCODE_MODEL}",
  "provider": {
    "anthropic": {
      "options": {
        "apiKey": "{env:ANTHROPIC_API_KEY}"
      }
    }
  }
}
```

---

## Agents

### Agent Types

| Mode | Description |
|------|-------------|
| `primary` | User-facing agents, switchable with Tab |
| `subagent` | Background specialists, invoked via `@` mention or Task tool |

### Built-in Agents

| Agent | Mode | Description |
|-------|------|-------------|
| `build` | primary | Default agent, all tools enabled |
| `plan` | primary | Restricted agent for planning/analysis |
| `general` | subagent | General-purpose, full tool access |
| `explore` | subagent | Read-only codebase exploration |
| `scout` | subagent | External docs/dependency research |
| `compaction` | primary (hidden) | Auto context compaction |
| `title` | primary (hidden) | Auto session title generation |
| `summary` | primary (hidden) | Auto session summaries |

### JSON Configuration (opencode.json)

```json
{
  "agent": {
    "my-agent": {
      "description": "What this agent does (shown in @ autocomplete)",
      "mode": "primary",
      "model": "ollama/glm-5.1:cloud",
      "temperature": 0.2,
      "top_p": 0.9,
      "steps": 50,
      "color": "#f59e0b",
      "hidden": false,
      "prompt": "{file:./prompts/my-agent.txt}",
      "permission": {
        "edit": "allow",
        "bash": { "*": "deny", "git status *": "allow" },
        "task": { "my-sub-agent": "allow" },
        "external_directory": { "~/Projects/**": "allow" }
      }
    }
  }
}
```

### Markdown Agent Configuration (.opencode/agents/)

Create a `.md` file in `.opencode/agents/` (project) or `~/.config/opencode/agents/` (global).
The filename becomes the agent name (e.g., `review.md` → `review` agent).

```markdown
---
description: Reviews code for quality and best practices
mode: subagent
model: anthropic/claude-sonnet-4-20250514
temperature: 0.1
color: "#4ade80"
hidden: false
permission:
  edit: deny
  bash:
    "*": ask
    "git diff": allow
    "git log*": allow
  webfetch: allow
  external_directory:
    "~/Projects/rust/**": allow
  task:
    analyze-python: allow
    analyze-rust: allow
---

You are a code reviewer. Focus on:
- Code quality and best practices
- Potential bugs and edge cases
- Security considerations
```

### Agent Options Reference

| Option | Type | Description |
|--------|------|-------------|
| `description` | string | **Required.** What the agent does (shown in @ menu) |
| `mode` | string | `primary`, `subagent`, or `all` |
| `model` | string | Model override in `provider/model-id` format |
| `variant` | string | Default model variant (when using agent's model) |
| `temperature` | number | Response randomness (0.0–1.0) |
| `top_p` | number | Response diversity (0.0–1.0) |
| `steps` | integer | Max agentic iterations before forced text response |
| `color` | string | Hex color (`#FF5733`) or theme color (`primary`, `secondary`, `accent`, `success`, `warning`, `error`, `info`) |
| `hidden` | bool | Hide from @ autocomplete (subagent only, default: false) |
| `disable` | bool | Disable the agent entirely |
| `prompt` | string | Custom system prompt (supports `{file:...}` references) |
| `permission` | object | Permission rules (see Permissions section) |

### Task Permissions

Control which subagents an agent can invoke with `permission.task`:

```json
{
  "agent": {
    "orchestrator": {
      "mode": "primary",
      "permission": {
        "task": {
          "*": "deny",
          "orchestrator-*": "allow",
          "code-reviewer": "ask"
        }
      }
    }
  }
}
```

Rules are evaluated in order — **last matching rule wins**.

---

## Skills

Skills are reusable instruction sets loaded on-demand via the `skill` tool.

### Skill Discovery Locations

OpenCode searches these paths for `SKILL.md` files:

| Location | Scope |
|----------|-------|
| `.opencode/skills/<name>/SKILL.md` | Project |
| `.agents/skills/<name>/SKILL.md` | Project (compatible) |
| `.claude/skills/<name>/SKILL.md` | Project (compatible) |
| `~/.config/opencode/skills/<name>/SKILL.md` | Global |
| `~/.agents/skills/<name>/SKILL.md` | Global (compatible) |
| `~/.claude/skills/<name>/SKILL.md` | Global (compatible) |

Additional skill paths can be configured in `opencode.json`:

```json
{
  "skills": {
    "paths": ["./custom-skills/", "../../shared-skills/"],
    "urls": ["https://example.com/.well-known/skills/"]
  }
}
```

### SKILL.md Format

Each skill lives in its own directory containing a `SKILL.md` file with YAML frontmatter:

```
.opencode/skills/my-skill/
└── SKILL.md
```

The frontmatter **must** include `name` and `description`:

```markdown
---
name: my-skill
description: Short description of what this skill does (1–1024 chars)
license: MIT
compatibility: opencode
metadata:
  audience: developers
  workflow: ci
---

## What I Do

Detailed instructions for the agent when this skill is loaded.

## When to Use Me

Explain when agents should choose this skill.
```

### Skill Name Validation

The `name` field must:

- Be 1–64 characters
- Be lowercase alphanumeric with single hyphen separators
- Not start or end with `-`
- Not contain consecutive `--`
- Match the directory name containing `SKILL.md`

Regex: `^[a-z0-9]+(-[a-z0-9]+)*$`

### Skill Permissions

Control which skills agents can load with `permission.skill`:

```json
{
  "permission": {
    "skill": {
      "*": "allow",
      "internal-*": "deny",
      "dangerous-*": "ask"
    }
  }
}
```

Override per agent:

```json
{
  "agent": {
    "build": {
      "permission": {
        "skill": { "rust-rpm-packaging": "allow" }
      }
    }
  }
}
```

Or in agent frontmatter:

```yaml
permission:
  skill:
    "documents-*": allow
```

### Disable Skill Tool

Per agent:

```json
{
  "agent": {
    "plan": {
      "tools": { "skill": false }
    }
  }
}
```

Or in markdown frontmatter:

```yaml
tools:
  skill: false
```

---

## Commands

Commands are custom prompts triggered with `/command-name` in the TUI.

### JSON Configuration (opencode.json)

```json
{
  "command": {
    "test": {
      "template": "Run the full test suite with coverage and fix failures.",
      "description": "Run tests with coverage",
      "agent": "build",
      "model": "anthropic/claude-haiku-4-5",
      "subtask": false
    },
    "commit": {
      "template": "Stage changes and create a commit with a descriptive message.",
      "description": "Commit changes",
      "agent": "build"
    }
  }
}
```

### Markdown Command Configuration (.opencode/commands/)

Create a `.md` file in `.opencode/commands/` (project) or `~/.config/opencode/commands/` (global).
The filename becomes the command name (e.g., `test.md` → `/test` command).

```markdown
---
description: Run tests with coverage
agent: build
model: anthropic/claude-haiku-4-5
---

Run the full test suite with coverage report and show any failures.
Focus on the failing tests and suggest fixes.
```

### Command Template Features

**Arguments** — Use `$ARGUMENTS` for all args, `$1`/`$2`/`$3` for positional args:

```markdown
Create a new React component named $ARGUMENTS with TypeScript support.
```

Usage: `/component Button`

**Shell output** — Use `` !`command` `` to inject bash output:

```markdown
Recent git commits:
!`git log --oneline -10`

Review these changes and suggest improvements.
```

**File references** — Use `@path/to/file` to include file contents:

```markdown
Review the component in @src/components/Button.tsx.
```

### Command Options

| Option | Type | Required | Description |
|--------|------|----------|-------------|
| `template` | string | Yes (JSON) | The prompt template |
| `description` | string | No | Shown in TUI command list |
| `agent` | string | No | Which agent to use (default: current) |
| `model` | string | No | Model override |
| `subtask` | bool | No | Force subagent invocation (default: false) |

---

## Permissions

Permission rules control what actions require approval. Each rule resolves to `"allow"`,
`"ask"`, or `"deny"`.

### Available Permission Keys

| Key | What it gates |
|-----|---------------|
| `read` | Reading files |
| `edit` | All file modifications (write, edit, patch) |
| `glob` | File globbing |
| `grep` | Content search |
| `bash` | Shell commands (matches parsed commands) |
| `task` | Subagent invocation (matches agent names) |
| `skill` | Skill loading (matches skill names) |
| `lsp` | LSP queries |
| `question` | Asking user questions |
| `webfetch` | URL fetching |
| `websearch` | Web search |
| `external_directory` | Access paths outside workspace |
| `todowrite` | Todo list tool |
| `doom_loop` | Recovery when agent appears stuck |
| `repo_clone` | Cloning repositories |
| `repo_overview` | Repository overview |

### Simple Permissions

```json
{
  "permission": {
    "*": "deny",
    "read": "allow",
    "glob": "allow",
    "bash": "ask"
  }
}
```

### Granular Permissions (Object Syntax)

For `read`, `edit`, `glob`, `grep`, `list`, `bash`, `task`, `skill`, `external_directory`,
and `lsp`, you can use pattern-based rules:

```json
{
  "permission": {
    "bash": {
      "*": "deny",
      "git status *": "allow",
      "cargo *": "allow",
      "rm *": "deny"
    },
    "edit": {
      "*": "deny",
      "src/**/*.rs": "allow"
    },
    "external_directory": {
      "~/Projects/rust/**": "allow"
    }
  }
}
```

Rules are evaluated in order — **last matching rule wins**. Put `*` catch-alls first,
specific rules after.

### Home Directory Expansion

Use `~` or `$HOME` at the start of patterns:

- `~/projects/*` → `/Users/username/projects/*`
- `$HOME/projects/*` → same expansion

### Per-Agent Permission Overrides

Agent permissions are merged with global config, with agent rules taking precedence:

```json
{
  "permission": {
    "bash": { "*": "ask" }
  },
  "agent": {
    "build": {
      "permission": {
        "bash": { "*": "allow" }
      }
    }
  }
}
```

---

## Providers

### Custom Provider Configuration

```json
{
  "provider": {
    "my-provider": {
      "name": "My Provider",
      "api": "openai",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "https://api.my-provider.com/v1",
        "apiKey": "{env:MY_PROVIDER_API_KEY}",
        "timeout": 300000,
        "chunkTimeout": 30000
      },
      "models": {
        "my-model": {
          "id": "my-model-v1",
          "name": "My Model V1",
          "limit": { "context": 128000, "output": 4096 },
          "cost": { "input": 0.01, "output": 0.03 },
          "modalities": { "input": ["text"], "output": ["text"] },
          "tool_call": true,
          "temperature": true
        }
      }
    },
    "ollama": {
      "name": "Ollama",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "http://127.0.0.1:11434/v1"
      }
    }
  }
}
```

### Provider Options

| Option | Type | Description |
|--------|------|-------------|
| `name` | string | Display name |
| `api` | string | API compatibility type |
| `npm` | string | npm package for the provider SDK |
| `id` | string | Provider identifier |
| `env` | string[] | Required environment variables |
| `whitelist` | string[] | Allowed model patterns |
| `blacklist` | string[] | Blocked model patterns |
| `options.apiKey` | string | API key (supports `{env:...}` and `{file:...}`) |
| `options.baseURL` | string | Base URL for API |
| `options.timeout` | int/false | Request timeout in ms (default: 300000, `false` to disable) |
| `options.chunkTimeout` | int | Timeout between stream chunks in ms |
| `options.setCacheKey` | bool | Always set a cache key |

### Model Definition in Provider

```json
{
  "models": {
    "model-name": {
      "id": "actual-model-id",
      "name": "Display Name",
      "family": "model-family",
      "release_date": "2025-01-01",
      "attachment": true,
      "reasoning": false,
      "temperature": true,
      "tool_call": true,
      "cost": { "input": 0.01, "output": 0.03 },
      "limit": { "context": 128000, "output": 4096 },
      "modalities": { "input": ["text", "image"], "output": ["text"] },
      "status": "active",
      "experimental": false,
      "variants": {
        "thinking": { "disabled": false }
      }
    }
  }
}
```

### Provider/Disable Selection

```json
{
  "enabled_providers": ["anthropic", "ollama"],
  "disabled_providers": ["gemini"]
}
```

`disabled_providers` takes priority over `enabled_providers`.

---

## LSP Servers

### Enable Built-in LSP

```json
{
  "lsp": true
}
```

### Custom LSP Configuration

```json
{
  "lsp": {
    "rust": {
      "command": ["rust-analyzer"],
      "env": { "RUST_LOG": "debug" }
    },
    "typescript": {
      "command": ["typescript-language-server", "--stdio"],
      "extensions": [".ts", ".tsx"]
    },
    "python": {
      "disabled": true
    }
  }
}
```

### LSP Options

| Option | Type | Description |
|--------|------|-------------|
| `command` | string[] | Command and arguments to start the server |
| `env` | object | Environment variables for the server |
| `extensions` | string[] | File extensions to associate |
| `disabled` | bool | Disable this LSP server |
| `initialization` | object | LSP initialization options |

---

## Formatters

### Enable Built-in Formatters

```json
{
  "formatter": true
}
```

### Custom Formatter Configuration

```json
{
  "formatter": {
    "prettier": { "disabled": true },
    "custom-rust": {
      "command": ["rustfmt", "--emit=stdout"],
      "extensions": [".rs"],
      "environment": { "RUST_LOG": "off" }
    }
  }
}
```

| Option | Type | Description |
|--------|------|-------------|
| `command` | string[] | Command and arguments (`$FILE` for target file) |
| `extensions` | string[] | File extensions to format |
| `environment` | object | Environment variables |
| `disabled` | bool | Disable this formatter |

---

## MCP Servers

### Local MCP Server

```json
{
  "mcp": {
    "my-server": {
      "type": "local",
      "command": ["npx", "-y", "my-mcp-server"],
      "environment": { "MY_API_KEY": "{env:MY_API_KEY}" },
      "enabled": true,
      "timeout": 5000
    }
  }
}
```

### Remote MCP Server

```json
{
  "mcp": {
    "my-remote": {
      "type": "remote",
      "url": "https://mcp.example.com/sse",
      "headers": { "Authorization": "Bearer {env:MY_TOKEN}" },
      "timeout": 5000
    }
  }
}
```

### MCP with OAuth

```json
{
  "mcp": {
    "github": {
      "type": "remote",
      "url": "https://github.com/mcp",
      "oauth": {
        "clientId": "my-client-id",
        "scope": "repo read:user"
      }
    }
  }
}
```

Disable OAuth auto-detection with `"oauth": false`.

### Disable an MCP Server

```json
{
  "mcp": {
    "my-server": { "enabled": false }
  }
}
```

---

## Compaction

```json
{
  "compaction": {
    "auto": true,
    "prune": true,
    "tail_turns": 2,
    "preserve_recent_tokens": 5000,
    "reserved": 10000
  }
}
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `auto` | bool | true | Auto-compact when context is full |
| `prune` | bool | true | Remove old tool outputs |
| `tail_turns` | integer | 2 | Recent turns to keep verbatim |
| `preserve_recent_tokens` | integer | — | Max tokens from recent turns to preserve |
| `reserved` | integer | — | Token buffer to avoid overflow during compaction |

---

## Server Configuration

```json
{
  "server": {
    "port": 4096,
    "hostname": "0.0.0.0",
    "mdns": true,
    "mdnsDomain": "myproject.local",
    "cors": ["http://localhost:5173"]
  }
}
```

---

## Plugins

```json
{
  "plugin": [
    "opencode-helicone-session",
    "@my-org/custom-plugin",
    ["@franlol/opencode-md-table-formatter", { "option": "value" }]
  ]
}
```

Plugins can be:
- A string (package name, uses latest)
- A tuple of `[package, options]` for configuration

---

## TUI Configuration (tui.json)

Separate TUI-specific settings in `tui.json`:

```json
{
  "$schema": "https://opencode.ai/tui.json",
  "theme": "tokyonight",
  "scroll_speed": 3,
  "scroll_acceleration": { "enabled": true },
  "diff_style": "auto",
  "mouse": true,
  "keybinds": {
    "command_list": "ctrl+p"
  }
}
```

Use `OPENCODE_TUI_CONFIG` env var for a custom TUI config path.

---

## Watcher Configuration

```json
{
  "watcher": {
    "ignore": ["node_modules/**", "dist/**", ".git/**", "target/**"]
  }
}
```

---

## Complete Example

This project's `opencode.json` demonstrates a realistic configuration:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "default_agent": "plan",
  "model": "ollama/glm-5.1:cloud",
  "instructions": ["AGENTS.md"],
  "agent": {
    "build": {
      "disable": false,
      "permission": { "*": "allow" }
    },
    "plan": {
      "disable": false
    }
  },
  "command": {
    "commit": {
      "description": "commit changes",
      "agent": "build",
      "model": "ollama/kimi-k2.6:cloud",
      "template": "Run the code quality tools, fix all issues, run rustfmt and commit"
    }
  },
  "lsp": {
    "rust": {
      "command": ["rust-analyzer"],
      "env": { "RUST_LOG": "debug" }
    }
  },
  "permission": {
    "*": "deny",
    "external_directory": { "~/local/opt/reclass/reclass-salt/**": "allow" },
    "glob": "allow",
    "read": "allow",
    "grep": "allow",
    "rg": "allow",
    "lsp": "allow",
    "question": "allow",
    "webfetch": "allow",
    "websearch": "allow",
    "todo": "allow",
    "bash": {
      "*": "deny",
      "wc -l *": "allow",
      "grep *": "allow",
      "rg *": "allow",
      "find *-exec*": "deny",
      "find *-execdir*": "deny",
      "find *-delete*": "deny",
      "find *": "allow"
    }
  },
  "plugin": ["@franlol/opencode-md-table-formatter@latest"],
  "provider": {
    "ollama": {
      "name": "Ollama",
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "http://127.0.0.1:11434/v1" }
    }
  },
  "tools": {
    "codesearch": true,
    "websearch": true
  }
}
```

---

## Quick Reference: Creating New Configuration Elements

### Adding an Agent

**Via markdown** (recommended for version control):

1. Create `.opencode/agents/my-agent.md`:

```markdown
---
description: What this agent does and when to use it
mode: subagent
temperature: 0.1
permission:
  edit: deny
  bash:
    "*": ask
    "git log*": allow
---

Your agent's system prompt goes here.
```

2. To allow other agents to delegate to it, add `task` permissions:

```json
{
  "agent": {
    "plan": {
      "permission": {
        "task": { "my-agent": "allow" }
      }
    }
  }
}
```

**Via JSON** (for simpler agents):

```json
{
  "agent": {
    "my-agent": {
      "description": "What this agent does",
      "mode": "subagent",
      "temperature": 0.1,
      "permission": { "edit": "deny" }
    }
  }
}
```

### Adding a Skill

1. Create `.opencode/skills/my-skill/SKILL.md`:

```markdown
---
name: my-skill
description: What this skill does and when to use it
---

Skill content goes here. Agents load this via the skill tool.
```

2. Ensure the directory name matches the `name` field: `my-skill/` → `name: my-skill`

### Adding a Command

**Via markdown** (recommended):

1. Create `.opencode/commands/my-command.md`:

```markdown
---
description: What this command does
agent: build
model: anthropic/claude-haiku-4-5
---

The prompt template with $ARGUMENTS substitution
and !`shell output` injection.
```

**Via JSON**:

```json
{
  "command": {
    "my-command": {
      "template": "The prompt template",
      "description": "What this command does",
      "agent": "build"
    }
  }
}
```

### Adding a Provider

```json
{
  "provider": {
    "my-provider": {
      "name": "My Provider",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "https://api.my-provider.com/v1",
        "apiKey": "{env:MY_PROVIDER_API_KEY}"
      }
    }
  }
}
```

### Adding an LSP Server

```json
{
  "lsp": {
    "python": {
      "command": ["pylsp"],
      "extensions": [".py", ".pyi"]
    }
  }
}
```

### Adding an MCP Server

```json
{
  "mcp": {
    "filesystem": {
      "type": "local",
      "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"],
      "enabled": true
    }
  }
}
```