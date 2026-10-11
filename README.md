# http-trax

A desktop API client in the spirit of Postman, built around git. Your collections are plain YAML files in a repository, your team syncs through branches and pull requests, and AI agents can create and run API tests through MCP.

Built with Rust and Tauri 2 (backend and desktop shell) and Next.js (UI). Runs on Linux and Windows.

The app, installers (`Courion`), binaries (`courion`, `courion-mcp`, `courion-cli`, plus `http-trax` / `trax-mcp` / `trax` aliases), MCP server (`courion`), and script API (`courion.*` and `trax.*`) all support the **Courion** name while preserving full backwards compatibility with `http-trax` / `trax`.

![Request editor](docs/screenshots/request.png)

## Features

- **Workspaces for every team.** A workspace is a folder you keep in git: its own repo for a team's shared collections, or `api-tests/` inside a service repo. Switch between as many as you need; the switcher groups them by team. Create one, open an existing folder, or clone a team repository. Large workspaces use an in-memory index with instant invalidation.
- **Bookmarks, scope, and drag-and-drop.** Star a collection to pin it to the top. Use the crosshair to scope the sidebar to one collection and hide everything else. Drag and drop requests, folders and collections to reorder or move them across folders.
- **Full git workflow.** Stage, commit, fetch, pull (merge, rebase or fast-forward only), push, branches, merge with conflict resolution, history, and a pull-request style **review** of any branch (commits, changed requests, diff, conflict check) with a one-click link to open the PR on GitHub, GitLab, Bitbucket or Azure DevOps.
- **Always online.** A background worker fetches every workspace and fast-forwards when it's safe (no local commits, no uncommitted edits), so teammates' changes simply appear, with a note of who changed which requests. It never merges or touches your work.
- **Merges that understand requests.** When both sides edit the same request, environment or collection, Courion merges field by field: different headers, params or tests are combined automatically, and only a field both sides changed is left for you to pick in the conflict editor (or edit the merged file by hand).
- **Pull requests without leaving the app.** Create, list, approve and merge GitHub pull requests and GitLab merge requests, and leave review comments pinned to a request field such as `headers.Authorization` or the tests; open comments also show on the request. Stash, cherry-pick, revert, tags, sparse checkout for monorepos, and commit signing (GPG, SSH, X.509) with a Verified badge in history.
- **Onboarding in one step.** Start from a template (REST CRUD, GraphQL, OAuth 2.0 service, webhooks), or share an onboarding link: a teammate pastes it, and Courion clones the repository, opens the workspace and sets up the environment, asking only for its secrets.
- **Scripts.** JavaScript pre-request and post-response scripts at request, folder and collection level, with tests and assertions (`courion.test` / `trax.test`, `courion.expect` / `trax.expect`), variable chaining, and a Postman-compatible `pm.*` subset. Scripts run in a sandbox with no file or network access.
- **Saved responses.** Keep known-good responses as examples on a request. They're committed with the workspace.
- **Readable reviews.** Source control shows what changed in a request or environment ("URL changed", "Header added", "Test added") instead of raw YAML, with the line diff one click away.
- **Fast to drive from the keyboard.** Configurable keyboard shortcuts: Ctrl+K searches every request and command (Ctrl+. for actions), Ctrl+N creates a request, and a pasted cURL command becomes a request. Search recent runs under History and restore any history entry as a saved request. HTML responses can be previewed safely.
- **Complete auth & TLS.** OAuth 2.0 (authorization code with PKCE, client credentials, device code, password grants, `private_key_jwt` client assertions, and RFC 7662 token introspection), Bearer, Basic, Digest (MD5/SHA-256), AWS Signature v4, NTLM, and API keys. Per-request HTTP/HTTPS/SOCKS5 proxy, custom CA certificates, and client certificates (mTLS).
- **Everyday tools.** `{{variable}}` autocomplete with the resolved value on hover; copy any request as cURL or as fetch, axios, Python, Go or Rust code; search, filter (JSONPath & JMESPath), wrap, save responses, and virtualized rendering for multi-megabyte responses; plus a per-workspace cookie jar with a cookie manager.
- **Environments done right for git.** Shared values are committed; values your scripts set (like fresh tokens) stay on your machine, and secrets go to the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service). A secret can also be a vault reference the whole team shares (`op://` 1Password, `vault://` HashiCorp Vault, `azkv://` Azure Key Vault), read when the request is sent. Keep a whole environment off git with one switch, and mark production as protected so nothing is sent without a confirmation.
- **MCP server for AI agents.** `courion-mcp` (`trax-mcp`) ships with the app. After an agent builds or changes a service, it can register the endpoints, attach tests, run them and read the failures. What it creates shows up in the app immediately, as files you review in git. You decide what agents may do (edit requests, run them, read responses, edit environments, see secrets), see which clients are connected, and what they changed.
- **Runs in CI.** The `trax` CLI runs collections in a pipeline with the same runner as the app, exits non-zero when a test fails, and writes JUnit, JSON and Markdown reports. Secrets come from the pipeline's environment and are masked in the output.
- **Import from anywhere.** Import OpenAPI 3.x / Swagger specs (YAML or JSON), Postman collections (v2.0/v2.1) and environments with example responses, HAR archives, Insomnia exports, and Bruno collections; `pm.*` scripts keep working.

![Source control](docs/screenshots/source-control.png)

![AI agents](docs/screenshots/agents.png)

## Install

Download the latest installers from the [releases page](https://github.com/aitoncumbi/http-trax/releases): `.deb`, `.rpm` and `.AppImage` for Linux, `.msi` and a setup `.exe` for Windows, plus standalone `trax-mcp` (MCP server) and `trax` (CLI) binaries. Pushing a version tag (`v0.2.0`) builds and publishes a new release automatically; the installers appear on it about 20 minutes later, once the builds finish. Every CI run also uploads the installers as build artifacts.

Courion uses your installed `git` for all repository operations, so your existing credentials (SSH keys, Git Credential Manager, `gh auth`) just work.

## Connect an AI agent (MCP)

Open the **AI agents** panel in the app for ready-to-paste configs with the right path. For Claude Code:

```sh
claude mcp add courion -- courion-mcp --workspace ./api-tests
```

For clients that use an `mcp.json`:

```json
{
  "mcpServers": {
    "courion": { "command": "courion-mcp", "args": ["--workspace", "./api-tests"] }
  }
}
```

Leave out `--workspace` to use whichever workspace is active in the app. The agent also uses the environment you've selected there.

| Tool | What it does |
| --- | --- |
| `init_workspace` | Create or open a workspace (e.g. in the service repo) and register it |
| `upsert_environment` | Create/update an environment, e.g. `baseUrl=http://localhost:8080` |
| `upsert_collection` | Collection-level variables, headers, auth and scripts |
| `upsert_request` | Add or update a request, including test scripts |
| `move_item` | Move or reorder a request, folder, or collection |
| `import_spec` | Import an OpenAPI, Postman, HAR, Insomnia, or Bruno spec/export into the workspace |
| `run_request` / `run_collection` | Run and report status, tests, console output and body |
| `send_http_request` | One-off request that isn't saved |
| `list_history` / `restore_history_entry` | Search recent run history or restore a history run as a saved request |
| `list_workspace_templates` / `create_onboarding_link` | Starter templates (also `init_workspace` with `template` or `onboarding_link`) and links for teammates |
| `list_pull_requests`, `create_pull_request`, `approve_pull_request`, `merge_pull_request` | Pull requests on GitHub, merge requests on GitLab |
| `list_review_comments`, `add_review_comment`, `resolve_review_comment` | Review comments pinned to a request field |
| `semantic_merge_file` | Field-level merge preview (and resolution) of a conflicted request, environment or collection |
| `get_workspace`, `get_request`, `list_environments`, `list_workspaces`, `list_saved_responses`, `delete_request`, `list_cookies`, `clear_cookies` | Read and tidy up |

## Run collections in CI

`trax` (also installed as `courion-cli`) runs collections from a terminal or a pipeline, using the same runner, scripts and auth as the app:

```sh
trax list ./api-tests                                   # collections, folders, environments
trax run ./api-tests -e Local                           # every collection
trax run ./api-tests -e Local -c Users -c "Orders/Admin" --bail
```

It exits with `0` when every request passed, `1` when a test, a script or a request failed, and `2` for setup errors (no workspace, unknown collection or environment). Secrets aren't committed, so pass them from the pipeline: `--secret apiKey=API_KEY` sets the `apiKey` variable from the `API_KEY` environment variable and masks its value everywhere in the output. Plain values go in with `--var baseUrl=http://localhost:8080`. Protected environments need `--allow-protected`. Variables set by scripts aren't kept, and runs aren't added to history, unless you pass `--persist`.

GitHub Actions, with a JUnit report and a summary on the run page:

```yaml
- name: API tests
  env:
    API_KEY: ${{ secrets.API_KEY }}
  run: |
    curl -sSL -o trax https://github.com/aitoncumbi/http-trax/releases/latest/download/trax-x86_64-unknown-linux-gnu
    chmod +x trax
    ./trax run api-tests -e CI --secret apiKey=API_KEY \
      --junit trax-report.xml --markdown "$GITHUB_STEP_SUMMARY"
```

`--json report.json` writes every request with its status, tests, script errors and console output. Reports leave out response headers and bodies, since they usually end up as build artifacts.

## Scripting in 30 seconds

```js
// Post-response script on "Create order"
trax.test("order created", () => {
  trax.expect(trax.response.status).toBe(201);
  trax.expect(trax.response.json()).toHaveProperty("id");
});

// Chain the next request: stored locally, not committed
trax.env.set("orderId", trax.response.json().id);
```

```js
// Pre-request script on a collection: runs before every request in it
trax.request.headers.set("X-Request-Id", trax.uuid());
if (!trax.env.get("token")) trax.skipRequest();
```

Postman-style scripts work for the common cases: `pm.test`, `pm.expect(x).to.equal(y)`, `pm.environment.set`, `pm.response.to.have.status(200)`.

## Workspace format

```
api-tests/
  trax.yaml                         # workspace name, team, variables
  collections/orders-api/
    collection.yaml                 # shared headers, auth, variables, scripts
    create-order.req.yaml           # one request per file
    admin/folder.yaml
    admin/refund-order.req.yaml
  environments/local.env.yaml
  responses/<request-id>/<id>.yaml  # saved responses
  .trax/                            # local only (git-ignored): secrets, current values
```

```yaml
# collections/orders-api/create-order.req.yaml
id: 6e0c3f0e-8d52-4c56-9a51-1d7f0d7a5c11
name: Create order
seq: 2
method: POST
url: '{{baseUrl}}/v1/orders'
body:
  mode: json
  text: |-
    {
      "customerId": "{{customerId}}"
    }
scripts:
  postResponse: |
    trax.test("order created", () => {
      trax.expect(trax.response.status).toBe(201);
    });
```

See [DESIGN.md](DESIGN.md) for why it looks like this.

## Development

Prerequisites:

- Rust (stable) and Node.js 22
- git
- Linux: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential`
- Windows: Microsoft C++ Build Tools and WebView2 (preinstalled on Windows 10/11)

```sh
npm install
npm run app:dev        # desktop app with hot reload (builds trax-mcp first)
npm run dev            # UI only, in the browser, against a mock backend
npm run app:build      # release build + installers in target/release/bundle

cargo test             # Rust tests (core, scripting, git, MCP, CLI)
npm run lint && npm run typecheck
```

Project layout:

| Path | |
| --- | --- |
| `crates/trax-core` | Data model, workspace storage, registry, variables, HTTP client, runner |
| `crates/trax-script` | JavaScript sandbox for scripts |
| `crates/trax-git` | Git integration and the background sync pass |
| `crates/trax-mcp` | MCP server (`trax-mcp` binary) |
| `crates/trax-cli` | Command-line runner for CI (`trax` binary) |
| `src-tauri` | Tauri app: commands, sync worker, file watcher |
| `src` | Next.js UI |
| `scripts/build-sidecar.mjs` | Builds `trax-mcp` for bundling with the app |

## Roadmap

MCP over HTTP to drive the live app, contract checks, mock servers and monitors, then more protocols (WebSocket, SSE, gRPC) and auto-updates. See [ROADMAP.md](ROADMAP.md).
