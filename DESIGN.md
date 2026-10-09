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
| `src-tauri` | Desktop shell: Tauri commands over the crates above, background sync worker, file watcher. |
| `src/` | Next.js UI. `src/lib/api.ts` is the only place that talks to the backend. |

The desktop app and the MCP server are deliberately thin layers over `trax-core`. That is what guarantees parity: a request run from the UI and the same request run by an agent go through the same code.

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
| Values of environment variables marked **secret** | `.trax/state.json` | no |
| Environment "current values" set by scripts (`trax.env.set`) or edited locally | `.trax/state.json` | no |
| Runtime variables (`trax.vars.set`) | `.trax/state.json` | no |
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
4. Apply auth (bearer, basic, API key in header or query). The closest non-`inherit` auth wins, and an explicit `Authorization` header always wins.
5. Send with `reqwest` (rustls with the OS certificate store; HTTP/2; gzip/brotli/deflate; per-request timeout, redirect and TLS-verification settings).
6. Run post-response scripts (collection → folders → request) and collect tests and logs.
7. Persist variable changes to local state.

A failing pre-request script stops the request from being sent. Errors are values, not exceptions: a `RunResult` always comes back, carrying the transport error, script errors, tests and logs, so the UI and agents can show exactly what happened.

Collection runs execute requests depth-first in display order and share one variable session, so a login request can feed a token to every request after it.

## Scripting

Scripts run in [Boa](https://boajs.dev), a JavaScript engine written in Rust.

- **Why in Rust rather than in the webview?** The MCP server and (later) a CLI runner must run the same scripts with no webview. One engine guarantees identical behaviour everywhere.
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
- **Multiple teams.** Each team's collections are a workspace backed by that team's repository; a developer registers as many as they need and switches between them (grouped by team in the switcher). Git identity can be set per repository, which matters when the same person commits to different organisations.

## Background sync ("always online")

`src-tauri/src/sync.rs` runs a loop that, for every registered workspace with sync enabled, calls `trax_git::sync::sync_once` on its interval (default 60 s, minimum 15 s). The window regaining focus also triggers a pass (throttled).

A pass:

1. Fetches the remote with prompts disabled. Failures are classified as offline, auth-required or error, and the UI shows them; nothing else happens.
2. Computes ahead/behind and local changes.
3. Fast-forwards the current branch **only if** it is behind, has no local commits, has no uncommitted changes to tracked files, and no merge or rebase is in progress. It never merges, rebases or touches local edits.
4. Emits `sync-status`, and `workspace-changed` when commits arrived. The UI reloads the tree and refreshes open tabs.

Passes for the same workspace never overlap. A file watcher (`notify`) on every workspace and on the registry covers the other sources of change: an MCP agent adding requests, the user editing YAML by hand, a `git checkout` in a terminal. Events are debounced.

If a file changes on disk while its tab has unsaved edits, the tab is not overwritten. It shows "this request changed on disk" with *Load theirs* / *Keep mine*.

## MCP server

`trax-mcp` speaks MCP over stdio (newline-delimited JSON-RPC 2.0) and negotiates protocol versions `2025-11-25`, `2025-06-18`, `2025-03-26` and `2024-11-05`. It is implemented directly rather than with an SDK because the surface is small (initialize, ping, tools/list, tools/call) and the SDKs' APIs were still moving quickly.

Tools: `list_workspaces`, `init_workspace`, `get_workspace`, `get_request`, `upsert_collection`, `upsert_request`, `delete_request`, `run_request`, `run_collection`, `send_http_request`, `list_environments`, `upsert_environment`, `list_saved_responses`.

Decisions:

- **Same files, same rules.** The server uses `trax-core` directly. Anything it creates is a normal YAML file that the app's watcher picks up, and that the developer reviews and commits like any other change.
- **Workspace resolution:** the `workspace` argument (id, name or path), else `--workspace`, else the workspace active in the app, else the current directory. The environment defaults to the one selected in the app, so "run it against what I'm looking at" is the default.
- **Forgiving inputs.** Headers and params may be maps or key/value lists; bodies may be JSON values, strings or `{mode, ...}`; `upsert_request` merges into an existing request matched by id or by name within the folder, and creates missing collections and folders.
- **Model-friendly outputs.** Each result has a compact text rendering (status, timing, test results with failure messages, console, pretty-printed and truncated body) plus `structuredContent` with the full JSON. Collection runs append details only for failing requests, so the agent can fix the service without another round-trip.
- **Tool calls are serialised.** Clients may call tools in parallel; files and the registry are read-modify-write, so tool calls run one at a time (pings stay responsive).
- **Shipped with the app.** `scripts/build-sidecar.mjs` builds `trax-mcp` and Tauri bundles it (`externalBin`) next to the main executable. The MCP panel in the app shows ready-to-paste configs for Claude Code, Claude Desktop/Cursor and VS Code using the installed path.

## UI

The interface follows the "Courion" design (dark first, quiet chrome, one accent colour). Only the UI carries that name for now: the window title, logo and wordmark. Binaries, installers, the MCP server and the `trax.*` script API keep the http-trax name.

- **Frame:** the window is frameless (`decorations: false`). The 40px toolbar is the title bar: logo, workspace switcher, New, Import, environment picker, sync status, search, and window controls drawn by the app. Empty toolbar space drags the window. On Linux, resizing a frameless window depends on the window manager; if that turns out to be unreliable, native decorations come back.
- **Layout:** a 46px icon rail (Collections, Environments, History, Source control, AI agents, Settings), a resizable side panel, and the main area with tabs (requests, collection and folder settings, environments, runs, reviews). Requests show a breadcrumb, the URL bar (method, URL with highlighted `{{variables}}`, Send, Save), request sections (Query params, Body, Headers, Authorization, Scripts, Settings, Docs) and the response (Body, Headers, Tests, Console, Request, Saved). The response sits below or beside the request; the choice is remembered.
- **Source control** is a full-width view rather than a side panel, because reviewing changes needs room: changed files with stage checkboxes and the commit box on the left, the selected change in the middle, history on the right. Branch switching, creating, merging, comparing and opening a pull request live in one branch menu.
- **Readable diffs.** For workspace files the middle column shows semantic changes by default ("URL changed", "Header added", "Body updated · currency", "Test added", "Variable updated"), computed by `trax_core::diff::semantic_diff` from the before and after YAML. Secrets show as "secret · not synced". Raw shows the line diff with old and new line numbers. Files that aren't workspace YAML, and conflicted files, show the line diff only.
- **History** lists recent runs (from the app and from agents through MCP) grouped by day. It is local to the machine (`.trax/history.json`, last 300 runs) and never committed.
- **Command palette** (Ctrl/Cmd+K) searches requests by name, URL and path, lists recent runs, and runs commands (new request, switch workspace or environment, import from cURL, sync, run a collection, toggle layout and theme). **Import from cURL** turns a pasted command into a request (method, headers, body, basic and bearer auth).
- **Bookmarks** pin collections to a "Pinned" section at the top of the tree, in the order they were pinned. **Scope** shows only one collection and hides the others until "Show all". Both are per-user, per-workspace preferences stored in the registry.
- **State:** a single Zustand store (`src/lib/store.ts`). Tabs are kept per workspace, so switching teams and back restores your tabs. Dirty state is a comparison of the draft with the last saved snapshot.
- **Theme:** design tokens as CSS variables (`src/app/globals.css`) mapped into Tailwind v4's theme; dark by default, light derived from the same tokens. Schibsted Grotesk for the interface and JetBrains Mono for code, bundled with the app (no font requests at runtime). CodeMirror 6 for editors, with a highlight style that uses the same tokens.
- **Environment colours** are a convention, not a setting: names containing "prod" are red, "staging", "qa", "test" and similar amber, "local" and "dev" green, anything else blue.
- **Keyboard:** Ctrl/Cmd+K search, Ctrl/Cmd+N new request, Ctrl/Cmd+E environments, Ctrl/Cmd+Shift+I import from cURL, Ctrl/Cmd+1–9 switch workspace, Ctrl/Cmd+Enter send, Ctrl/Cmd+S save, Ctrl/Cmd+W close tab.

## Security notes

- Scripts have no I/O. They can only shape the request and read the response.
- Secret variables never reach the shared files. They are stored in `.trax/state.json`, which `init` adds to `.gitignore`. OS-keychain storage is on the roadmap.
- TLS verification is on by default and can be disabled per request only.
- The webview CSP is currently disabled (`"csp": null`) while the static export's inline bootstrap scripts are verified against Tauri's CSP hash injection. Response bodies are never rendered as HTML; images are shown from `data:` URLs. Tightening the CSP is on the roadmap.
- No telemetry and no network traffic except the requests you send and the git remotes you configure.

## Testing and CI

- Rust unit and integration tests: the script sandbox, workspace storage, the runner against a local mock HTTP server (`wiremock`), git operations against real temporary repositories (two clones of a bare remote simulate teammates, including conflicts and auto-pull), and an end-to-end MCP session in which an "agent" builds and runs a suite.
- Frontend: ESLint (Next.js core-web-vitals + TypeScript rules, React Compiler checks), `tsc --noEmit`, and a production build.
- GitHub Actions: `ci.yml` runs frontend checks, Rust fmt/clippy/tests on Linux and Windows, then builds the desktop app and installers on both (Linux: .deb, .rpm, AppImage; Windows: .msi and NSIS setup) and uploads them with standalone `trax-mcp` binaries. `release.yml` builds the same on a `v*` tag and publishes everything on that tag's GitHub release (a pre-release when the tag has a suffix such as `-beta.1`).

## Known trade-offs

- Each tree load parses every YAML file. That is fast for hundreds of requests; an in-memory index invalidated by the watcher will be needed for very large workspaces.
- The app shows the whole repository's status even when the workspace is a subfolder (files outside the workspace are dimmed). Committing commits what is staged.
- Query parameters from the Params table are appended to the URL after variable resolution and are not re-encoded; use `%26` for a literal `&` inside a value (Postman behaves the same).
- Multipart file parts are read into memory.
