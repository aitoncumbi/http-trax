"use client";

import { ChevronDown } from "lucide-react";

import type { Auth, AuthType, Request } from "@/lib/types";

import { GRANTS, OAuth2Fields, OAuthTokenPanel } from "./OAuthEditor";
import { VarInput } from "./VarInput";

const TYPES: { id: AuthType; label: string }[] = [
  { id: "inherit", label: "Inherit from parent" },
  { id: "none", label: "No auth" },
  { id: "bearer", label: "Bearer token" },
  { id: "basic", label: "Basic auth" },
  { id: "digest", label: "Digest auth" },
  { id: "api-key", label: "API key" },
  { id: "oauth2", label: "OAuth 2.0" },
  { id: "aws-sigv4", label: "AWS Signature v4" },
  { id: "ntlm", label: "NTLM" },
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
    case "digest":
      return { type, username: "", password: "" };
    case "api-key":
      return { type, key: "X-Api-Key", value: "", location: "header" };
    case "oauth2":
      return { type, grant: "authorization-code", authUrl: "", tokenUrl: "", clientId: "", scope: "" };
    case "aws-sigv4":
      return { type, accessKey: "", secretKey: "", region: "us-east-1", service: "execute-api" };
    case "ntlm":
      return { type, username: "", password: "", domain: "" };
  }
}

function describe(auth: Auth): string {
  switch (auth.type) {
    case "bearer":
      return "a bearer token";
    case "basic":
      return `basic auth as ${auth.username || "…"}`;
    case "digest":
      return `digest auth as ${auth.username || "…"}`;
    case "api-key":
      return `the API key ${auth.key} in the ${auth.location}`;
    case "oauth2":
      return `OAuth 2.0 (${GRANTS.find((g) => g.id === auth.grant)?.short ?? auth.grant})`;
    case "aws-sigv4":
      return `AWS SigV4 (${auth.service || "service"} in ${auth.region || "region"})`;
    case "ntlm":
      return `NTLM as ${auth.domain ? `${auth.domain}\\` : ""}${auth.username || "…"}`;
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
  oauthTarget,
}: {
  auth: Auth;
  onChange: (a: Auth) => void;
  /** Effective auth from the closest parent, shown when inheriting. */
  inherited?: { from: string; auth: Auth } | null;
  allowInherit?: boolean;
  /** Active environment name, for the "resolved from" hint. */
  environment?: string | null;
  /** What to get OAuth 2.0 tokens for: the request, or a stand-in for a collection or folder. */
  oauthTarget?: { request: Request; parentId: string | null } | null;
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
      {(auth.type === "basic" || auth.type === "digest") && (
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
      {auth.type === "aws-sigv4" && (
        <div className="flex max-w-[620px] flex-col gap-3">
          <div className="grid grid-cols-2 gap-3">
            <div className="flex flex-col gap-2">
              <span className="text-[13px] text-faint">Access Key ID</span>
              <div className={field}>
                <VarInput aria-label="Access Key ID" value={auth.accessKey} placeholder="{{awsAccessKeyId}}" onChange={(accessKey) => onChange({ ...auth, accessKey })} className="h-full px-0.5" />
              </div>
            </div>
            <div className="flex flex-col gap-2">
              <span className="text-[13px] text-faint">Secret Access Key</span>
              <div className={field}>
                <VarInput aria-label="Secret Access Key" value={auth.secretKey} placeholder="{{awsSecretAccessKey}}" onChange={(secretKey) => onChange({ ...auth, secretKey })} className="h-full px-0.5" />
              </div>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="flex flex-col gap-2">
              <span className="text-[13px] text-faint">AWS Region</span>
              <div className={field}>
                <VarInput aria-label="AWS Region" value={auth.region} placeholder="us-east-1" onChange={(region) => onChange({ ...auth, region })} className="h-full px-0.5" />
              </div>
            </div>
            <div className="flex flex-col gap-2">
              <span className="text-[13px] text-faint">Service Name</span>
              <div className={field}>
                <VarInput aria-label="Service Name" value={auth.service} placeholder="execute-api / s3" onChange={(service) => onChange({ ...auth, service })} className="h-full px-0.5" />
              </div>
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <span className="text-[13px] text-faint">Session Token (optional)</span>
            <div className={field}>
              <VarInput aria-label="Session Token" value={auth.sessionToken ?? ""} placeholder="{{awsSessionToken}}" onChange={(sessionToken) => onChange({ ...auth, sessionToken: sessionToken || undefined })} className="h-full px-0.5" />
            </div>
          </div>
        </div>
      )}
      {auth.type === "ntlm" && (
        <div className="grid max-w-[620px] grid-cols-3 gap-3">
          <div className="flex flex-col gap-2">
            <span className="text-[13px] text-faint">Username</span>
            <div className={field}>
              <VarInput aria-label="Username" value={auth.username} onChange={(username) => onChange({ ...auth, username })} className="h-full px-0.5" />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <label htmlFor="ntlm-password" className="text-[13px] text-faint">
              Password
            </label>
            <div className={field}>
              <input
                id="ntlm-password"
                type="password"
                className="h-full w-full bg-transparent px-3.5 outline-none placeholder:text-dim"
                value={auth.password}
                placeholder="{{password}}"
                onChange={(e) => onChange({ ...auth, password: e.target.value })}
              />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <span className="text-[13px] text-faint">Domain (optional)</span>
            <div className={field}>
              <VarInput aria-label="Domain" value={auth.domain ?? ""} placeholder="WORKGROUP" onChange={(domain) => onChange({ ...auth, domain: domain || undefined })} className="h-full px-0.5" />
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
      {auth.type === "oauth2" && <OAuth2Fields value={auth} onChange={(o) => onChange({ ...o, type: "oauth2" })} />}
      {oauthTarget && (auth.type === "oauth2" || (auth.type === "inherit" && inherited?.auth.type === "oauth2")) && (
        <OAuthTokenPanel target={oauthTarget} configKey={JSON.stringify(auth.type === "oauth2" ? auth : inherited?.auth)} />
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
