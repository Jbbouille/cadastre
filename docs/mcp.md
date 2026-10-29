# Cadastre for AI agents (MCP)

`cadastre mcp [directory]` is a [Model Context Protocol](https://modelcontextprotocol.io) server: an AI agent (Claude Code, Claude Desktop, VS Code, Cursor…) explores a real project and writes views and checks against it, instead of guessing kinds, keys and names.

- **Read only.** The server never writes: the agent writes its `.cav` (views) and `.cac` (checks) files with its own tools, after trying them.
- **Always up to date.** The project is read again at each call, as `cadastre check` reads it (its `cadastre.toml`, its modules, its CSV files): what the agent just wrote is taken into account.
- **The syntax of the version installed.** The reference, the cheat sheet for views and the tutorial are resources of the server (`cadastre://reference`, `cadastre://views`, `cadastre://tutorial`).

## Tools

| Tool | Gives |
|---|---|
| `list_kinds` | the kinds (built in, and of the project with what each is like), their parents, the groups |
| `list_keys(kind)` | the keys of a kind: type, cardinality, closed values; the keys of every resource; the free keys written in the project |
| `search_resources(query, kind?, limit?)` | resources by name, qualified name or label, best first (exact, prefix, substring, close spelling) |
| `get_resource(name)` | a resource: its keys, references, content, what runs on it, its incoming and outgoing links |
| `list_views` | the views and checks of the project |
| `check_view(text)` | the diagnostics of a proposed view or check (a block, or the clauses of a view alone), compiled with the project |
| `eval_view(name \| text, format?)` | what a view selects: nodes, frames, edges; with a format, its rendering (`text`, `mermaid`, `markdown`…) |
| `run_checks(name?)` | the checks: passed or failed, what fails them |

Results are capped at 200 nodes, edges or links, with their totals, so that a large project does not fill the context of the agent. The records are those of the [JSON model](api.md), the same as the export and the API of the site.

## Configuring a client

The `cadastre` executable must be installed (the releases, or `cargo install cadastre`).

**Claude Code**, in the directory of the project:

```bash
claude mcp add cadastre -- cadastre mcp .
```

**Claude Desktop, Cursor, VS Code** and the other clients take the same command in their configuration file (`claude_desktop_config.json`, `.cursor/mcp.json`, `.vscode/mcp.json`…):

```json
{
  "mcpServers": {
    "cadastre": { "command": "cadastre", "args": ["mcp", "/path/to/the/project"] }
  }
}
```

## A session

1. The agent reads `cadastre://views` and calls `list_kinds`, then `search_resources("billing")` to find the names it needs.
2. It proposes `select app, database where ancestor(system) = billing group-by system` to `check_view`: an unknown key comes back with the closest one.
3. `eval_view` with `format: "text"` shows what the view draws; the agent adjusts it, then writes it into `src/views/billing.cav`.
4. For a rule, it proposes a `check` the same way, writes it into `src/checks/<theme>.cac`, and `run_checks` tells whether the project meets it.
