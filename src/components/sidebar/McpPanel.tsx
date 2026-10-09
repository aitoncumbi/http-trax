"use client";

import { Bot, Check, Copy } from "lucide-react";
import { useState } from "react";

import { useStore } from "@/lib/store";
import { cx } from "@/lib/util";

const TOOLS: [string, string][] = [
  ["init_workspace", "create/open a workspace (e.g. in the service repo)"],
  ["upsert_environment", "set baseUrl and other variables"],
  ["upsert_collection", "shared headers, auth, variables, scripts"],
  ["upsert_request", "add or update an endpoint with tests"],
  ["run_request", "send it, get status, tests, body"],
  ["run_collection", "run everything, report failures"],
  ["send_http_request", "one-off call, not saved"],
  ["get_workspace / get_request", "read what exists"],
  ["list_saved_responses", "known-good examples"],
];

function CopyBlock({ label, text }: { label: string; text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="mb-3">
      <div className="mb-1 flex items-center justify-between">
        <span className="text-[11.5px] font-medium text-muted">{label}</span>
        <button
          className={cx("btn-ghost h-6", copied && "text-ok")}
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(text);
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            } catch {
              // clipboard unavailable
            }
          }}
        >
          {copied ? <Check size={12} /> : <Copy size={12} />} {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="overflow-x-auto rounded-md border border-border bg-panel-2 p-2.5 font-mono text-[11px] leading-relaxed whitespace-pre text-text">
        {text}
      </pre>
    </div>
  );
}

export function McpPanel() {
  const info = useStore((s) => s.info);
  const entry = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId) ?? null);
  const command = info?.mcpCommand ?? "trax-mcp";
  const quote = (s: string) => (/\s/.test(s) ? `"${s}"` : s);
  const wsArg = entry ? entry.path : "./api-tests";

  const claudeCode = `claude mcp add http-trax -- ${quote(command)} --workspace ${quote(wsArg)}`;
  const json = JSON.stringify(
    { mcpServers: { "http-trax": { command, args: ["--workspace", wsArg] } } },
    null,
    2,
  );
  const vscode = JSON.stringify(
    { servers: { "http-trax": { type: "stdio", command, args: ["--workspace", wsArg] } } },
    null,
    2,
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-3 pt-3 pb-6">
      <h2 className="mb-3 text-[11px] font-semibold tracking-wider text-faint uppercase">AI agents (MCP)</h2>
      <div className="mb-4 flex gap-2.5 rounded-lg border border-border bg-panel-2 p-3">
        <Bot size={18} className="mt-0.5 shrink-0 text-accent" />
        <p className="text-[12.5px] leading-relaxed text-muted">
          After an agent builds or changes a service, it can register the endpoints here, attach tests and run them. Everything it creates is
          plain YAML in this workspace, so it shows up in this window right away and you review it in git like any other change.
        </p>
      </div>

      {!info?.mcpCommand && (
        <p className="mb-3 rounded-md bg-warn/10 px-2.5 py-2 text-[12px] text-warn">
          The bundled <code className="font-mono">trax-mcp</code> binary wasn&apos;t found next to the app. Build it with{" "}
          <code className="font-mono">cargo build -p trax-mcp --release</code> and put it on your PATH.
        </p>
      )}

      <CopyBlock label="Claude Code" text={claudeCode} />
      <CopyBlock label="Claude Desktop / Cursor / Windsurf (mcp.json)" text={json} />
      <CopyBlock label="VS Code (.vscode/mcp.json)" text={vscode} />

      <p className="mb-2 text-[11.5px] text-faint">
        Leave out <code className="font-mono">--workspace</code> to use whichever workspace is active here. The agent also follows the
        environment you select in the top bar.
      </p>

      <h3 className="mt-3 mb-1.5 text-[11px] font-semibold tracking-wider text-faint uppercase">Tools</h3>
      <ul className="space-y-1">
        {TOOLS.map(([name, desc]) => (
          <li key={name} className="text-[12px]">
            <code className="font-mono text-accent">{name}</code> <span className="text-muted">— {desc}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
