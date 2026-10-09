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

## Phase 1 — Daily-driver polish

- [ ] Import: Postman collection v2.1 and environments, Insomnia, Bruno, OpenAPI 3.x, HAR, cURL
- [ ] Export: cURL and code snippets (fetch, axios, Python requests, Go, Rust)
- [ ] Request history (local, searchable) with "restore as request"
- [ ] Cookie jar per workspace, with an editor
- [ ] Drag and drop to reorder and move requests and folders
- [ ] Variable autocomplete and hover-to-preview resolved values in the URL bar and editors
- [ ] Response tools: search, JSONPath/JMESPath filter, word wrap toggle, save body to file, large-response virtualization
- [ ] Command palette and configurable keyboard shortcuts
- [ ] Proxy settings, custom CA certificates, client certificates (mTLS)
- [ ] More auth: OAuth 2.0 (authorization code + PKCE, client credentials, device code) with token refresh, Digest, AWS SigV4, NTLM
- [ ] Tighten the webview CSP
- [ ] In-memory workspace index for very large workspaces

## Phase 2 — Teams and git, deeper

- [ ] Semantic 3-way merge for request files (field-level merge of headers, params, scripts) with an in-app conflict editor
- [ ] Pull requests through the GitHub and GitLab APIs: create, list, review comments on requests, approve, merge
- [ ] Inline review comments mapped to requests and fields
- [ ] Secrets in the OS keychain (Windows Credential Manager, Secret Service/libsecret) and pluggable vaults (1Password, HashiCorp Vault, Azure Key Vault)
- [ ] Stash, cherry-pick, revert and tag support
- [ ] Notifications when teammates' changes arrive (with a summary of changed requests)
- [ ] Workspace templates and team onboarding links (clone + environment setup in one step)
- [ ] Sparse checkout for workspaces inside large monorepos
- [ ] Commit signing status and per-team signing settings

## Phase 3 — Automation and AI

- [ ] `trax` CLI: run collections in CI with JUnit, JSON and Markdown reports and a non-zero exit on failures
- [ ] MCP over Streamable HTTP served by the running app, so agents can drive the live UI (open a request, show a result)
- [ ] MCP resources (collections, requests, environments) and prompts ("write tests for this endpoint")
- [ ] `import_openapi` MCP tool and "generate requests and tests from an OpenAPI spec"
- [ ] Contract checks: validate responses against OpenAPI/JSON Schema
- [ ] Data-driven runs (CSV/JSON iterations) and run delays
- [ ] Mock servers that serve saved responses
- [ ] Scheduled monitors with alerts
- [ ] Script APIs: `trax.sendRequest`, crypto (hashes, HMAC), a vetted `require` set (lodash, dayjs, ajv), and a debugger

## Phase 4 — Protocols and platform

- [ ] WebSocket, Server-Sent Events, gRPC (with reflection), GraphQL schema introspection and autocomplete
- [ ] macOS builds with notarisation
- [ ] Code signing for Windows installers
- [ ] Auto-updates (`tauri-plugin-updater`)
- [ ] Plugin system for auth providers, importers and script libraries

## How we work through this

- Small pull requests; each one keeps CI green on Linux and Windows.
- Changes to the on-disk format bump `FORMAT_VERSION` and ship a migration.
- New features that agents could use get an MCP tool in the same phase.
