# http-trax — design

This document records the design decisions behind http-trax and the reasons for them. When a decision changes, update this file in the same pull request.

## Goals

1. **Git is the collaboration layer.** Collections live in plain files in a repository. Teams review API changes the way they review code: branches, diffs and pull requests. There is no proprietary cloud and no account.
2. **Feels always online.** A background worker fetches and fast-forwards, so teammates' changes show up without anyone pressing "pull", and it never puts local work at risk.
3. **AI agents are first-class users.** An agent that has just built a service can register its endpoints, attach tests and run them through MCP. Everything it does lands in the same files the developer has open.
4. **Scriptable like Postman.** JavaScript pre-request and post-response scripts, with a Postman-compatible surface so existing snippets keep working.
5. **Fast and native on Linux and Windows.** One small binary, no Electron.

## Architecture

```
 ┌───────────────────────────── desktop app (one process) ─────────────────────────────┐
 │                                                                                       │
 │   Next.js UI (static export, React, Zustand)                                          │
 │        │  invoke() / events                                                            │
 │   ─────┼──────────────────────────────── Tauri IPC ──────────────────────────────────  │
 │        ▼                                                                               │
 │   src-tauri: commands · sync worker · file watcher                                     │
 │        │                    │                                                          │
 │        ▼                    ▼                                                          │
 │   trax-core ───────▶ trax-script (Boa JS sandbox)        trax-git (git CLI wrapper)    │
 └────────┼──────────────────────────────────────────────────────────┼───────────────────┘
          │ reads/writes                                               │ spawns `git`
          ▼                                                            ▼
   workspace folder (YAML in a git repo)  ◀──── watched ────  registry.json (per user)
          ▲
          │ reads/writes (same files, same rules)
 ┌────────┴──────────────┐
 │ trax-mcp (stdio)       │ ◀── AI agent (Claude Code, Cursor, VS Code, …)
 │ uses trax-core         │
 └────────────────────────┘
```

| Crate / folder | Responsibility |
| --- | --- |
| `crates/trax-core` | Data model, on-disk workspace format, registry (workspaces, bookmarks, scope, sync prefs), variable interpolation, HTTP client, the request runner, saved responses. |
| `crates/trax-script` | The JavaScript sandbox for scripts: `trax`/`pm` API, `expect`, tests, limits. |
| `crates/trax-git` | Git operations through the `git` CLI, output parsers, review/compare, the one-shot sync pass. |
| `crates/trax-mcp` | MCP server (library + `trax-mcp` binary). |
| `crates/trax-cli` | Command-line runner for CI (library + `trax` binary): selection, console output, JUnit/JSON/Markdown reports. |
| `src-tauri` | Desktop shell: Tauri commands over the crates above, background sync worker, file watcher. |
| `src/` | Next.js UI. `src/lib/api.ts` is the only place that talks to the backend. |

The desktop app, the MCP server and the CLI are deliberately thin layers over `trax-core`. That is what guarantees parity: a request run from the UI and the same request run by an agent go through the same code.

### Why Tauri + Rust + a static Next.js export

- Tauri uses the OS webview (WebView2 on Windows, WebKitGTK on Linux), so installers are a few MB instead of Electron's ~100 MB, and memory use is lower.
- Rust gives us one language for the app backend, the MCP server and the script host, and a single static binary per platform.
- Next.js is used in **static export** mode (`output: "export"`). There is no Node server at runtime; Tauri serves the files from disk. The page renders client-side only (`dynamic(..., { ssr: false })`) because everything it shows comes from the local backend.
- When the UI runs in a plain browser (`npm run dev`), `src/lib/api.ts` routes calls to an in-memory mock (`src/lib/mock.ts`). This lets the UI be developed, reviewed and screenshot-tested without building Rust.

## Workspace format

A workspace is a directory. It can be its own repository (a team's shared collections) or a folder inside a service repository (e.g. `api-tests/`), so tests live and are reviewed next to the code they test.

```
<workspace>/
  trax.yaml                                   manifest: id, name, team, workspace variables
  .gitignore                                  ignores .trax/
  .trax/state.json                            local only (see below)
  collections/
    orders-api/
      collection.yaml                         collection settings: variables, headers, auth, scripts
      create-order.req.yaml                   one request per file
      admin/
        folder.yaml                           folder settings: headers, auth, scripts
        refund-order.req.yaml
  environments/
    local.env.yaml
    staging.env.yaml
  responses/
    <request-id>/<response-id>.yaml           saved responses ("examples")
```

Decisions:

- **One small file per request.** Two people editing different requests never conflict, and a diff names exactly which request changed. A single collection file (Postman's format) turns every edit into a conflict hotspot.
- **YAML, not JSON.** Scripts and bodies are multi-line text. YAML stores them as block scalars (`text: |`), so a one-line change to a script is a one-line diff. JSON would put the whole script on one escaped line. YAML is produced by `serde_yaml_ng` from the same structs that are sent to the UI as JSON, so there is a single model.
- **Stable ids inside the files, readable names on disk.** Every request, folder, collection and environment carries a UUID. File and directory names are slugs of the display name and follow renames (git records the rename). The UI, the MCP server and saved responses refer to things by id, so renames never break references.
- **Ordering with `seq`.** Each item stores its own position. A shared ordering list would conflict whenever two people add items at once.
- **Saved responses live beside, not inside, the request,** keyed by request id. Responses can be large; keeping them out of the request file keeps request diffs small, and moving a request between collections keeps its examples.
- **Broken files don't break the workspace.** Unparseable files (for example a request with unresolved conflict markers) are reported as `problems` while the rest of the tree loads. The UI shows a banner and points to Source control.
- **Ids are validated** (`[A-Za-z0-9_-]`) before being used in a path, so a hand-edited or agent-written id cannot escape the workspace.

### What is shared, what is local

| Data | Where | Shared via git? |
| --- | --- | --- |
| Requests, collections, folders, environments (names and shared values), saved responses | workspace files | yes |
| Values of environment variables marked **secret** | OS keychain, with a `keychain://` pointer in `.trax/state.json`; in `.trax/state.json` itself when there is no keychain or it is turned off | no |
| Vault references held by secret variables (`op://`, `vault://`, `azkv://`) | workspace files | yes |
| Vault, GitHub and GitLab tokens | OS keychain (else `.trax/vault.json` / `.trax/pr_settings.json`) | no |
| Review comments on requests | `.trax/review_comments.json`, and the PR itself on GitHub/GitLab | through the PR |
| Environment "current values" set by scripts (`trax.env.set`) or edited locally | `.trax/state.json` | no |
| Runtime variables (`trax.vars.set`) | `.trax/state.json` | no |
| Cookies stored from responses | `.trax/cookies.json` | no |
| OAuth 2.0 access and refresh tokens | `.trax/state.json` | no |
| Bookmarked collections, scoped collection, active environment, sync settings | `registry.json` in the user config dir | no |

This mirrors Postman's *initial value* / *current value* split. A login script that stores a fresh token should not create a git change for everyone on every run, and secrets must never be committed by accident. Bookmarks and the "scope to this collection" view are personal preferences, so they are per user rather than per repository.

The registry (`registry.json`, under the OS config dir or `$HTTP_TRAX_HOME`) is shared by the app and the MCP server. Both always read-modify-write it (never cache it), so changes from one process are not lost by the other.

## Variables

Placeholders use `{{name}}` in URLs, params, headers, bodies and auth fields. Resolution order, highest first:

1. Run-only variables (passed by the MCP server)
2. Runtime variables (`trax.vars`)
3. Active environment: local overrides, then shared values (secrets included)
4. Collection variables
5. Workspace variables (`trax.yaml`)

Values may reference other variables (resolved up to a fixed depth, so cycles terminate). Unknown placeholders are left in place and reported as warnings in the console, so a missing variable is visible instead of silently becoming an empty string. Dynamic variables: `{{$uuid}}`, `{{$timestamp}}`, `{{$timestampMs}}`, `{{$isoTimestamp}}`, `{{$randomInt}}`.

## Request execution

The runner (`trax-core/src/runner.rs`) follows Postman's order so existing habits carry over:

1. Merge headers: collection → folders → request (later levels override by name).
2. Run pre-request scripts in the same order. Scripts see `{{placeholders}}` intact and can change the method, URL, headers and body, set variables, or call `trax.skipRequest()`.
3. Resolve variables.
4. Apply auth (bearer, basic, API key in header or query, OAuth 2.0). The closest non-`inherit` auth wins, and an explicit `Authorization` header always wins. An OAuth 2.0 token is only fetched once the pre-request scripts have passed, right before sending.
5. Send with `reqwest` (rustls with the OS certificate store; HTTP/2; gzip/brotli/deflate; per-request timeout, redirect and TLS-verification settings). Cookies come from the workspace cookie jar (below).
6. Run post-response scripts (collection → folders → request) and collect tests and logs.
7. Check the response against the contract, if one applies (below).
8. Persist variable changes to local state.

A failing pre-request script stops the request from being sent. Errors are values, not exceptions: a `RunResult` always comes back, carrying the transport error, script errors, tests and logs, so the UI and agents can show exactly what happened.

Collection runs execute requests depth-first in display order and share one variable session, so a login request can feed a token to every request after it.

### Contract checks

A collection or folder can name an OpenAPI document, and a request a JSON Schema file (`contract:` in their YAML, `trax_core::contract`). The nearest setting applies, so a folder can point at another spec and a request can opt out with `off: true`.

- **One test per response**, "contract: GET /users/{id}", added after the scripts' tests. It fails with every violation at once: an undocumented path or method, an undocumented status (exact code, then `4XX`, then `default`), an undocumented content type, a missing required response header, no body where one is documented, or schema errors (the first five, each with its JSON pointer). Being a test, it shows up in the app, in MCP results and in CLI reports with nothing extra, and it fails a collection run like any other test.
- **Matching** strips each server URL's path (variables at their defaults) or Swagger's `basePath`, then tries none, and compares segments: literals must be equal, `{param}` takes at least one character, and partial templates (`{name}.{ext}`) work. The template with the method wins, then the longest server prefix, then the most literal characters, so `/users/me` beats `/users/{id}` but `DELETE /users/me` still finds `DELETE /users/{id}`.
- **Validation** uses the `jsonschema` crate without its HTTP and file retrievers: only local `$ref`s resolve (the spec's `components` and `definitions` are put next to the response schema). OpenAPI 3.0 and Swagger 2.0 schemas are validated as draft 4 (`exclusiveMinimum: true`, `$ref` siblings ignored), with 3.0's `nullable: true` rewritten to allow `null`; OpenAPI 3.1 as draft 2020-12; a standalone schema file by its own `$schema`. Formats are not checked, as in most OpenAPI tooling, because servers rarely agree on `date-time` or `email` edge cases and a contract test should fail on real drift.
- **Paths are relative to the workspace root and may leave it** (`../openapi.yaml`): in a service repository the spec usually lives next to the code, not inside `api-tests/`. Documents are parsed once and cached until the file changes. Missing or unparseable files fail the contract test with the reason rather than the run.
- The field is optional, so `FORMAT_VERSION` stays at 1. The semantic diff reports "Contract added/changed/removed", and the semantic merge treats it as one field.

### OAuth 2.0

`auth: {type: oauth2}` (`trax_core::oauth`) supports four grants. Every field takes `{{variables}}`, so token URLs and client ids can differ per environment and a client secret can live in a secret variable.

| Grant | Who gets the token |
| --- | --- |
| Client credentials, password | The runner, by itself, whenever there is no valid token |
| Authorization code (PKCE S256 by default) | The user once, from the Authorization tab: the browser opens the provider's page and the redirect comes back to a loopback listener (`http://127.0.0.1:<free port>/callback`, or a fixed loopback URI registered with the provider, as RFC 8252 recommends for native apps). The `state` parameter is checked. |
| Device code (RFC 8628) | The user once: the app shows the code and the verification page, and polls (honouring `slow_down`) |

Before each send the runner uses the cached token while it has more than 30 seconds left, else refreshes it when there is a refresh token (keeping the old refresh token when the server doesn't rotate it), else fetches a new one for the unattended grants. For the sign-in grants it stops with a message pointing to the Authorization tab rather than sending an unauthenticated request. Token errors are reported with the provider's `error` and `error_description`.

Tokens are kept in `.trax/state.json` (never committed), keyed by grant, token URL, client id, scope and user. Two environments with different token URLs or clients therefore get their own token, and a token is reused by every request that shares the same settings (typically set once on the collection). Client credentials go in a Basic header by default (client_secret_basic) or in the body; extra parameters (`audience`, `resource`) are sent with the authorization and token requests. Agents use OAuth 2.0 like the app, cached tokens are masked in their output, and a sign-in grant without a token tells them to ask the user to sign in.

### Cookie jar

Each workspace has a cookie jar (`trax_core::cookies`), so a login request that sets a session cookie works for every request after it, like in a browser. The jar is a `reqwest` cookie provider, which means cookies set on a redirect (the usual `POST /login` → `302` → `GET /home`) are stored and sent on the next hop too. Matching follows RFC 6265 (domain, path, `Secure`, expiry) via the `cookie_store` crate.

- Stored in `.trax/cookies.json` as a readable list (name, value, domain, path, expiry, flags). Session cookies are kept until cleared, as in Postman. Never committed.
- A request that sets its own `Cookie` header bypasses the jar, as an explicit `Authorization` header bypasses auth. Settings → "Use the cookie jar" turns it off per request (`settings.cookies: false` in the YAML, written only when off).
- The cookies sent are listed in the sent request, and the response's Cookies tab shows what it set. "Copy as cURL" with variables filled in includes them.
- The app and `trax-mcp` each keep the jar in memory and reload it when the file's modification time changes, so cookies from an agent's login are seen by the app and the other way round.

## Scripting

Scripts run in [Boa](https://boajs.dev), a JavaScript engine written in Rust.

- **Why in Rust rather than in the webview?** The MCP server and the CLI must run the same scripts with no webview. One engine guarantees identical behaviour everywhere.
- **Why Boa rather than QuickJS or V8?** Boa is pure Rust: no C toolchain quirks on Windows, no large native dependency, and it builds anywhere `cargo` does. Script workloads (assertions, small transforms) are tiny, so raw engine speed doesn't matter here. The boundary is engine-agnostic, so this can be revisited.
- **JSON in, JSON out.** The host serialises the request, response and variables to JSON. A JavaScript prelude (`crates/trax-script/src/prelude.js`) builds the `trax` API on top of it, runs the user script as an `AsyncFunction` (so top-level `await` works and syntax errors are caught), and returns the new state as JSON. There are no native callbacks into Rust, so the sandbox has no file, network or process access at all.
- **Limits.** Loop iteration and recursion limits stop runaway scripts. Uncaught errors are reported along with any tests and logs recorded before them.
- **API.** `trax.request`, `trax.response` (`status`, `headers.get`, `json()`, `time`), `trax.env`, `trax.vars`, `trax.variables`, `trax.collectionVars` (read-only), `trax.test`, `trax.expect` (Jest-style matchers plus Chai-style chains such as `.to.have.property`), `console.*`, `trax.uuid`, `btoa`/`atob`, `trax.base64`. A `pm` alias covers the commonly used Postman surface (`pm.environment`, `pm.test`, `pm.expect`, `pm.response.to.have.status`, ...).

## Git

All git operations shell out to the user's `git` executable (`crates/trax-git`).

- **Why the CLI rather than libgit2 (`git2`) or gitoxide?** Authentication is the hard part of git in a desktop app, and the CLI already solves it the way the user has configured: Git Credential Manager on Windows, SSH agents and `~/.ssh/config`, `gh auth`, `includeIf`-based identities per organisation, signing and hooks. libgit2 would mean re-implementing credential callbacks and still differing from the terminal in subtle ways. It also keeps OpenSSL/libssh2 out of the binary.
- **Machine-readable output only:** `status --porcelain=v2 -z`, `for-each-ref`/`log` with `%x1f`/`%x1e` separators, `diff --name-status -z`. Parsers are unit-tested.
- **Never hang, never flash a window:** `GIT_TERMINAL_PROMPT=0` always; background calls also set `GCM_INTERACTIVE=never`; every call has a timeout; on Windows processes are spawned with `CREATE_NO_WINDOW`.
- **Review** compares two refs like a pull request: commits (`base..head`), changed files with line counts, the diff (`base...head`), and a dry-run conflict check with `git merge-tree --write-tree`. The UI labels files as requests, environments, collections or saved responses. "Open pull request" builds the compare URL for GitHub (also GHE, Gitea, Forgejo), GitLab, Bitbucket and Azure DevOps.
- **Merging and conflicts.** Pull supports merge, rebase and fast-forward-only. Merge outcomes are reported as up-to-date, fast-forward, merged, rebased or conflicts; conflicted files can be resolved with "keep mine", "take theirs" or "mark resolved", then the merge/rebase is continued or aborted.
- **Semantic merge** (`trax-core/src/merge.rs`). Git's line merge sees two people adding different headers to the same request as a conflict. For a conflicted request, environment, collection, folder or `trax.yaml`, the three index stages (`:1:`, `:2:`, `:3:`) are parsed into the model and merged per field: scalars (name, method, URL, auth, settings), keyed lists (headers, params, variables, form fields) by key, JSON bodies key by key (nested objects too), and scripts as text, where two sides that each appended a test are combined. Only fields both sides changed differently become `FieldConflict`s; each has a stable id (`headers.Authorization`), and the user's pick per id (mine, theirs, original) produces the merged YAML, which is written and staged. Files that don't parse fall back to the line-level choices.
- **Stash, cherry-pick, revert, tags.** Thin wrappers over the CLI; cherry-pick and revert report conflicts like a merge, so the same editor resolves them. Tags can be annotated, pushed and deleted.
- **Sparse checkout** for monorepos: `clone --filter=blob:none --sparse` then `sparse-checkout set` (cone mode by default), on clone, from an onboarding link, or later in Settings.
- **Commit signing.** `log` reads `%G?`, `%GS` and `%GK`, so history shows Verified / unverified per commit. Signing settings (`commit.gpgsign`, `gpg.format` gpg/ssh/x509, `user.signingkey`, the program) are written to the repository's own config, so each team's repository can sign differently.
- **Pull requests** (`trax-core/src/pr.rs`) go straight to the GitHub REST and GitLab v4 APIs with the token from Settings (kept in the OS keychain), `GITHUB_TOKEN`/`GITLAB_TOKEN`, or `gh auth token`. The host comes from `origin` (GitHub Enterprise and self-hosted GitLab via an API URL override). Review comments carry the request field they're about; on GitHub they are anchored to the line of that field at the PR's head commit, and become a conversation comment tagged with the field when the line isn't in the diff. A comment meant for a PR that can't be posted is an error, never silently kept local.
- **Multiple teams.** Each team's collections are a workspace backed by that team's repository; a developer registers as many as they need and switches between them (grouped by team in the switcher). Git identity can be set per repository, which matters when the same person commits to different organisations.

## Background sync ("always online")

`src-tauri/src/sync.rs` runs a loop that, for every registered workspace with sync enabled, calls `trax_git::sync::sync_once` on its interval (default 60 s, minimum 15 s). The window regaining focus also triggers a pass (throttled).

A pass:

1. Fetches the remote with prompts disabled. Failures are classified as offline, auth-required or error, and the UI shows them; nothing else happens.
2. Computes ahead/behind and local changes.
3. Fast-forwards the current branch **only if** it is behind, has no local commits, has no uncommitted changes to tracked files, and no merge or rebase is in progress. It never merges, rebases or touches local edits.
4. Summarises incoming commits: their authors and the requests, environments and collections they touch (`changedRequests`).
5. Emits `sync-status`, and `workspace-changed` when commits arrived. The UI reloads the tree, refreshes open tabs and shows who changed what ("Grace changed Create order, Refund order"), both when commits are waiting and when they were pulled.

Passes for the same workspace never overlap. A file watcher (`notify`) on every workspace and on the registry covers the other sources of change: an MCP agent adding requests, the user editing YAML by hand, a `git checkout` in a terminal. Events are debounced.

If a file changes on disk while its tab has unsaved edits, the tab is not overwritten. It shows "this request changed on disk" with *Load theirs* / *Keep mine*.

## MCP server

`trax-mcp` speaks MCP over stdio (newline-delimited JSON-RPC 2.0) and negotiates protocol versions `2025-11-25`, `2025-06-18`, `2025-03-26` and `2024-11-05`. It is implemented directly rather than with an SDK because the surface is small (initialize, ping, tools/list, tools/call) and the SDKs' APIs were still moving quickly.

Tools: `list_workspaces`, `init_workspace`, `get_workspace`, `get_request`, `upsert_collection`, `upsert_request`, `delete_request`, `move_item`, `import_spec`, `run_request`, `run_collection`, `send_http_request`, `list_environments`, `upsert_environment`, `list_saved_responses`, `list_cookies`, `clear_cookies`, `list_history`, `restore_history_entry`, `list_workspace_templates`, `create_onboarding_link`, `list_pull_requests`, `create_pull_request`, `approve_pull_request`, `merge_pull_request`, `list_review_comments`, `add_review_comment`, `resolve_review_comment`, `semantic_merge_file`. Auth objects accept every type the app has, OAuth 2.0 included.

Decisions:

- **Same files, same rules.** The server uses `trax-core` directly. Anything it creates is a normal YAML file that the app's watcher picks up, and that the developer reviews and commits like any other change.
- **Workspace resolution:** the `workspace` argument (id, name or path), else `--workspace`, else the workspace active in the app, else the current directory. The environment defaults to the one selected in the app, so "run it against what I'm looking at" is the default.
- **Forgiving inputs.** Headers and params may be maps or key/value lists; bodies may be JSON values, strings or `{mode, ...}`; `upsert_request` merges into an existing request matched by id or by name within the folder, and creates missing collections and folders.
- **Model-friendly outputs.** Each result has a compact text rendering (status, timing, test results with failure messages, console, pretty-printed and truncated body) plus `structuredContent` with the full JSON. Collection runs append details only for failing requests, so the agent can fix the service without another round-trip.
- **Tool calls are serialised.** Clients may call tools in parallel; files and the registry are read-modify-write, so tool calls run one at a time (pings stay responsive).
- **Shipped with the app.** `scripts/build-sidecar.mjs` builds `trax-mcp` and Tauri bundles it (`externalBin`) next to the main executable. The AI agents view in the app shows ready-to-paste configs for Claude Code, Claude Desktop/Cursor/Windsurf and VS Code using the installed path.
- **The user decides what agents may do.** Five permissions live in the registry (`agentPermissions`) and `trax-mcp` reads them on every tool call, so a change in the app applies immediately: create and edit requests, run requests, read responses (status and test results are always returned), edit environments, read secret values. Editing environments and reading secrets are off by default; agents put base URLs in collection variables instead. With secrets off, the values of secret variables, of cookies in the jar and of cached OAuth 2.0 tokens are masked anywhere in a tool's output, including echoed request headers. A refused call names the setting to ask the user about.
- **Protected environments** are never used by agents: a run whose environment is protected fails with a message telling the agent to pick another environment or ask the user.
- **Who is connected, and what they did.** Each `trax-mcp` process writes `agents/<pid>.json` in the registry directory after `initialize` (client name from `clientInfo`) and refreshes it every 30 seconds; the app treats sessions seen in the last 90 seconds as connected and also checks the clients' config files to show "Set up". Creating, editing, deleting and running are appended to `.trax/agent-activity.json` in the workspace (local, last 200 entries), which the agents view lists.

## CLI

`trax` (`courion-cli`) runs collections outside the app, mainly in CI: `trax run [WORKSPACE] [-c Collection/Folder]... [-e Environment]`, plus `trax list` to discover names.

Decisions:

- **Same runner.** Each selected collection or folder goes through `run_group_with_progress`, the app's collection runner with a callback per finished request, so lines stream to the CI log instead of appearing when the collection ends. Variables flow between requests of a collection as in the app; each collection starts fresh.
- **Exit codes CI can rely on:** `0` all passed, `1` something failed (a test, a script error, no response), `2` nothing could run (usage, missing workspace, unknown collection, folder or environment, a protected environment without `--allow-protected`, unreadable workspace files, a report that can't be written). A requested environment that doesn't exist is an error rather than the app's "run without it" fallback, because a typo in a pipeline should fail loudly. Files that don't parse (an unresolved merge conflict, say) stop the run for the same reason: those requests would otherwise be skipped silently.
- **Workspace resolution:** a folder with `trax.yaml` (default: the current directory), else a workspace registered in the app by name or id. No environment is selected unless `-e` is passed: the app's selection is a per-machine preference, and pipelines should say what they run against.
- **Secrets from the pipeline.** Secret values are never committed, so in CI an environment's secret variables are empty. `--secret key=ENV` reads a value from the process environment; the CLI warns about each secret variable of the selected environment that has no value. Every secret value (from `--secret`, or a secret variable the environment resolved, vault references included) is masked in the console and in all reports. Values shorter than four characters aren't masked, as they'd mangle unrelated text.
- **Stateless by default.** Runs don't write variable changes or history to `.trax/` unless `--persist` is passed, so a pipeline (or a local check) doesn't leave state that changes the next run. Cookies and OAuth 2.0 tokens are still cached as in the app.
- **Reports.** JUnit: one `<testsuite>` per request ("Collection / Folder / Request"), one `<testcase>` per test; a request without tests gets a "response received" case, transport and script errors are `<error>`s, failed assertions `<failure>`s. JSON: every request with status, timing, tests, script errors and console output. Markdown: a summary table and the failures, sized for `$GITHUB_STEP_SUMMARY`. Response headers and bodies are left out of all three, since reports become build artifacts.
- **Hand-rolled arguments**, like `trax-mcp`: a handful of flags doesn't justify `clap`'s compile time and binary size.

## UI

The interface follows the "Courion" design (dark first, quiet chrome, one indigo accent, Inter). Only the UI carries that name for now: the window title, logo and wordmark. Binaries, installers, the MCP server and the `trax.*` script API keep the http-trax name.

- **Frame:** the window is frameless (`decorations: false`). A 32px title strip holds the logo and the window controls; empty space drags the window. On Linux, resizing a frameless window depends on the window manager; if that turns out to be unreliable, native decorations come back.
- **Layout:** the window background is the frame. A 48px icon rail (Collections, Environments, History, AI agents, Source control, Settings) and a resizable sidebar sit on it, and the main content lives in an inset, rounded card. The sidebar header holds the workspace switcher, search (Ctrl/Cmd+K) and New request; its footer holds the environment picker. Each rail section has a sidebar part and a card part:
  - Collections and History: tree or run list in the sidebar, request tabs in the card.
  - Environments: the list in the sidebar (picking one makes it active), the active environment in the card.
  - AI agents: MCP clients and their state in the sidebar; setup snippets, server details, permissions and recent agent activity in the card.
  - Source control: changed files and the commit box in the sidebar; the selected change and history in the card.
  - Settings: section index in the sidebar, the settings page in the card.
- **Requests** show a breadcrumb, the URL bar (method, URL with highlighted `{{variables}}`, Send, Save, and a menu for code snippets, copy as cURL and cookies), request sections (Query params, Body, Headers, Authorization, Scripts, Settings, Docs) and the response (Body, Headers, Cookies, Tests, Console, Request, Saved). The response sits below or beside the request; the choice is remembered. HTML responses get a Preview, rendered in a sandboxed iframe with a CSP that blocks scripts and every outside request.
- **Environments** carry two switches. *Sync with Git* moves the file between `environments/` (committed) and `.trax/environments/` (this machine only). *Protected* asks for confirmation before anything is sent with it and keeps agents away from it. The variables table shows, per variable, whether its value is synced, local only, or never synced (secrets).
- **Readable diffs.** For workspace files Source control shows semantic changes by default ("URL changed", "Header added", "Body updated · currency", "Test added", "Variable updated"), computed by `trax_core::diff::semantic_diff` from the before and after YAML. Secrets show as "secret · not synced". Raw shows the line diff with old and new line numbers. Files that aren't workspace YAML, and conflicted files, show the line diff only.
- **History** lists recent runs (from the app and from agents through MCP) grouped by day. It is local to the machine (`.trax/history.json`, last 300 runs) and never committed.
- **Command palette** (Ctrl/Cmd+K) searches requests by name, URL and path, lists recent runs, and runs commands. Ctrl/Cmd+. on a request opens its actions (open, open and send, copy URL).
- **Import** takes a cURL command, OpenAPI 3.x / Swagger specs (YAML or JSON), Postman collections (v2.0/v2.1) and environments, HAR archives, Insomnia exports, or Bruno collections (`trax_core::import`). Postman's `pm.*` scripts keep working, example responses are preserved as saved responses, and OAuth 2.0 settings are carried over (not the tokens). Any format can also be imported programmatically by agents via the `import_spec` MCP tool.
- **Bookmarks** pin collections to a "Pinned" section at the top of the tree, in the order they were pinned. **Scope** shows only one collection and hides the others until "Show all". Both are per-user, per-workspace preferences stored in the registry.
- **State:** a Zustand store (`src/lib/store.ts`) plus small ones for Source control and the agents view, whose sidebar and card halves share state. Tabs are kept per workspace, so switching teams and back restores your tabs. Dirty state is a comparison of the draft with the last saved snapshot.
- **Theme:** design tokens as CSS variables (`src/app/globals.css`) mapped into Tailwind v4's theme; dark by default, light derived from the same tokens. Inter for the interface and JetBrains Mono for code, bundled with the app (no font requests at runtime). CodeMirror 6 for editors, with a highlight style that uses the same tokens.
- **Environment colours** are a convention, not a setting: names containing "prod" are red, "staging", "qa", "test" and similar amber, "local" and "dev" green, anything else blue.
- **Keyboard:** Ctrl/Cmd+K search, Ctrl/Cmd+. actions in the palette, Ctrl/Cmd+N new request, Ctrl/Cmd+E switch environment, Ctrl/Cmd+Shift+I import, Ctrl/Cmd+1–9 switch workspace, Ctrl/Cmd+Enter send, Ctrl/Cmd+S save, Ctrl/Cmd+W close tab.

## Security notes

- Scripts have no I/O. They can only shape the request and read the response.
- AI agents get only what the user allows (see MCP server). Secret values are masked in their output unless the user turns that on, and they never run requests against a protected environment.
- Secret variables, OAuth 2.0 tokens and cookies never reach the shared files. Secret values go to the OS keychain (Credential Manager and the macOS Keychain through `keyring`; on Linux libsecret's `secret-tool`, with the value on stdin so it never appears in a process list); without a keychain they fall back to `.trax/`, which `init` adds to `.gitignore`. There is no plaintext mirror of keychain entries.
- Vault references are resolved only when a request is sent, cached in memory for two minutes, and an unreachable vault stops the run rather than sending the reference as a value. Vault CLIs (`op`, `vault`, `az`) run without a console window.
- Onboarding links carry names, the repository, non-secret variables and vault references from a shared environment only. Secret values are never put in a link; the newcomer types them and they go to their keychain.
- The OAuth 2.0 loopback listener binds to 127.0.0.1 only, lives for one sign-in (at most five minutes), and accepts a code only with the `state` it sent.
- TLS verification is on by default and can be disabled per request only.
- The webview CSP is currently disabled (`"csp": null`) while the static export's inline bootstrap scripts are verified against Tauri's CSP hash injection. HTML responses are previewed inside an iframe with an empty `sandbox` (no scripts, no same-origin access, no navigation of the app) and a CSP that blocks every outside request; images are shown from `data:` URLs. Tightening the CSP is on the roadmap.
- No telemetry and no network traffic except the requests you send and the git remotes you configure.

## Testing and CI

- Rust unit and integration tests: the script sandbox, workspace storage, the runner against a local mock HTTP server (`wiremock`), git operations against real temporary repositories (two clones of a bare remote simulate teammates, including conflicts and auto-pull), an end-to-end MCP session in which an "agent" builds and runs a suite, and CLI runs (reports, masking, `--bail`, setup errors, and the binary's exit codes) against a mock server.
- Frontend: ESLint (Next.js core-web-vitals + TypeScript rules, React Compiler checks), `tsc --noEmit`, and a production build.
- GitHub Actions: `ci.yml` runs frontend checks, Rust fmt/clippy/tests on Linux and Windows, then builds the desktop app and installers on both (Linux: .deb, .rpm, AppImage; Windows: .msi and NSIS setup) and uploads them with standalone `trax-mcp` and `trax` binaries. `release.yml` builds the same on a `v*` tag and publishes everything on that tag's GitHub release (a pre-release when the tag has a suffix such as `-beta.1`).

## Known trade-offs

- Each tree load parses every YAML file. That is fast for hundreds of requests; an in-memory index invalidated by the watcher will be needed for very large workspaces.
- The app shows the whole repository's status even when the workspace is a subfolder (files outside the workspace are dimmed). Committing commits what is staged.
- Query parameters from the Params table are appended to the URL after variable resolution and are not re-encoded; use `%26` for a literal `&` inside a value (Postman behaves the same).
- Multipart file parts are read into memory.
