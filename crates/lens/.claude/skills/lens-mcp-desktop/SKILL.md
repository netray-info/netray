---
name: lens-mcp-desktop
description: Install the lens MCP server and register it with Claude Desktop
user-invocable: true
---

Install the lens MCP server locally and register it with Claude Desktop so you can call `check_domain`, `check_domains`, and `lens_meta` tools directly from any conversation.

## Steps

### 1. Write server files

Create `~/.claude/mcp-servers/lens/` and write these two files using the Write tool. Expand `~` to the actual home directory path.

**`~/.claude/mcp-servers/lens/package.json`**:

```json
{
  "name": "lens-mcp",
  "version": "1.0.0",
  "type": "module",
  "description": "MCP server for lens.netray.info — unified domain health check",
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.0.0",
    "zod": "^3.0.0"
  }
}
```

**`~/.claude/mcp-servers/lens/server.mjs`**:

```javascript
#!/usr/bin/env node
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const BASE_URL = "https://lens.netray.info";

async function lensGet(path) {
  const res = await fetch(`${BASE_URL}${path}`, {
    headers: { Accept: "application/json" },
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(`${res.status}: ${err.error?.message ?? res.statusText}`);
  }
  return res.json();
}

const server = new McpServer({
  name: "lens",
  version: "1.0.0",
  description:
    "lens — unified domain health check (DNS, TLS, IP) at lens.netray.info",
});

server.tool(
  "check_domain",
  "Check DNS, TLS, and IP reputation for a single domain. Returns a structured health report with an A+–F grade, per-section scores, and hard-fail indicators.",
  { domain: z.string().describe("Domain name to check (e.g. example.com)") },
  async ({ domain }) => {
    const data = await lensGet(`/api/check/${encodeURIComponent(domain)}`);
    return { content: [{ type: "text", text: JSON.stringify(data, null, 2) }] };
  }
);

server.tool(
  "check_domains",
  "Check up to 10 domains sequentially. Returns an array of health reports, one per domain. Errors on individual domains are captured per-entry rather than failing the whole batch.",
  {
    domains: z
      .array(z.string())
      .min(1)
      .max(10)
      .describe("List of domain names to check"),
  },
  async ({ domains }) => {
    const results = [];
    for (const domain of domains) {
      try {
        const data = await lensGet(`/api/check/${encodeURIComponent(domain)}`);
        results.push({ domain, ...data });
      } catch (err) {
        results.push({ domain, error: err.message });
      }
    }
    return {
      content: [{ type: "text", text: JSON.stringify(results, null, 2) }],
    };
  }
);

server.tool(
  "lens_meta",
  "Get lens server metadata: version, configured backends, scoring profile name, and rate limit policy.",
  {},
  async () => {
    const data = await lensGet("/api/meta");
    return { content: [{ type: "text", text: JSON.stringify(data, null, 2) }] };
  }
);

await server.connect(new StdioServerTransport());
```

### 2. Install dependencies

Run this in the server directory (requires Node.js ≥ 18):

```sh
cd ~/.claude/mcp-servers/lens && npm install
```

If `npm` is not found or the Node version is below 18, stop and tell the user: "Node.js ≥ 18 is required. Install it from https://nodejs.org and re-run this skill."

### 3. Find the node binary path

Run:

```sh
which node
```

Store the output (e.g. `/usr/local/bin/node` or `/opt/homebrew/bin/node`). You need the absolute path because Claude Desktop does not inherit the user's PATH.

Also resolve the server path:

```sh
echo "$HOME/.claude/mcp-servers/lens/server.mjs"
```

### 4. Register with Claude Desktop

The config file is at `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS.

Read the file (create it if missing with `{}`), then add or update the `mcpServers.lens` entry:

```json
{
  "mcpServers": {
    "lens": {
      "command": "<absolute-node-path>",
      "args": ["<absolute-server-path>"]
    }
  }
}
```

Replace `<absolute-node-path>` with the output of `which node` and `<absolute-server-path>` with the expanded `$HOME` path from step 3. Merge with any existing `mcpServers` entries — do not overwrite them.

### 5. Confirm

Tell the user:

> The lens MCP server is installed. **Restart Claude Desktop** to activate it.
>
> Available tools: `check_domain`, `check_domains`, `lens_meta`
>
> Example: "Check the domain health of example.com"
