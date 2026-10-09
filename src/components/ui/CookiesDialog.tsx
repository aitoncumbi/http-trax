"use client";

import { ChevronDown, Cookie, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { api, errorMessage } from "@/lib/api";
import { cookieKey, expiryLabel, fromLocalInput, toLocalInput } from "@/lib/cookies";
import { useStore } from "@/lib/store";
import type { CookieInfo, CookieKey } from "@/lib/types";
import { cx } from "@/lib/util";

import { Checkbox, Switch } from "./Controls";
import { Modal } from "./Dialogs";

const blank = (domain: string): CookieInfo => ({
  name: "",
  value: "",
  domain,
  includeSubdomains: false,
  path: "/",
  expires: null,
  secure: false,
  httpOnly: false,
  sameSite: null,
});

const matchesDomain = (c: CookieInfo, d: string) => c.domain === d || c.domain.endsWith(`.${d}`);

function Flag({ children }: { children: React.ReactNode }) {
  return <span className="rounded-[5px] bg-selected px-1.5 py-px text-[11px] text-muted">{children}</span>;
}

/** Two clicks to run a destructive action; the second must come within 3 s. */
function ArmedButton({ label, armedLabel, onConfirm, className }: { label: string; armedLabel: string; onConfirm: () => void; className?: string }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(t);
  }, [armed]);
  return (
    <button
      type="button"
      className={cx(armed ? "btn-danger h-7" : "btn-ghost", className)}
      onClick={() => {
        if (armed) {
          setArmed(false);
          onConfirm();
        } else setArmed(true);
      }}
    >
      {armed ? armedLabel : label}
    </button>
  );
}

function CookieForm({
  initial,
  previous,
  onSave,
  onCancel,
}: {
  initial: CookieInfo;
  previous: CookieKey | null;
  onSave: (c: CookieInfo, previous: CookieKey | null) => Promise<string | null>;
  onCancel: () => void;
}) {
  const [c, setC] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);
  const set = (patch: Partial<CookieInfo>) => setC((x) => ({ ...x, ...patch }));
  const session = !c.expires;

  return (
    <form
      className="flex min-h-0 flex-1 flex-col"
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true);
        setError(await onSave(c, previous));
        setBusy(false);
      }}
    >
      <div className="min-h-0 flex-1 space-y-3.5 overflow-auto px-5 py-4">
        <div className="text-[14px] font-semibold">{previous ? `Edit ${previous.name}` : "Add a cookie"}</div>
        <div className="grid grid-cols-[1fr_1.6fr] gap-3">
          <div>
            <label className="label" htmlFor="ck-name">
              Name
            </label>
            <input id="ck-name" ref={first} className="input font-mono" value={c.name} onChange={(e) => set({ name: e.target.value })} spellCheck={false} />
          </div>
          <div>
            <label className="label" htmlFor="ck-value">
              Value
            </label>
            <input id="ck-value" className="input font-mono" value={c.value} onChange={(e) => set({ value: e.target.value })} spellCheck={false} />
          </div>
          <div>
            <label className="label" htmlFor="ck-domain">
              Domain
            </label>
            <input
              id="ck-domain"
              className="input font-mono"
              placeholder="api.example.com"
              value={c.domain}
              onChange={(e) => set({ domain: e.target.value })}
              spellCheck={false}
            />
          </div>
          <div>
            <label className="label" htmlFor="ck-path">
              Path
            </label>
            <input id="ck-path" className="input font-mono" value={c.path} onChange={(e) => set({ path: e.target.value })} spellCheck={false} />
          </div>
        </div>
        <div className="grid grid-cols-[1fr_1.6fr] gap-3">
          <div>
            <label className="label" htmlFor="ck-samesite">
              SameSite
            </label>
            <div className="relative">
              <select
                id="ck-samesite"
                value={c.sameSite ?? ""}
                onChange={(e) => set({ sameSite: e.target.value || null })}
                className="h-9 w-full appearance-none rounded-lg border border-border-strong bg-transparent pr-8 pl-3 text-[13.5px] text-text outline-none focus:border-accent-line"
              >
                <option value="">Not set</option>
                <option value="Lax">Lax</option>
                <option value="Strict">Strict</option>
                <option value="None">None</option>
              </select>
              <ChevronDown size={13} className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-muted" />
            </div>
          </div>
          <div>
            <label className="label" htmlFor="ck-expires">
              Expires
            </label>
            <div className="flex items-center gap-3">
              <input
                id="ck-expires"
                type="datetime-local"
                className="input min-w-0 flex-1 font-mono text-[13px] disabled:opacity-40"
                disabled={session}
                value={toLocalInput(c.expires)}
                onChange={(e) => set({ expires: fromLocalInput(e.target.value) })}
              />
              <label className="flex shrink-0 items-center gap-2 text-[13px] text-muted">
                <Checkbox
                  checked={session}
                  label="Session cookie"
                  size={16}
                  onChange={(v) => set({ expires: v ? null : new Date(Date.now() + 7 * 86_400_000).toISOString().replace(/\.\d+Z$/, "Z") })}
                />
                Session
              </label>
            </div>
          </div>
        </div>
        <div className="divide-y divide-border-soft rounded-[10px] border border-border-mid">
          {(
            [
              ["includeSubdomains", "Include subdomains", "Also send to every subdomain of the domain."],
              ["secure", "Secure", "Only send over HTTPS."],
              ["httpOnly", "HttpOnly", "Hidden from browser scripts. Requests here always send it."],
            ] as const
          ).map(([key, label, desc]) => (
            <div key={key} className="flex items-center gap-3 px-3.5 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="text-[13.5px]">{label}</div>
                <div className="text-[12px] text-faint">{desc}</div>
              </div>
              <Switch checked={c[key]} onChange={(v) => set({ [key]: v })} label={label} />
            </div>
          ))}
        </div>
        {error && <p className="text-[12.5px] text-danger">{error}</p>}
      </div>
      <div className="flex justify-end gap-2 border-t border-border px-5 py-3">
        <button type="button" className="btn" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="btn-primary" disabled={busy || !c.name.trim() || !c.domain.trim()}>
          {previous ? "Save" : "Add cookie"}
        </button>
      </div>
    </form>
  );
}

export function CookiesDialog({ domain: initialDomain }: { domain?: string }) {
  const close = useStore((s) => s.closeDialog);
  const toast = useStore((s) => s.toast);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const [cookies, setCookies] = useState<CookieInfo[] | null>(null);
  const [domain, setDomain] = useState<string | null>(initialDomain ?? null);
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<{ cookie: CookieInfo; previous: CookieKey | null } | null>(null);

  const [version, setVersion] = useState(0);
  const reload = () => setVersion((v) => v + 1);
  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    api
      .listCookies(workspaceId)
      .then((list) => !cancelled && setCookies(list))
      .catch((e) => {
        if (cancelled) return;
        toast("error", errorMessage(e));
        setCookies([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workspaceId, version, toast]);

  const domains = useMemo(() => {
    const counts = new Map<string, number>();
    for (const c of cookies ?? []) counts.set(c.domain, (counts.get(c.domain) ?? 0) + 1);
    if (initialDomain && !counts.has(initialDomain)) counts.set(initialDomain, 0);
    return [...counts.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [cookies, initialDomain]);

  const q = query.trim().toLowerCase();
  const shown = (cookies ?? []).filter(
    (c) =>
      (!domain || c.domain === domain) &&
      (!q || c.name.toLowerCase().includes(q) || c.domain.includes(q) || c.value.toLowerCase().includes(q)),
  );

  const remove = async (c: CookieInfo) => {
    if (!workspaceId) return;
    try {
      await api.deleteCookie(workspaceId, { domain: c.domain, path: c.path, name: c.name });
      reload();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  const clear = async (d: string | null) => {
    if (!workspaceId) return;
    try {
      const n = await api.clearCookies(workspaceId, d);
      toast("success", `Removed ${n} cookie${n === 1 ? "" : "s"}`);
      if (d) setDomain(null);
      reload();
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  const save = async (c: CookieInfo, previous: CookieKey | null): Promise<string | null> => {
    if (!workspaceId) return "No workspace is open.";
    try {
      const stored = await api.saveCookie(workspaceId, c, previous);
      setEditing(null);
      setDomain(stored.domain);
      reload();
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  };

  const total = cookies?.length ?? 0;
  const domainCount = domain ? (cookies ?? []).filter((c) => matchesDomain(c, domain)).length : total;

  return (
    <Modal title="Cookies" onClose={close} width={860}>
      <div className="flex h-[min(560px,70vh)] min-h-0">
        <nav aria-label="Domains" className="flex w-[230px] shrink-0 flex-col border-r border-border">
          <div className="p-3">
            <div className="relative">
              <Search size={13} className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-faint" />
              <input
                aria-label="Filter cookies"
                className="input h-8 pl-8 text-[13px]"
                placeholder="Filter"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
          </div>
          <div className="min-h-0 flex-1 overflow-auto px-2 pb-2">
            {[[null, total] as const, ...domains].map(([d, n]) => (
              <button
                key={d ?? "*"}
                type="button"
                aria-current={domain === d ? "true" : undefined}
                onClick={() => {
                  setDomain(d);
                  setEditing(null);
                }}
                className={cx(
                  "flex h-8 w-full items-center gap-2 rounded-[7px] px-2.5 text-left text-[13px]",
                  domain === d ? "bg-selected text-text" : "text-muted hover:bg-hover hover:text-text",
                )}
              >
                <span className={cx("min-w-0 flex-1 truncate", d && "font-mono text-[12.5px]")}>{d ?? "All domains"}</span>
                <span className="text-[11.5px] text-faint tabular-nums">{n}</span>
              </button>
            ))}
          </div>
        </nav>

        <div className="flex min-w-0 flex-1 flex-col">
          {editing ? (
            <CookieForm
              key={editing.previous ? cookieKey(editing.previous) : "new"}
              initial={editing.cookie}
              previous={editing.previous}
              onSave={save}
              onCancel={() => setEditing(null)}
            />
          ) : (
            <>
              <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
                <div className="min-w-0 flex-1 truncate text-[13.5px]">
                  {domain ? <span className="font-mono">{domain}</span> : "All domains"}
                  <span className="ml-2 text-faint">{shown.length}</span>
                </div>
                {domainCount > 0 && (
                  <ArmedButton
                    label={domain ? "Clear domain" : "Clear all"}
                    armedLabel={`Remove ${domainCount}?`}
                    onConfirm={() => void clear(domain)}
                  />
                )}
                <button type="button" className="btn h-7" onClick={() => setEditing({ cookie: blank(domain ?? ""), previous: null })}>
                  <Plus size={13} /> Add cookie
                </button>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                {cookies === null ? null : shown.length === 0 ? (
                  <div className="flex h-full flex-col items-center justify-center px-10 text-center">
                    <Cookie size={22} strokeWidth={1.4} className="text-faint" />
                    <div className="mt-3 text-[14px] font-medium">{q ? "No matching cookies" : "No cookies yet"}</div>
                    <p className="mt-1 max-w-[340px] text-[12.5px] leading-relaxed text-faint">
                      Cookies that responses set are kept here and sent with later requests to the same site, like a browser.
                    </p>
                  </div>
                ) : (
                  <ul className="divide-y divide-border-soft">
                    {shown.map((c) => {
                      const expiry = expiryLabel(c.expires);
                      return (
                        <li key={cookieKey(c)} className="group flex items-start gap-3 px-4 py-2.5 hover:bg-hover">
                          <div className="min-w-0 flex-1">
                            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                              <span className="font-mono text-[13px] text-key">{c.name}</span>
                              {!domain && <span className="font-mono text-[12px] text-faint">{c.domain}</span>}
                              {c.path !== "/" && <span className="font-mono text-[12px] text-faint">{c.path}</span>}
                              {c.includeSubdomains && <Flag>Subdomains</Flag>}
                              {c.httpOnly && <Flag>HttpOnly</Flag>}
                              {c.secure && <Flag>Secure</Flag>}
                              {c.sameSite && <Flag>SameSite={c.sameSite}</Flag>}
                            </div>
                            <div className="mt-0.5 truncate font-mono text-[12.5px] text-muted" title={c.value}>
                              {c.value || <span className="text-faint">empty</span>}
                            </div>
                          </div>
                          <span className={cx("shrink-0 pt-0.5 text-[12px]", expiry === "Session" ? "text-faint" : "text-muted")} title={c.expires ?? "Kept until cleared"}>
                            {expiry}
                          </span>
                          <div className="flex shrink-0 gap-0.5 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
                            <button
                              type="button"
                              className="icon-btn h-7 w-7"
                              aria-label={`Edit ${c.name}`}
                              onClick={() => setEditing({ cookie: c, previous: { domain: c.domain, path: c.path, name: c.name } })}
                            >
                              <Pencil size={13} strokeWidth={1.6} />
                            </button>
                            <button type="button" className="icon-btn h-7 w-7 hover:text-danger" aria-label={`Delete ${c.name}`} onClick={() => void remove(c)}>
                              <Trash2 size={13} strokeWidth={1.6} />
                            </button>
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
            </>
          )}
          <p className="shrink-0 border-t border-border px-4 py-2.5 text-[12px] text-faint">
            Kept on this computer in <span className="font-mono">.trax/cookies.json</span>, never committed. A request that sets its own Cookie header, or turns
            off the cookie jar in its settings, doesn&apos;t use these.
          </p>
        </div>
      </div>
    </Modal>
  );
}
