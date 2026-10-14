# http-trax roadmap

We're building this in phases. Each phase should leave the app shippable. Checked items are done; everything else is planned and open for discussion. Design decisions behind finished work are recorded in [DESIGN.md](DESIGN.md).

## Phase 0 — Foundation ✅ (this release)

**Platform and build**
- [x] Rust workspace: `trax-core`, `trax-script`, `trax-git`, `trax-mcp`, Tauri 2 app
- [x] Next.js (static export) + Tailwind UI, dark and light themes
- [x] GitHub Actions CI on Linux and Windows: lint, typecheck, clippy, tests, installers (.deb, .rpm, AppImage, .msi, NSIS)
- [x] Tag-triggered release workflow publishing installers and standalone `trax-mcp` binaries
- [x] Browser preview mode with a mock backend for UI work

**Workspaces and collections**
- [x] Git-friendly workspace format: YAML, one file per request, stable ids, readable file names
- [x] Multiple workspaces (one per team or service), switcher grouped by team
- [x] Create, open, or clone a team repository as a workspace
- [x] Collections and nested folders with shared headers, auth, variables and scripts
- [x] **Bookmark collections** so they stay pinned at the top
- [x] **Scope to a collection** to hide all the others
- [x] Environments with shared values, local "current values", and secrets kept out of git
- [x] Workspace, collection, environment and runtime variables with clear precedence; dynamic variables

**Requests and responses**
- [x] Methods, query params, headers, bodies (JSON, text, XML, form URL-encoded, multipart with files, GraphQL)
- [x] Auth: bearer, basic, API key (header/query), inherited from folders/collections
- [x] Per-request timeout, redirects and TLS verification
- [x] Response viewer: pretty/raw body, headers, timing, size, sent request, images
- [x] **Save responses** on a request (examples), view and delete them
- [x] Collection and folder runner with shared variables and a report

**Scripting**
- [x] **Pre-request and post-response JavaScript** in a sandbox (Boa), at collection, folder and request level
- [x] `trax` API: request mutation, env/vars, tests, Jest- and Chai-style `expect`, console, `skipRequest`, top-level `await`
- [x] Postman-compatible `pm.*` subset
- [x] Snippet library in the editor

**Git and teams**
- [x] Status, stage/unstage, discard, commit
- [x] Fetch, pull (merge / rebase / fast-forward only), push with upstream setup
- [x] Branches: create, switch (including remote branches), delete
- [x] Merge with conflict detection; resolve with ours/theirs/manual, then continue or abort
- [x] **Review**: branch compare with commits, changed requests, diff, and a dry-run conflict check
- [x] "Open pull request" links for GitHub, GitLab, Bitbucket, Azure DevOps (and GitHub-compatible hosts)
- [x] History with commit diffs
- [x] Per-repository git identity and remote management
- [x] **Background worker** that fetches and safely fast-forwards every workspace, with live sync status
- [x] File watcher: changes from agents, teammates or editors appear immediately; conflict banner for unsaved tabs

**AI integration**
- [x] **MCP server** (`trax-mcp`, stdio) bundled with the app
- [x] Tools to create workspaces, environments, collections and requests with tests, run requests and collections, send ad-hoc requests, and read saved responses
- [x] In-app setup panel with configs for Claude Code, Claude Desktop/Cursor and VS Code

## Phase 0.2 — Courion interface ✅

- [x] New interface design: frameless window with an app-drawn toolbar, icon rail, quieter tree with pinned section, bordered parameter tables with descriptions, `{{variable}}` highlighting in inputs
- [x] Response below or beside the request, remembered per machine
- [x] Workspace switcher with team avatars and Ctrl+1–9
- [x] Command palette (Ctrl+K): requests, recent runs, commands, environment and workspace switching
- [x] Import a request from a cURL command
- [x] Local run history (app and MCP runs), grouped by day
- [x] Full-width source control view with semantic "what changed" diffs for requests and environments, raw diffs with line numbers, commit history and a searchable branch menu

## Phase 0.3 — Courion interface, second pass ✅

- [x] Inset-card layout: 32px title strip, rail, sidebar with workspace switcher, search and environment picker
- [x] Environments view: Sync with Git switch (shared or this machine only) and protected environments with confirmation
- [x] AI agents view: client setup, connected clients, server details, agent permissions enforced by `trax-mcp`, recent agent activity
- [x] Source control split between sidebar (changes, commit) and main card (what changed, history)
- [x] Import Postman collections (v2.0/v2.1) and environments
- [x] Sandboxed HTML response preview
- [x] Palette actions (Ctrl+.)
- [x] Workspace switcher with team size and request counts

## Phase 1 — Daily-driver polish ✅

- [x] Import: Insomnia, Bruno, OpenAPI 3.x, HAR (cURL, Postman and Postman's OAuth 2.0 settings are done); Postman example responses; `import_spec` MCP tool
- [x] Export: copy as cURL and code snippets (cURL, fetch, axios, Python requests, Go, Rust), with or without variables filled in
- [x] Search history and "restore as request"
- [x] Cookie jar per workspace (redirect-aware, kept in `.trax/cookies.json`), cookie manager, response Cookies tab, per-request opt-out, MCP `list_cookies` / `clear_cookies`
- [x] Drag and drop to reorder and move requests and folders
- [x] Variable autocomplete and hover-to-preview resolved values in the URL bar, key-value tables and body editors
- [x] Response tools: search, JSONPath filter, word wrap toggle, save body to file; very large bodies open as plain text
- [x] JMESPath filter, and virtualized rendering for very large responses
- [x] Configurable keyboard shortcuts
- [x] Finish the Courion rename: installers, binaries, MCP server name, and a `courion.*` alias for the script API
- [x] Proxy settings, custom CA certificates, client certificates (mTLS)
- [x] OAuth 2.0: authorization code with PKCE (loopback redirect), client credentials, device code and password grants; tokens cached locally, refreshed and fetched automatically when no sign-in is needed; works for agents through MCP
- [x] More auth: Digest, AWS SigV4, NTLM; OAuth 2.0 client assertions (private_key_jwt) and token introspection
- [x] Tighten the webview CSP
- [x] In-memory workspace index for very large workspaces

## Phase 2 — Teams and git, deeper

- [x] Semantic 3-way merge for request files (field-level merge of headers, params, scripts) with an in-app conflict editor; also environments, collections and folders; MCP `semantic_merge_file`
- [x] Pull requests through the GitHub and GitLab APIs: create, list, review comments on requests, approve, merge (PRs tab in Source control, PR actions in the review tab); MCP tools for each
- [x] Inline review comments mapped to requests and fields (`headers.Authorization`, `scripts.postResponse`…), anchored to the PR's head commit on GitHub and shown on the request itself
- [x] Secrets in the OS keychain (Windows Credential Manager, macOS Keychain, Secret Service/libsecret via `secret-tool`) and pluggable vaults (1Password `op://`, HashiCorp Vault `vault://`, Azure Key Vault `azkv://`) referenced from shared environments
- [x] Stash, cherry-pick, revert and tag support
- [x] Notifications when teammates' changes arrive (with a summary of changed requests)
- [x] Workspace templates and team onboarding links (clone + environment setup in one step); MCP `list_workspace_templates`, `create_onboarding_link`, `init_workspace` with a template or link
- [x] Sparse checkout for workspaces inside large monorepos (on clone, onboarding, and in Settings)
- [x] Commit signing status and per-team signing settings (GPG, SSH, X.509; per repository)

## Phase 3 — Automation and AI

- [x] `trax` CLI (`courion-cli`): run collections in CI with JUnit, JSON and Markdown reports and a non-zero exit on failures; secrets from the environment, masked in output
- [ ] MCP over Streamable HTTP served by the running app, so agents can drive the live UI (open a request, show a result)
- [ ] MCP resources (collections, requests, environments) and prompts ("write tests for this endpoint")
- [x] Generate requests and tests from an OpenAPI spec: a status test per request, contract checks against the spec, ids passed from POSTs to the `/{id}` requests; `import_spec` with `generate_tests` for agents (in place of a separate `import_openapi` tool)
- [x] Contract checks: validate responses against OpenAPI 3.x / Swagger 2.0 (path, method, status, content type, required headers, body schema) or a JSON Schema file, set per collection, folder or request and reported as a test
- [x] Data-driven runs (CSV/JSON iterations, `trax.iterationData` / `pm.iterationData`) and run delays, in the app's runner, the CLI (`--data`, `--iterations`, `--delay`) and MCP `run_collection`
- [ ] Mock servers that serve saved responses
- [ ] Scheduled monitors with alerts
- [ ] Script APIs: `trax.sendRequest`, crypto (hashes, HMAC), a vetted `require` set (lodash, dayjs, ajv), and a debugger

## Phase 4 — Protocols and platform

- [ ] WebSocket, Server-Sent Events, gRPC (with reflection), GraphQL schema introspection and autocomplete
- [ ] macOS builds with notarisation
- [ ] Code signing for Windows installers
- [ ] Auto-updates (`tauri-plugin-updater`)
- [ ] Plugin system for auth providers, importers and script libraries
- [ ] Inline function calls for headers / body (Instead of setting body variables in scripts we can call a function such as uuid.v4 directly on the body to generate a UUID)

## How we work through this

- Small pull requests; each one keeps CI green on Linux and Windows.
- Changes to the on-disk format bump `FORMAT_VERSION` and ship a migration.
- New features that agents could use get an MCP tool in the same phase.
