"use client";

import { ChevronDown } from "lucide-react";

import type { Auth, AuthType } from "@/lib/types";

import { VarInput } from "./VarInput";

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
      return "a bearer token";
    case "basic":
      return `basic auth as ${auth.username || "…"}`;
    case "api-key":
      return `the API key ${auth.key} in the ${auth.location}`;
    default:
      return "no auth";
  }
}

const field = "flex h-[42px] max-w-[460px] items-center rounded-[9px] border border-border-strong font-mono text-[13px] focus-within:border-accent-line";

export function AuthEditor({
  auth,
  onChange,
  inherited,
  allowInherit = true,
  environment,
}: {
  auth: Auth;
  onChange: (a: Auth) => void;
  /** Effective auth from the closest parent, shown when inheriting. */
  inherited?: { from: string; auth: Auth } | null;
  allowInherit?: boolean;
  /** Active environment name, for the "resolved from" hint. */
  environment?: string | null;
}) {
  const types = allowInherit ? TYPES : TYPES.filter((t) => t.id !== "inherit");
  const usesVars = JSON.stringify(auth).includes("{{");
  return (
    <div className="flex max-w-[720px] flex-col gap-[18px] p-5">
      <div className="flex flex-col gap-2">
        <label htmlFor="auth-type" className="text-[13px] text-faint">
          Type
        </label>
        <div className="relative w-[260px]">
          <select
            id="auth-type"
            value={auth.type}
            onChange={(e) => onChange(make(e.target.value as AuthType))}
            className="h-10 w-full appearance-none rounded-[9px] border border-border-strong bg-transparent pr-9 pl-3.5 text-[14px] text-text outline-none focus:border-accent-line"
          >
            {types.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </select>
          <ChevronDown size={13} className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-muted" />
        </div>
      </div>

      {auth.type === "inherit" && (
        <p className="text-[13.5px] leading-[20px] text-muted">
          {inherited ? (
            <>
              Uses {describe(inherited.auth)} from <span className="text-text">{inherited.from}</span>.
            </>
          ) : (
            "No folder or collection above sets auth, so none is sent."
          )}
        </p>
      )}
      {auth.type === "bearer" && (
        <div className="flex flex-col gap-2">
          <span className="text-[13px] text-faint">Token</span>
          <div className={field}>
            <VarInput aria-label="Token" value={auth.token} placeholder="{{apiToken}}" onChange={(token) => onChange({ ...auth, token })} className="h-full px-0.5" />
          </div>
        </div>
      )}
      {auth.type === "basic" && (
        <div className="grid max-w-[620px] grid-cols-2 gap-3">
          <div className="flex flex-col gap-2">
            <span className="text-[13px] text-faint">Username</span>
            <div className={field}>
              <VarInput aria-label="Username" value={auth.username} onChange={(username) => onChange({ ...auth, username })} className="h-full px-0.5" />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <label htmlFor="auth-password" className="text-[13px] text-faint">
              Password
            </label>
            <div className={field}>
              <input
                id="auth-password"
                type="password"
                className="h-full w-full bg-transparent px-3.5 outline-none placeholder:text-dim"
                value={auth.password}
                placeholder="{{password}}"
                onChange={(e) => onChange({ ...auth, password: e.target.value })}
              />
            </div>
          </div>
        </div>
      )}
      {auth.type === "api-key" && (
        <div className="grid max-w-[720px] grid-cols-[1fr_1fr_150px] gap-3">
          <div className="flex flex-col gap-2">
            <span className="text-[13px] text-faint">Key</span>
            <div className={field}>
              <VarInput aria-label="Key" value={auth.key} onChange={(key) => onChange({ ...auth, key })} className="h-full px-0.5" />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <span className="text-[13px] text-faint">Value</span>
            <div className={field}>
              <VarInput aria-label="Value" value={auth.value} placeholder="{{apiKey}}" onChange={(value) => onChange({ ...auth, value })} className="h-full px-0.5" />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <label htmlFor="auth-location" className="text-[13px] text-faint">
              Add to
            </label>
            <select
              id="auth-location"
              className="h-[42px] rounded-[9px] border border-border-strong bg-transparent px-3 text-[13.5px] outline-none focus:border-accent-line"
              value={auth.location}
              onChange={(e) => onChange({ ...auth, location: e.target.value as "header" | "query" })}
            >
              <option value="header">Header</option>
              <option value="query">Query params</option>
            </select>
          </div>
        </div>
      )}
      {auth.type !== "inherit" && auth.type !== "none" && (
        <p className="-mt-2 text-[12.5px] text-faint">
          {usesVars
            ? environment
              ? `Resolved from the ${environment} environment.`
              : "Choose an environment to resolve the variables."
            : "Tip: put secrets in an environment variable marked secret, so they never reach git."}
        </p>
      )}
    </div>
  );
}
