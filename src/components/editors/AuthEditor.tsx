"use client";

import type { Auth, AuthType } from "@/lib/types";

const TYPES: { id: AuthType; label: string }[] = [
  { id: "inherit", label: "Inherit from parent" },
  { id: "none", label: "No auth" },
  { id: "bearer", label: "Bearer token" },
  { id: "basic", label: "Basic auth" },
  { id: "api-key", label: "API key" },
];

function make(type: AuthType): Auth {
  switch (type) {
    case "inherit":
    case "none":
      return { type };
    case "bearer":
      return { type, token: "" };
    case "basic":
      return { type, username: "", password: "" };
    case "api-key":
      return { type, key: "X-Api-Key", value: "", location: "header" };
  }
}

function describe(auth: Auth): string {
  switch (auth.type) {
    case "bearer":
      return `Bearer token ${auth.token ? `(${auth.token})` : ""}`;
    case "basic":
      return `Basic auth as ${auth.username || "…"}`;
    case "api-key":
      return `API key ${auth.key} in ${auth.location}`;
    case "none":
      return "No auth";
    default:
      return "No auth";
  }
}

export function AuthEditor({
  auth,
  onChange,
  inherited,
  allowInherit = true,
}: {
  auth: Auth;
  onChange: (a: Auth) => void;
  /** Effective auth from the closest parent, shown when inheriting. */
  inherited?: { from: string; auth: Auth } | null;
  allowInherit?: boolean;
}) {
  const types = allowInherit ? TYPES : TYPES.filter((t) => t.id !== "inherit");
  return (
    <div className="max-w-[640px] space-y-3 p-4">
      <div>
        <label className="label">Type</label>
        <select className="input w-64" value={auth.type} onChange={(e) => onChange(make(e.target.value as AuthType))}>
          {types.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      </div>
      {auth.type === "inherit" && (
        <p className="rounded-md border border-border bg-panel-2 px-3 py-2 text-[12.5px] text-muted">
          {inherited ? (
            <>
              Uses <strong className="text-text">{describe(inherited.auth)}</strong> from <strong className="text-text">{inherited.from}</strong>.
            </>
          ) : (
            "No parent sets auth, so none is sent."
          )}
        </p>
      )}
      {auth.type === "bearer" && (
        <div>
          <label className="label">Token</label>
          <input className="input font-mono text-[12px]" value={auth.token} placeholder="{{token}}" onChange={(e) => onChange({ ...auth, token: e.target.value })} />
        </div>
      )}
      {auth.type === "basic" && (
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="label">Username</label>
            <input className="input font-mono text-[12px]" value={auth.username} onChange={(e) => onChange({ ...auth, username: e.target.value })} />
          </div>
          <div>
            <label className="label">Password</label>
            <input className="input font-mono text-[12px]" type="password" value={auth.password} placeholder="{{password}}" onChange={(e) => onChange({ ...auth, password: e.target.value })} />
          </div>
        </div>
      )}
      {auth.type === "api-key" && (
        <div className="grid grid-cols-[1fr_1fr_140px] gap-3">
          <div>
            <label className="label">Key</label>
            <input className="input font-mono text-[12px]" value={auth.key} onChange={(e) => onChange({ ...auth, key: e.target.value })} />
          </div>
          <div>
            <label className="label">Value</label>
            <input className="input font-mono text-[12px]" value={auth.value} placeholder="{{apiKey}}" onChange={(e) => onChange({ ...auth, value: e.target.value })} />
          </div>
          <div>
            <label className="label">Add to</label>
            <select className="input" value={auth.location} onChange={(e) => onChange({ ...auth, location: e.target.value as "header" | "query" })}>
              <option value="header">Header</option>
              <option value="query">Query params</option>
            </select>
          </div>
        </div>
      )}
      {auth.type !== "inherit" && auth.type !== "none" && (
        <p className="text-[11.5px] text-faint">
          Tip: reference secrets as <code className="font-mono">{"{{variables}}"}</code> and mark them secret in the environment so they never reach git.
        </p>
      )}
    </div>
  );
}
