"use client";

import { Check, Copy } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { useStore } from "@/lib/store";
import type { ActivityKind, AgentClient, AgentPermissions } from "@/lib/types";
import { cx, envTone, timeAgo } from "@/lib/util";

import { useAgents } from "../agents/agentsStore";
import { Switch } from "../ui/Controls";

/** Tools `courion-mcp` (`trax-mcp`) offers (see crates/trax-mcp/src/tools.rs). */
const TOOLS = [
  "list_workspaces",
  "init_workspace",
  "get_workspace",
  "get_request",
  "upsert_collection",
  "upsert_request",
  "delete_request",
  "move_item",
  "run_request",
  "run_collection",
  "send_http_request",
  "list_environments",
  "upsert_environment",
  "list_saved_responses",
  "list_history",
  "restore_history_entry",
];

const PERMISSIONS: { key: keyof AgentPermissions; label: string; desc: string }[] = [
  { key: "editRequests", label: "Create and edit requests", desc: "Agents can register endpoints and change existing requests as YAML." },
  { key: "runRequests", label: "Run requests and tests", desc: "Runs use the active environment and show up in History." },
  { key: "readResponses", label: "Read responses", desc: "Lets agents see headers and bodies from runs. Status and test results are always shared." },
  { key: "editEnvironments", label: "Edit environments", desc: "Off keeps agents from adding or changing variables. They can still use collection variables." },
  { key: "readSecrets", label: "Read secret values", desc: "Off shows secrets to agents as masked values." },
];

const KIND: Record<ActivityKind, { label: string; className: string }> = {
  created: { label: "Created", className: "bg-ok/14 text-ok" },
  edited: { label: "Edited", className: "bg-warn/14 text-warn" },
  deleted: { label: "Deleted", className: "bg-danger/14 text-danger" },
  ran: { label: "Ran", className: "bg-accent-soft text-accent-text" },
};

const TABS: { id: AgentClient["id"]; label: string; hint: string }[] = [
  { id: "code", label: "Claude Code", hint: "Run in your terminal" },
  { id: "desktop", label: "Desktop / Cursor", hint: "mcp.json (Claude Desktop, Cursor, Windsurf)" },
  { id: "vscode", label: "VS Code", hint: ".vscode/mcp.json" },
];

/** Colour a JSON snippet: keys, strings, punctuation. */
function highlightJson(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /("(?:[^"\\]|\\.)*")(\s*:)?|([{}[\],:])|([^"{}[\],:]+)/g;
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = re.exec(text))) {
    if (m[1] && m[2]) {
      out.push(<span key={i++} className="text-key">{m[1]}</span>, <span key={i++} className="text-faint">{m[2]}</span>);
    } else if (m[1]) {
      out.push(<span key={i++} className="text-[var(--syn-str)]">{m[1]}</span>);
    } else if (m[3]) {
      out.push(<span key={i++} className="text-faint">{m[3]}</span>);
    } else {
      out.push(<span key={i++}>{m[4]}</span>);
    }
  }
  return out;
}

function CopyButton({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      className="btn-ghost h-[26px] text-[12.5px]"
      onClick={() => {
        void navigator.clipboard?.writeText(text);
        setDone(true);
        setTimeout(() => setDone(false), 1200);
      }}
    >
      {done ? <Check size={13} /> : <Copy size={13} strokeWidth={1.4} />}
      {done ? "Copied" : "Copy"}
    </button>
  );
}

function Heading({ children, right }: { children: ReactNode; right?: ReactNode }) {
  return (
    <div className="mt-7 flex items-center justify-between">
      <span className="text-[13px] font-semibold">{children}</span>
      {right}
    </div>
  );
}

/** Main card of the AI agents view: connect a client, what agents may do, what they did. */
export function AgentsView() {
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const info = useStore((s) => s.info);
  const entry = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId) ?? null);
  const env = useStore((s) => {
    const id = s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment;
    return s.tree?.environments.find((e) => e.id === id) ?? null;
  });
  const gitEpoch = useStore((s) => s.gitEpoch);
  const setView = useStore((s) => s.setView);
  const overview = useAgents((s) => s.overview);
  const client = useAgents((s) => s.client);
  const { setClient, setPermission, load } = useAgents.getState();

  // Sessions refresh every 30s on the server side; poll a little faster while visible.
  useEffect(() => {
    void load(workspaceId);
    const t = setInterval(() => void load(workspaceId), 10_000);
    return () => clearInterval(t);
  }, [workspaceId, gitEpoch, load]);

  const command = info?.mcpCommand ?? "courion-mcp";
  const quote = (s: string) => (/[\s"]/.test(s) ? JSON.stringify(s) : s);
  const wsPath = entry?.path ?? "./api-tests";
  const tab = TABS.find((t) => t.id === client) ?? TABS[0];
  const snippet =
    tab.id === "code"
      ? `claude mcp add courion -- ${quote(command)} --workspace ${quote(wsPath)}`
      : JSON.stringify(
          tab.id === "desktop"
            ? { mcpServers: { courion: { command, args: ["--workspace", wsPath] } } }
            : { servers: { courion: { type: "stdio", command, args: ["--workspace", wsPath] } } },
          null,
          2,
        );
  const sessions = overview?.sessions ?? [];
  const status = sessions.length
    ? { text: sessions.length === 1 ? `${sessions[0].client} connected` : `${sessions.length} agents connected`, className: "bg-ok/12 text-ok", dot: "bg-ok" }
    : info?.mcpCommand
      ? { text: "MCP server ready", className: "bg-selected text-muted", dot: "bg-faint" }
      : { text: "MCP server not found", className: "bg-warn/12 text-warn", dot: "bg-warn" };
  const activity = overview?.activity ?? [];

  return (
    <section aria-label="AI agents" className="min-h-0 flex-1 overflow-y-auto px-7 pt-6 pb-8">
      <div className="max-w-[800px]">
        <div className="flex items-center gap-3">
          <h2 className="text-[20px] leading-[26px] font-semibold tracking-[-0.02em]">AI agents</h2>
          <span className={cx("inline-flex items-center gap-1.5 rounded-[5px] px-2 py-[3px] text-[12px] leading-none font-medium", status.className)}>
            <span className={cx("h-1.5 w-1.5 rounded-full", status.dot)} aria-hidden />
            {status.text}
          </span>
        </div>
        <p className="mt-2 max-w-[640px] text-[14px] leading-[21px] text-muted">
          After an agent builds or changes a service, it can register the endpoints here, attach tests and run them. Everything it creates is plain
          YAML in this workspace, so it shows up in this window right away and you review it in git like any other change.
        </p>
        {!info?.mcpCommand && (
          <p className="mt-3 rounded-lg bg-warn/10 px-3 py-2 text-[12.5px] text-warn">
            The bundled <span className="font-mono">courion-mcp</span> binary wasn&apos;t found next to the app. Build it with{" "}
            <span className="font-mono">cargo build -p trax-mcp --release</span> and put it on your PATH.
          </p>
        )}

        <div className="mt-6 flex items-center justify-between">
          <span className="text-[13px] font-semibold">Connect a client</span>
          <div role="group" aria-label="Client" className="segmented">
            {TABS.map((t) => (
              <button key={t.id} type="button" aria-pressed={t.id === client} onClick={() => setClient(t.id)}>
                {t.label}
              </button>
            ))}
          </div>
        </div>
        <div className="mt-2.5 overflow-hidden rounded-[10px] border border-border-mid bg-sunken">
          <div className="flex h-9 items-center justify-between border-b border-border pr-1.5 pl-3.5">
            <span className="text-[12.5px] text-muted">{tab.hint}</span>
            <CopyButton text={snippet} />
          </div>
          <pre className="overflow-x-auto px-4 py-3.5 font-mono text-[13px] leading-[21px] text-text">
            {tab.id === "code" ? (
              <>
                <span className="text-key">claude</span> mcp add courion -- <span className="text-[var(--syn-str)]">{quote(command)}</span> --workspace{" "}
                <span className="text-[var(--syn-str)]">{quote(wsPath)}</span>
              </>
            ) : (
              highlightJson(snippet)
            )}
          </pre>
        </div>
        <div className="mt-2.5 text-[12.5px] leading-[18px] text-faint">
          Leave out <span className="font-mono text-[12px] text-muted">--workspace</span> to use whichever workspace is active here. The agent also
          follows the environment you select, currently <span className="text-text-2">{env?.name ?? "none"}</span>.
        </div>

        <Heading>Server</Heading>
        <div className="mt-2.5 grid grid-cols-2 overflow-hidden rounded-[10px] border border-border-mid sm:grid-cols-4">
          {[
            { label: "Transport", value: <span className="font-mono">stdio</span> },
            { label: "Workspace", value: entry?.name ?? "None" },
            {
              label: "Environment",
              value: (
                <span className="flex items-center gap-[7px]">
                  <span className="h-[7px] w-[7px] rounded-full" style={{ background: envTone(env?.name) }} aria-hidden />
                  {env?.name ?? "No Environment"}
                </span>
              ),
            },
            { label: "Tools", value: `${TOOLS.length} available` },
          ].map((c, i) => (
            <div key={c.label} className={cx("min-w-0 px-3.5 py-3", i < 3 && "sm:border-r sm:border-border")}>
              <div className="text-[12px] text-faint">{c.label}</div>
              <div className="mt-1 truncate text-[13.5px]">{c.value}</div>
            </div>
          ))}
        </div>
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          {TOOLS.map((t) => (
            <span key={t} className="rounded-md border border-border bg-panel-2 px-2 py-1 font-mono text-[12px] leading-none text-key">
              {t}
            </span>
          ))}
        </div>

        <Heading>What agents can do</Heading>
        <p className="mt-1 text-[12.5px] text-faint">Applies to every workspace and every connected client, right away.</p>
        <div className="mt-1.5">
          {PERMISSIONS.map((p) => (
            <div key={p.key} className="flex min-h-[54px] items-center gap-3.5 border-b border-border-soft py-2">
              <div className="min-w-0 flex-1">
                <div className="text-[14px]">{p.label}</div>
                <div className="mt-0.5 text-[12.5px] text-faint">{p.desc}</div>
              </div>
              <Switch checked={overview?.permissions[p.key] ?? false} onChange={(on) => void setPermission(p.key, on)} label={p.label} />
            </div>
          ))}
        </div>

        <Heading
          right={
            <button type="button" className="btn h-7 px-2.5" onClick={() => setView("git")}>
              Review in Source control
            </button>
          }
        >
          Recent agent activity
        </Heading>
        <div className="mt-2.5 overflow-hidden rounded-[10px] border border-border-mid">
          {activity.length === 0 && (
            <div className="px-3.5 py-4 text-[13px] text-faint">Nothing yet. When an agent creates, changes or runs requests in this workspace, it shows up here.</div>
          )}
          {activity.slice(0, 30).map((a, i) => (
            <div key={i} className="flex min-h-11 items-center gap-3 border-b border-border px-3.5 py-2 last:border-b-0">
              <span className={cx("shrink-0 rounded px-[7px] py-0.5 text-[11.5px] leading-4 font-semibold", KIND[a.kind].className)}>{KIND[a.kind].label}</span>
              <span className="min-w-0 flex-1 truncate text-[13.5px] text-text-2" title={a.what}>
                {a.what}
              </span>
              <span className="shrink-0 text-[12px] whitespace-nowrap text-faint">
                {a.client} · {timeAgo(a.at)}
              </span>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
