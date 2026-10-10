"use client";

import { AlertTriangle, ChevronDown, ChevronRight, Code2, Cookie, Copy, Loader2, Save, Send } from "lucide-react";
import { useMemo, useRef, useState } from "react";

import { tabDirty, useStore, type PaneLayout, type RequestTab } from "@/lib/store";
import type { Auth, Body, GroupNode, KeyValue, Request } from "@/lib/types";
import { hostOf } from "@/lib/cookies";
import { cx, findChain, METHODS, methodColor } from "@/lib/util";
import { resolveText } from "@/lib/variables";

import { AuthEditor } from "../editors/AuthEditor";
import { BodyEditor } from "../editors/BodyEditor";
import { KeyValueEditor, makeKv } from "../editors/KeyValueEditor";
import { ScriptsEditor } from "../editors/ScriptsEditor";
import { VarInput } from "../editors/VarInput";
import { useScopeFor, VarScopeContext } from "../editors/VarScope";
import { Switch } from "../ui/Controls";
import { MenuButton } from "../ui/Menu";
import { TabStrip } from "../ui/Segmented";
import { ResponseViewer } from "./ResponseViewer";

type Section = "params" | "body" | "headers" | "auth" | "scripts" | "settings" | "docs";

/** Draggable split between the request and response panes. */
function useSplit(layout: PaneLayout) {
  const [ratio, setRatio] = useState(0.52);
  const container = useRef<HTMLDivElement>(null);
  const onMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    const move = (ev: MouseEvent) => {
      const rect = container.current?.getBoundingClientRect();
      if (!rect) return;
      const r = layout === "side" ? (ev.clientX - rect.left) / rect.width : (ev.clientY - rect.top) / rect.height;
      setRatio(Math.min(0.8, Math.max(0.2, r)));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  return { ratio, container, onMouseDown };
}

function implicitHeaders(body: Body | undefined): string[] {
  switch (body?.mode) {
    case "json":
    case "graphql":
      return ["Content-Type: application/json"];
    case "xml":
      return ["Content-Type: application/xml"];
    case "text":
      return [`Content-Type: ${body.contentType || "text/plain"}`];
    case "form-urlencoded":
      return ["Content-Type: application/x-www-form-urlencoded"];
    case "multipart":
      return ["Content-Type: multipart/form-data"];
    default:
      return [];
  }
}

function inheritedAuth(chain: GroupNode[]): { from: string; auth: Auth } | null {
  for (let i = chain.length - 1; i >= 0; i--) {
    const a = chain[i].meta.auth;
    if (a && a.type !== "inherit") return { from: chain[i].meta.name, auth: a };
  }
  return null;
}

function LayoutToggle() {
  const layout = useStore((s) => s.layout);
  const setLayout = useStore((s) => s.setLayout);
  const btn = (id: PaneLayout, label: string, path: string) => (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={layout === id}
      onClick={() => setLayout(id)}
      className={cx("flex h-[26px] w-7 items-center justify-center rounded-[7px]", layout === id ? "bg-selected text-text" : "text-faint hover:text-text")}
    >
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" aria-hidden>
        <rect x="2.5" y="3" width="11" height="10" rx="2" />
        <path d={path} />
      </svg>
    </button>
  );
  return (
    <div className="flex gap-0.5 rounded-[9px] border border-border-mid p-0.5">
      {btn("stack", "Response below", "M2.5 8h11")}
      {btn("side", "Response beside", "M8 3v10")}
    </div>
  );
}

export function RequestView({ tab }: { tab: RequestTab }) {
  const tree = useStore((s) => s.tree);
  const layout = useStore((s) => s.layout);
  const envName = useStore((s) => {
    const id = s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment;
    return s.tree?.environments.find((e) => e.id === id)?.name ?? null;
  });
  const { updateTab, sendTab, saveTab, reloadTabFromDisk } = useStore.getState();
  const [section, setSection] = useState<Section>(() => (tab.draft.body?.mode && tab.draft.body.mode !== "none" ? "body" : "params"));
  const { ratio, container, onMouseDown } = useSplit(layout);
  const r = tab.draft;
  const dirty = tabDirty(tab);
  const chain = useMemo(() => findChain(tree?.collections ?? [], tab.requestId) ?? [], [tree, tab.requestId]);
  const scope = useScopeFor(chain[0]?.meta);

  const set = (patch: Partial<Request>) => updateTab<RequestTab>(tab.id, (t) => ({ ...t, draft: { ...t.draft, ...patch } }));

  const inheritedHeaders: (KeyValue & { from: string })[] = chain.flatMap((g) =>
    (g.meta.headers ?? []).filter((h) => h.enabled).map((h) => ({ ...h, from: g.meta.name })),
  );
  const auth = r.auth?.type === "inherit" ? inheritedAuth(chain)?.auth : r.auth;
  const authHeader = auth && auth.type !== "none" && auth.type !== "inherit" && !(auth.type === "api-key" && auth.location === "query");
  const autoHeaders = [
    ...inheritedHeaders.map((h) => `${h.key} (from ${h.from})`),
    ...implicitHeaders(r.body).map((h) => h.split(":")[0]),
    ...(authHeader ? [auth.type === "api-key" ? auth.key : "Authorization"] : []),
    "Host",
    "User-Agent",
  ];
  const paramCount = (r.params ?? []).filter((p) => p.enabled && p.key).length;
  const headerCount = (r.headers ?? []).filter((h) => h.enabled && h.key).length;
  const hasScripts = !!(r.scripts?.preRequest?.trim() || r.scripts?.postResponse?.trim());
  const side = layout === "side";

  return (
    <VarScopeContext.Provider value={scope}>
    <div className="flex min-h-0 flex-1 flex-col">
      {tab.diskVersion && (
        <div className="flex items-center gap-3 border-b border-warn/25 bg-warn/10 px-4 py-2 text-[13px] text-warn">
          <AlertTriangle size={14} />
          <span className="flex-1">This request changed on disk (a teammate&apos;s change was pulled, or an agent edited it) while you had unsaved edits.</span>
          <button type="button" className="btn h-7" onClick={() => reloadTabFromDisk(tab.id)}>
            Load theirs
          </button>
          <button type="button" className="btn h-7" onClick={() => updateTab<RequestTab>(tab.id, (t) => ({ ...t, diskVersion: null }))}>
            Keep mine
          </button>
        </div>
      )}

      {/* breadcrumb */}
      <div className="flex shrink-0 items-center gap-2.5 overflow-hidden px-4 pt-2.5 pb-2 text-[13px] whitespace-nowrap text-muted">
        {chain.map((g) => (
          <span key={g.meta.id} className="flex shrink-0 items-center gap-2.5">
            <span>{g.meta.name}</span>
            <ChevronRight size={11} strokeWidth={2} className="text-faint" />
          </span>
        ))}
        <input
          aria-label="Request name"
          value={r.name}
          onChange={(e) => set({ name: e.target.value })}
          className="-mx-1 min-w-0 flex-1 rounded px-1 text-text outline-none hover:bg-hover focus:bg-panel-2"
        />
      </div>

      {/* URL */}
      <div className="flex shrink-0 items-center gap-2.5 px-4 pb-3">
        <div className="flex h-9 min-w-0 flex-1 items-center rounded-lg border border-border-strong focus-within:border-accent-line">
          <MenuButton
            items={METHODS.map((m) => ({ label: m, onSelect: () => set({ method: m }) }))}
            className={cx(
              "flex h-full w-[88px] shrink-0 items-center justify-between rounded-l-lg border-r border-border-strong px-3 font-mono text-[13px] font-semibold",
              methodColor(r.method),
            )}
            title="HTTP method"
            minWidth={120}
          >
            <span>{r.method}</span>
            <ChevronDown size={12} strokeWidth={1.8} className="text-muted" />
          </MenuButton>
          <VarInput
            aria-label="URL"
            value={r.url}
            placeholder="{{baseUrl}}/path"
            onChange={(url) => set({ url })}
            onKeyDown={(e) => e.key === "Enter" && void sendTab(tab.id)}
            className="h-full font-mono text-[13px]"
          />
        </div>
        <button
          type="button"
          onClick={() => void sendTab(tab.id)}
          disabled={tab.running}
          title="Send (Ctrl+Enter)"
          className="inline-flex h-9 items-center gap-2 rounded-lg border border-accent bg-accent px-3.5 text-[13.5px] font-medium text-white hover:border-accent-strong hover:bg-accent-strong disabled:opacity-70"
        >
          {tab.running ? <Loader2 size={15} className="animate-spin" /> : <Send size={15} strokeWidth={1.8} />}
          Send
        </button>
        <button
          type="button"
          onClick={() => void saveTab(tab.id)}
          disabled={!dirty}
          title="Save (Ctrl+S)"
          className={cx(
            "inline-flex h-9 items-center gap-2 rounded-lg border px-3.5 text-[13.5px] font-medium",
            dirty ? "border-accent-line text-text hover:bg-hover" : "border-border-strong text-text hover:bg-hover",
          )}
        >
          <Save size={15} strokeWidth={1.4} />
          Save
        </button>
        <MenuButton
          title="Code and more"
          minWidth={200}
          className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-border-strong text-text-2 hover:bg-hover"
          items={[
            { label: "Code snippet…", icon: <Code2 size={14} />, onSelect: () => void useStore.getState().showCodeDialog({ tabId: tab.id }) },
            { label: "Copy as cURL", icon: <Copy size={14} />, onSelect: () => void useStore.getState().copyAsCurl({ tabId: tab.id }) },
            {
              label: "Cookies…",
              icon: <Cookie size={14} />,
              onSelect: () => useStore.getState().showCookies(hostOf(resolveText(r.url, scope)) ?? undefined),
            },
          ]}
        >
          <Code2 size={15} strokeWidth={1.5} />
        </MenuButton>
      </div>

      {/* panes */}
      <div ref={container} className={cx("flex min-h-0 flex-1 border-t border-border", side ? "flex-row" : "flex-col")}>
        <section aria-label="Request" className="flex min-h-0 min-w-0 flex-col" style={{ flex: `0 0 ${ratio * 100}%` }}>
          <TabStrip<Section>
            label="Request sections"
            value={section}
            onChange={setSection}
            right={<LayoutToggle />}
            tabs={[
              { id: "params", label: "Query params", badge: paramCount || undefined },
              { id: "body", label: "Body", badge: r.body?.mode && r.body.mode !== "none" ? r.body.mode : undefined },
              { id: "headers", label: "Headers", badge: headerCount || undefined },
              { id: "auth", label: "Authorization" },
              { id: "scripts", label: "Scripts", badge: hasScripts ? "•" : undefined, tone: "accent" },
              { id: "settings", label: "Settings" },
              { id: "docs", label: "Docs" },
            ]}
          />
          <div className="min-h-0 flex-1 overflow-auto">
            {section === "params" && (
              <div className="px-4 py-3">
                <KeyValueEditor rows={r.params ?? []} onChange={(params) => set({ params })} make={makeKv} describe />
              </div>
            )}
            {section === "headers" && (
              <div className="px-4 py-3">
                <KeyValueEditor rows={r.headers ?? []} onChange={(headers) => set({ headers })} make={makeKv} keyPlaceholder="Key" describe />
                <p className="mx-1 mt-3 text-[12.5px] leading-[18px] text-faint">
                  {autoHeaders.length} headers are added automatically: {autoHeaders.join(", ")}.
                </p>
              </div>
            )}
            {section === "body" && <BodyEditor body={r.body ?? { mode: "none" }} onChange={(body) => set({ body })} />}
            {section === "auth" && (
              <AuthEditor
                auth={r.auth ?? { type: "inherit" }}
                onChange={(a) => set({ auth: a })}
                inherited={inheritedAuth(chain)}
                environment={envName}
                oauthTarget={{ request: r, parentId: null }}
              />
            )}
            {section === "scripts" && (
              <ScriptsEditor
                scripts={r.scripts ?? {}}
                onChange={(scripts) => set({ scripts })}
                scopeNote={
                  chain.some((g) => g.meta.scripts?.preRequest || g.meta.scripts?.postResponse) ? "Collection and folder scripts run first." : undefined
                }
              />
            )}
            {section === "settings" && (
              <div className="px-5 py-2">
                {[
                  {
                    label: "Follow redirects",
                    desc: "Up to 10 hops before the request fails.",
                    on: r.settings?.followRedirects ?? true,
                    toggle: (v: boolean) => set({ settings: { verifyTls: true, ...r.settings, followRedirects: v } }),
                  },
                  {
                    label: "Verify TLS certificates",
                    desc: "Turn off only for local development with self-signed certificates.",
                    on: r.settings?.verifyTls ?? true,
                    toggle: (v: boolean) => set({ settings: { followRedirects: true, ...r.settings, verifyTls: v } }),
                  },
                  {
                    label: "Use the cookie jar",
                    desc: "Send cookies saved for this site and keep the ones the response sets.",
                    on: r.settings?.cookies ?? true,
                    toggle: (v: boolean) => set({ settings: { followRedirects: true, verifyTls: true, ...r.settings, cookies: v ? undefined : false } }),
                  },
                ].map((row) => (
                  <div key={row.label} className="flex min-h-[62px] items-center gap-3.5 border-b border-border-soft">
                    <div className="min-w-0 flex-1">
                      <div className="text-[14.5px]">{row.label}</div>
                      <div className="mt-0.5 text-[12.5px] text-faint">{row.desc}</div>
                    </div>
                    <Switch checked={row.on} onChange={row.toggle} label={row.label} />
                  </div>
                ))}
                <div className="flex min-h-[62px] items-center gap-3.5 border-b border-border-soft">
                  <div className="min-w-0 flex-1">
                    <label htmlFor="req-timeout" className="text-[14.5px]">
                      Timeout
                    </label>
                    <div className="mt-0.5 text-[12.5px] text-faint">Give up after this many milliseconds. Defaults to 30 000.</div>
                  </div>
                  <input
                    id="req-timeout"
                    type="number"
                    min={1}
                    placeholder="30000"
                    value={r.settings?.timeoutMs ?? ""}
                    onChange={(e) =>
                      set({ settings: { followRedirects: true, verifyTls: true, ...r.settings, timeoutMs: e.target.value ? Number(e.target.value) : undefined } })
                    }
                    className="input h-8 w-28 text-right font-mono text-[13px]"
                  />
                </div>
                <div className="flex flex-col gap-3.5 py-4">
                  <div className="flex flex-col gap-1.5">
                    <label htmlFor="req-proxy" className="text-[14px]">
                      Proxy URL
                    </label>
                    <div className="text-[12px] text-faint">Route this request through an HTTP, HTTPS or SOCKS5 proxy (supports {"{{variables}}"}).</div>
                    <input
                      id="req-proxy"
                      placeholder="http://127.0.0.1:8080 or socks5://127.0.0.1:1080"
                      value={r.settings?.proxyUrl ?? ""}
                      onChange={(e) =>
                        set({ settings: { followRedirects: true, verifyTls: true, ...r.settings, proxyUrl: e.target.value || undefined } })
                      }
                      className="input h-9 max-w-[520px] font-mono text-[13px]"
                    />
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <label htmlFor="req-ca-cert" className="text-[14px]">
                      Custom CA certificate
                    </label>
                    <div className="text-[12px] text-faint">File path or inline PEM bundle to trust for internal or self-signed roots.</div>
                    <textarea
                      id="req-ca-cert"
                      placeholder="/path/to/ca-bundle.pem or -----BEGIN CERTIFICATE-----"
                      value={r.settings?.caCert ?? ""}
                      onChange={(e) =>
                        set({ settings: { followRedirects: true, verifyTls: true, ...r.settings, caCert: e.target.value || undefined } })
                      }
                      className="input min-h-[60px] max-w-[520px] py-1.5 font-mono text-[12px]"
                    />
                  </div>
                  <div className="grid max-w-[520px] grid-cols-2 gap-3">
                    <div className="flex flex-col gap-1.5">
                      <label htmlFor="req-client-cert" className="text-[14px]">
                        Client certificate (mTLS)
                      </label>
                      <div className="text-[12px] text-faint">PEM path or inline certificate.</div>
                      <textarea
                        id="req-client-cert"
                        placeholder="/path/to/client.crt or -----BEGIN CERTIFICATE-----"
                        value={r.settings?.clientCert ?? ""}
                        onChange={(e) =>
                          set({ settings: { followRedirects: true, verifyTls: true, ...r.settings, clientCert: e.target.value || undefined } })
                        }
                        className="input min-h-[68px] py-1.5 font-mono text-[12px]"
                      />
                    </div>
                    <div className="flex flex-col gap-1.5">
                      <label htmlFor="req-client-key" className="text-[14px]">
                        Client private key (mTLS)
                      </label>
                      <div className="text-[12px] text-faint">PEM path or inline private key.</div>
                      <textarea
                        id="req-client-key"
                        placeholder="/path/to/client.key or -----BEGIN PRIVATE KEY-----"
                        value={r.settings?.clientKey ?? ""}
                        onChange={(e) =>
                          set({ settings: { followRedirects: true, verifyTls: true, ...r.settings, clientKey: e.target.value || undefined } })
                        }
                        className="input min-h-[68px] py-1.5 font-mono text-[12px]"
                      />
                    </div>
                  </div>
                </div>
              </div>
            )}
            {section === "docs" && (
              <div className="flex h-full max-w-[640px] flex-col p-5">
                <div className="mb-2 text-[17px] font-semibold tracking-[-0.01em]">{r.name}</div>
                <textarea
                  aria-label="Documentation"
                  placeholder="Describe what this request does, what it expects and what it returns."
                  value={r.description ?? ""}
                  onChange={(e) => set({ description: e.target.value })}
                  className="min-h-[160px] flex-1 resize-none bg-transparent text-[14px] leading-[22px] text-muted outline-none placeholder:text-dim"
                />
              </div>
            )}
          </div>
        </section>

        <div
          role="separator"
          aria-orientation={side ? "vertical" : "horizontal"}
          aria-label="Resize panes"
          onMouseDown={onMouseDown}
          className={cx("shrink-0 bg-border hover:bg-accent-line", side ? "w-px cursor-col-resize" : "h-px cursor-row-resize")}
        />

        <section aria-label="Response" className="flex min-h-0 min-w-0 flex-1 flex-col bg-sunken">
          <ResponseViewer tab={tab} />
        </section>
      </div>
    </div>
    </VarScopeContext.Provider>
  );
}
