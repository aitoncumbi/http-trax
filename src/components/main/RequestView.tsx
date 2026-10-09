"use client";

import { AlertTriangle, ChevronDown, Loader2, Save, Send } from "lucide-react";
import { useMemo, useRef, useState } from "react";

import { tabDirty, useStore, type RequestTab } from "@/lib/store";
import type { Auth, Body, GroupNode, KeyValue, Request } from "@/lib/types";
import { cx, findChain, METHODS, methodColor } from "@/lib/util";

import { AuthEditor } from "../editors/AuthEditor";
import { BodyEditor } from "../editors/BodyEditor";
import { KeyValueEditor, makeKv } from "../editors/KeyValueEditor";
import { ScriptsEditor } from "../editors/ScriptsEditor";
import { MenuButton } from "../ui/Menu";
import { TabStrip } from "../ui/Segmented";
import { ResponseViewer } from "./ResponseViewer";

type Section = "params" | "headers" | "body" | "auth" | "scripts" | "settings" | "docs";

function useSplit(initial = 0.48) {
  const [ratio, setRatio] = useState(initial);
  const container = useRef<HTMLDivElement>(null);
  const onMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    const move = (ev: MouseEvent) => {
      const rect = container.current?.getBoundingClientRect();
      if (!rect) return;
      setRatio(Math.min(0.85, Math.max(0.15, (ev.clientY - rect.top) / rect.height)));
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
      return ["Content-Type: multipart/form-data; boundary=…"];
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

export function RequestView({ tab }: { tab: RequestTab }) {
  const tree = useStore((s) => s.tree);
  const { updateTab, sendTab, saveTab, reloadTabFromDisk } = useStore.getState();
  const [section, setSection] = useState<Section>(() => (tab.draft.body?.mode && tab.draft.body.mode !== "none" ? "body" : "params"));
  const { ratio, container, onMouseDown } = useSplit();
  const r = tab.draft;
  const dirty = tabDirty(tab);
  const chain = useMemo(() => findChain(tree?.collections ?? [], tab.requestId) ?? [], [tree, tab.requestId]);

  const set = (patch: Partial<Request>) => updateTab<RequestTab>(tab.id, (t) => ({ ...t, draft: { ...t.draft, ...patch } }));

  const inheritedHeaders: (KeyValue & { from: string })[] = chain.flatMap((g) =>
    (g.meta.headers ?? []).filter((h) => h.enabled).map((h) => ({ ...h, from: g.meta.name })),
  );

  const counts = {
    params: (r.params ?? []).filter((p) => p.enabled && p.key).length,
    headers: (r.headers ?? []).filter((h) => h.enabled && h.key).length,
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {tab.diskVersion && (
        <div className="flex items-center gap-2 border-b border-warn/30 bg-warn/10 px-4 py-2 text-[12.5px] text-warn">
          <AlertTriangle size={14} />
          <span className="flex-1">This request changed on disk (pulled from your team, or edited by an agent) while you had unsaved edits.</span>
          <button className="btn h-7 text-[12px]" onClick={() => reloadTabFromDisk(tab.id)}>
            Load theirs
          </button>
          <button className="btn h-7 text-[12px]" onClick={() => updateTab<RequestTab>(tab.id, (t) => ({ ...t, diskVersion: null }))}>
            Keep mine
          </button>
        </div>
      )}

      {/* Breadcrumb + name */}
      <div className="flex items-center gap-1.5 px-4 pt-3 text-[12px] text-faint">
        {chain.map((g) => (
          <span key={g.meta.id} className="flex items-center gap-1.5">
            <span className="truncate">{g.meta.name}</span>
            <span>/</span>
          </span>
        ))}
        <input
          className="min-w-0 flex-1 rounded bg-transparent px-1 py-0.5 text-[13px] font-medium text-text outline-none hover:bg-hover focus:bg-panel-2"
          value={r.name}
          onChange={(e) => set({ name: e.target.value })}
          aria-label="Request name"
        />
      </div>

      {/* URL bar */}
      <div className="flex items-center gap-2 px-4 py-2.5">
        <div className="flex h-9 min-w-0 flex-1 items-center rounded-lg border border-border-strong bg-panel-2 focus-within:border-accent">
          <MenuButton
            items={METHODS.map((m) => ({ label: m, onSelect: () => set({ method: m }) }))}
            className={cx("flex h-full w-[104px] shrink-0 items-center justify-between gap-1 border-r border-border px-3 font-mono text-[12px] font-bold", methodColor(r.method))}
            title="Method"
            minWidth={120}
          >
            {r.method}
            <ChevronDown size={13} className="text-faint" />
          </MenuButton>
          <input
            className="h-full min-w-0 flex-1 bg-transparent px-3 font-mono text-[12.5px] outline-none placeholder:text-faint"
            placeholder="{{baseUrl}}/path"
            value={r.url}
            spellCheck={false}
            onChange={(e) => set({ url: e.target.value })}
            onKeyDown={(e) => e.key === "Enter" && void sendTab(tab.id)}
            aria-label="URL"
          />
        </div>
        <button className="btn-primary h-9 px-5" onClick={() => void sendTab(tab.id)} disabled={tab.running} title="Send (Ctrl+Enter)">
          {tab.running ? <Loader2 size={14} className="animate-spin" /> : <Send size={14} />}
          Send
        </button>
        <button className={cx("btn h-9", dirty && "border-accent text-accent")} onClick={() => void saveTab(tab.id)} disabled={!dirty} title="Save (Ctrl+S)">
          <Save size={14} /> Save
        </button>
      </div>

      <div ref={container} className="flex min-h-0 flex-1 flex-col">
        <div className="flex min-h-0 flex-col" style={{ flexBasis: `${ratio * 100}%`, flexGrow: 0, flexShrink: 0 }}>
          <TabStrip<Section>
            value={section}
            onChange={setSection}
            tabs={[
              { id: "params", label: "Params", badge: counts.params || undefined },
              { id: "headers", label: "Headers", badge: counts.headers || undefined },
              { id: "body", label: "Body", badge: r.body?.mode && r.body.mode !== "none" ? r.body.mode : undefined },
              { id: "auth", label: "Auth", badge: r.auth?.type && r.auth.type !== "inherit" ? r.auth.type : undefined },
              { id: "scripts", label: "Scripts", badge: r.scripts?.preRequest?.trim() || r.scripts?.postResponse?.trim() ? "●" : undefined },
              { id: "settings", label: "Settings" },
              { id: "docs", label: "Docs" },
            ]}
          />
          <div className="min-h-0 flex-1 overflow-auto">
            {section === "params" && (
              <div className="p-3">
                <KeyValueEditor rows={r.params ?? []} onChange={(params) => set({ params })} make={makeKv} />
                <p className="mt-2 text-[11.5px] text-faint">Appended to the URL as query parameters. Values support {"{{variables}}"}.</p>
              </div>
            )}
            {section === "headers" && (
              <div className="p-3">
                <KeyValueEditor rows={r.headers ?? []} onChange={(headers) => set({ headers })} make={makeKv} keyPlaceholder="Header" />
                {(inheritedHeaders.length > 0 || implicitHeaders(r.body).length > 0) && (
                  <div className="mt-3">
                    <div className="label">Also sent</div>
                    <div className="space-y-0.5 font-mono text-[11.5px] text-muted">
                      {inheritedHeaders.map((h) => (
                        <div key={`${h.from}-${h.key}`}>
                          {h.key}: {h.value} <span className="font-sans text-faint">— from {h.from}</span>
                        </div>
                      ))}
                      {implicitHeaders(r.body).map((h) => (
                        <div key={h}>
                          {h} <span className="font-sans text-faint">— from body (unless set above)</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
            {section === "body" && <BodyEditor body={r.body ?? { mode: "none" }} onChange={(body) => set({ body })} />}
            {section === "auth" && <AuthEditor auth={r.auth ?? { type: "inherit" }} onChange={(auth) => set({ auth })} inherited={inheritedAuth(chain)} />}
            {section === "scripts" && (
              <ScriptsEditor
                scripts={r.scripts ?? {}}
                onChange={(scripts) => set({ scripts })}
                scopeNote={chain.some((g) => g.meta.scripts?.preRequest || g.meta.scripts?.postResponse) ? "Collection/folder scripts run first." : undefined}
              />
            )}
            {section === "settings" && (
              <div className="max-w-[520px] space-y-3 p-4">
                <div>
                  <label className="label">Timeout (ms)</label>
                  <input
                    className="input w-48"
                    type="number"
                    min={1}
                    placeholder="30000"
                    value={r.settings?.timeoutMs ?? ""}
                    onChange={(e) =>
                      set({ settings: { followRedirects: true, verifyTls: true, ...r.settings, timeoutMs: e.target.value ? Number(e.target.value) : undefined } })
                    }
                  />
                </div>
                <label className="flex items-center gap-2 text-[12.5px]">
                  <input
                    type="checkbox"
                    className="accent-[var(--accent)]"
                    checked={r.settings?.followRedirects ?? true}
                    onChange={(e) => set({ settings: { verifyTls: true, ...r.settings, followRedirects: e.target.checked } })}
                  />
                  Follow redirects
                </label>
                <label className="flex items-center gap-2 text-[12.5px]">
                  <input
                    type="checkbox"
                    className="accent-[var(--accent)]"
                    checked={r.settings?.verifyTls ?? true}
                    onChange={(e) => set({ settings: { followRedirects: true, ...r.settings, verifyTls: e.target.checked } })}
                  />
                  Verify TLS certificates
                </label>
              </div>
            )}
            {section === "docs" && (
              <div className="h-full p-3">
                <textarea
                  className="input h-full min-h-[120px] resize-none py-2"
                  placeholder="Describe what this request does, expected responses, gotchas…"
                  value={r.description ?? ""}
                  onChange={(e) => set({ description: e.target.value })}
                />
              </div>
            )}
          </div>
        </div>
        <div role="separator" aria-orientation="horizontal" onMouseDown={onMouseDown} className="h-[5px] shrink-0 cursor-row-resize border-t border-border hover:bg-accent/40" />
        <div className="flex min-h-0 flex-1 flex-col">
          <ResponseViewer tab={tab} />
        </div>
      </div>
    </div>
  );
}
