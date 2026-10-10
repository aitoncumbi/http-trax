"use client";

import { Check, ChevronDown, ChevronRight, Copy, ExternalLink, Eye, EyeOff, KeyRound, Loader2, RefreshCw, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { api, errorMessage, onEvent, openExternal } from "@/lib/api";
import { expiryLabel } from "@/lib/cookies";
import { useStore } from "@/lib/store";
import type { OAuth2, OAuth2Grant, OAuthPrompt, OAuthTokenStatus, Request } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

import { Switch } from "../ui/Controls";
import { KeyValueEditor, makeKv } from "./KeyValueEditor";
import { VarInput } from "./VarInput";

export const GRANTS: { id: OAuth2Grant; label: string; short: string; hint: string }[] = [
  {
    id: "authorization-code",
    label: "Authorization code (PKCE)",
    short: "authorization code",
    hint: "You sign in in the browser once; the token is refreshed after that.",
  },
  { id: "client-credentials", label: "Client credentials", short: "client credentials", hint: "Service to service. Tokens are fetched and renewed automatically." },
  { id: "device-code", label: "Device code", short: "device code", hint: "You enter a code on the provider's page, from any device." },
  { id: "password", label: "Password", short: "password grant", hint: "Legacy grant for trusted internal APIs. Tokens are fetched automatically." },
];

const field = "flex h-[42px] items-center rounded-[9px] border border-border-strong font-mono text-[13px] focus-within:border-accent-line";

function Field({ label, children, className }: { label: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={cx("flex min-w-0 flex-col gap-2", className)}>
      <span className="text-[13px] text-faint">{label}</span>
      {children}
    </div>
  );
}

function TextField({ label, value, onChange, placeholder }: { label: string; value: string | undefined; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <Field label={label}>
      <div className={field}>
        <VarInput aria-label={label} value={value ?? ""} placeholder={placeholder} onChange={onChange} className="h-full px-0.5" />
      </div>
    </Field>
  );
}

/** The settings of an OAuth 2.0 auth. */
export function OAuth2Fields({ value, onChange }: { value: OAuth2; onChange: (o: OAuth2) => void }) {
  const [advanced, setAdvanced] = useState(false);
  const set = (patch: Partial<OAuth2>) => onChange({ ...value, ...patch });
  const g = value.grant;
  const grant = GRANTS.find((x) => x.id === g);

  return (
    <div className="flex max-w-[720px] flex-col gap-4">
      <Field label="Grant type">
        <div className="relative w-[260px]">
          <select
            aria-label="Grant type"
            value={g}
            onChange={(e) => set({ grant: e.target.value as OAuth2Grant })}
            className="h-10 w-full appearance-none rounded-[9px] border border-border-strong bg-transparent pr-9 pl-3.5 text-[14px] text-text outline-none focus:border-accent-line"
          >
            {GRANTS.map((x) => (
              <option key={x.id} value={x.id}>
                {x.label}
              </option>
            ))}
          </select>
          <ChevronDown size={13} className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-muted" />
        </div>
        {grant && <span className="text-[12.5px] text-faint">{grant.hint}</span>}
      </Field>

      {g === "authorization-code" && (
        <TextField label="Authorization URL" value={value.authUrl} onChange={(authUrl) => set({ authUrl })} placeholder="https://id.example.com/oauth2/authorize" />
      )}
      {g === "device-code" && (
        <TextField
          label="Device authorization URL"
          value={value.deviceAuthUrl}
          onChange={(deviceAuthUrl) => set({ deviceAuthUrl })}
          placeholder="https://id.example.com/oauth2/device/code"
        />
      )}
      <TextField label="Token URL" value={value.tokenUrl} onChange={(tokenUrl) => set({ tokenUrl })} placeholder="https://id.example.com/oauth2/token" />
      <div className="grid grid-cols-2 gap-3">
        <TextField label="Client ID" value={value.clientId} onChange={(clientId) => set({ clientId })} placeholder="{{clientId}}" />
        <TextField
          label={g === "client-credentials" ? "Client secret" : "Client secret (if confidential)"}
          value={value.clientSecret}
          onChange={(clientSecret) => set({ clientSecret })}
          placeholder="{{clientSecret}}"
        />
      </div>
      {g === "password" && (
        <div className="grid grid-cols-2 gap-3">
          <TextField label="Username" value={value.username} onChange={(username) => set({ username })} />
          <TextField label="Password" value={value.password} onChange={(password) => set({ password })} placeholder="{{password}}" />
        </div>
      )}
      <div className={cx("grid gap-3", g === "authorization-code" ? "grid-cols-2" : "grid-cols-1")}>
        <TextField label="Scope" value={value.scope} onChange={(scope) => set({ scope })} placeholder="openid profile" />
        {g === "authorization-code" && (
          <TextField
            label="Redirect URI"
            value={value.redirectUri}
            onChange={(redirectUri) => set({ redirectUri })}
            placeholder="http://127.0.0.1:<any free port>/callback"
          />
        )}
      </div>

      <button
        type="button"
        aria-expanded={advanced}
        onClick={() => setAdvanced((v) => !v)}
        className="-mb-1 flex w-fit items-center gap-1.5 text-[13px] text-muted hover:text-text"
      >
        {advanced ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        Advanced
      </button>
      {advanced && (
        <div className="flex flex-col gap-4">
          <div className="divide-y divide-border-soft rounded-[10px] border border-border-mid">
            {g === "authorization-code" && (
              <div className="flex items-center gap-3 px-3.5 py-2.5">
                <div className="min-w-0 flex-1">
                  <div className="text-[13.5px]">Use PKCE</div>
                  <div className="text-[12px] text-faint">Proof key for code exchange (S256). Leave on unless the provider rejects it.</div>
                </div>
                <Switch checked={value.pkce ?? true} onChange={(pkce) => set({ pkce: pkce ? undefined : false })} label="Use PKCE" />
              </div>
            )}
            <div className="flex items-center gap-3 px-3.5 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="text-[13.5px]">Send client credentials</div>
                <div className="text-[12px] text-faint">How the client id and secret reach the token endpoint.</div>
              </div>
              <div role="group" aria-label="Send client credentials" className="segmented">
                <button type="button" aria-pressed={(value.clientAuth ?? "basic") === "basic"} onClick={() => set({ clientAuth: undefined })}>
                  Basic auth header
                </button>
                <button type="button" aria-pressed={value.clientAuth === "body"} onClick={() => set({ clientAuth: "body" })}>
                  In the body
                </button>
                <button type="button" aria-pressed={value.clientAuth === "private-key-jwt"} onClick={() => set({ clientAuth: "private-key-jwt" })}>
                  private_key_jwt
                </button>
              </div>
            </div>
            {value.clientAuth === "private-key-jwt" && (
              <div className="flex flex-col gap-3 px-3.5 py-3">
                <TextField
                  label="Key ID (kid, optional)"
                  value={value.keyId}
                  onChange={(keyId) => set({ keyId: keyId || undefined })}
                  placeholder="key-1"
                />
                <Field label="RSA private key PEM (RS256) — leave blank to sign with Client secret (HS256)">
                  <textarea
                    aria-label="RSA private key PEM"
                    value={value.privateKeyPem ?? ""}
                    onChange={(e) => set({ privateKeyPem: e.target.value || undefined })}
                    placeholder="-----BEGIN PRIVATE KEY-----&#10;..."
                    className="min-h-[84px] rounded-[9px] border border-border-strong bg-transparent p-2.5 font-mono text-[12px] text-text outline-none focus:border-accent-line"
                  />
                </Field>
              </div>
            )}
            <div className="flex items-center gap-3 px-3.5 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="text-[13.5px]">Header prefix</div>
                <div className="text-[12px] text-faint">The token is sent as Authorization: prefix token.</div>
              </div>
              <input
                aria-label="Header prefix"
                className="input h-8 w-32 font-mono text-[13px]"
                placeholder="Bearer"
                value={value.headerPrefix ?? ""}
                onChange={(e) => set({ headerPrefix: e.target.value || undefined })}
              />
            </div>
          </div>
          <TextField
            label="Token introspection URL (RFC 7662, optional)"
            value={value.introspectionUrl}
            onChange={(introspectionUrl) => set({ introspectionUrl: introspectionUrl || undefined })}
            placeholder="https://id.example.com/oauth2/introspect"
          />
          <Field label="Extra parameters (sent with the authorization and token requests, e.g. audience)">
            <KeyValueEditor
              rows={value.extraParams ?? []}
              onChange={(extraParams) => set({ extraParams: extraParams.length ? extraParams : undefined })}
              keyPlaceholder="Parameter"
              make={makeKv}
            />
          </Field>
        </div>
      )}
    </div>
  );
}

function maskToken(t: string): string {
  return t.length <= 16 ? "•".repeat(t.length) : `${t.slice(0, 8)}${"•".repeat(12)}${t.slice(-6)}`;
}

/**
 * The access token for a request (or a collection or folder): status, and
 * buttons to get a new one, refresh or clear it.
 */
export function OAuthTokenPanel({ target, configKey }: { target: { request: Request; parentId: string | null }; configKey: string }) {
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const envId = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const toast = useStore((s) => s.toast);
  const [status, setStatus] = useState<OAuthTokenStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"get" | "refresh" | "introspect" | null>(null);
  const [prompt, setPrompt] = useState<OAuthPrompt | null>(null);
  const [reveal, setReveal] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const [introspection, setIntrospection] = useState<Record<string, unknown> | null>(null);
  // The target is a new object on every render; the effect below keys on
  // `configKey` and reads the latest target from here.
  const targetRef = useRef(target);
  useEffect(() => {
    targetRef.current = target;
  });

  // Reload when the settings, the environment or the token change.
  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    const t = setTimeout(() => {
      api
        .oauthTokenStatus(workspaceId, targetRef.current.request, targetRef.current.parentId, envId)
        .then((s) => {
          if (cancelled) return;
          setStatus(s);
          setError(null);
        })
        .catch((e) => !cancelled && setError(errorMessage(e)));
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [workspaceId, envId, configKey, version]);

  const copy = async (text: string, what: string) => {
    await navigator.clipboard?.writeText(text);
    setCopied(what);
    setTimeout(() => setCopied(null), 1200);
  };

  const getToken = async () => {
    if (!workspaceId) return;
    setBusy("get");
    setError(null);
    setPrompt(null);
    const off = await onEvent<OAuthPrompt>("oauth-prompt", setPrompt);
    try {
      await api.oauthGetToken(workspaceId, target.request, target.parentId, envId);
      toast("success", "Got a new access token");
    } catch (e) {
      const msg = errorMessage(e);
      if (!/cancelled/.test(msg)) setError(msg);
    } finally {
      off();
      setPrompt(null);
      setBusy(null);
      setVersion((v) => v + 1);
    }
  };
  const refresh = async () => {
    if (!workspaceId) return;
    setBusy("refresh");
    setError(null);
    try {
      await api.oauthRefreshToken(workspaceId, target.request, target.parentId, envId);
      toast("success", "Refreshed the access token");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
      setVersion((v) => v + 1);
    }
  };
  const introspect = async () => {
    if (!workspaceId) return;
    setBusy("introspect");
    setError(null);
    try {
      const res = await api.oauthIntrospectToken(workspaceId, target.request, target.parentId, envId);
      setIntrospection(res);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };
  const clear = async () => {
    if (!workspaceId) return;
    try {
      await api.oauthClearToken(workspaceId, target.request, target.parentId, envId);
      setIntrospection(null);
    } catch (e) {
      setError(errorMessage(e));
    }
    setVersion((v) => v + 1);
  };

  const token = status?.token ?? null;
  const expiry = token?.expiresAt ? expiryLabel(token.expiresAt) : null;
  const waiting = busy === "get" && status?.needsUser;

  return (
    <section aria-label="Access token" className="max-w-[720px] overflow-hidden rounded-[12px] border border-border-mid">
      <div className="flex items-center gap-3 border-b border-border-soft px-4 py-3">
        <KeyRound size={15} strokeWidth={1.6} className="shrink-0 text-muted" />
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-medium">Access token</div>
          <div className="text-[12.5px] text-faint">
            {!status
              ? "Checking…"
              : !token
                ? status.needsUser
                  ? "None yet. Sign in once; after that it's refreshed when it expires."
                  : "None yet. Sending the request gets one automatically."
                : status.fresh
                  ? `Valid${expiry ? `, expires ${expiry}` : ", no expiry given"} · got ${timeAgo(token.obtainedAt)}`
                  : `Expired${token.refreshToken ? "; it's refreshed on the next send" : status.needsUser ? "; sign in again" : "; a new one is fetched on the next send"}`}
          </div>
        </div>
        {token && (
          <button type="button" className="btn-ghost" disabled={!!busy} onClick={() => void introspect()} title="Introspect token (RFC 7662)">
            {busy === "introspect" ? <Loader2 size={13} className="animate-spin" /> : null}
            Introspect
          </button>
        )}
        {token?.refreshToken && (
          <button type="button" className="btn-ghost" disabled={!!busy} onClick={() => void refresh()}>
            {busy === "refresh" ? <Loader2 size={13} className="animate-spin" /> : <RefreshCw size={13} strokeWidth={1.6} />}
            Refresh
          </button>
        )}
        {token && (
          <button type="button" className="btn-ghost hover:text-danger" disabled={!!busy} onClick={() => void clear()}>
            <Trash2 size={13} strokeWidth={1.6} /> Clear
          </button>
        )}
        {busy === "get" ? (
          <button type="button" className="btn h-8" onClick={() => void api.oauthCancel()}>
            Cancel
          </button>
        ) : (
          <button type="button" className="btn-primary" disabled={!status} onClick={() => void getToken()}>
            {token ? "Get new token" : status?.needsUser ? "Sign in" : "Get access token"}
          </button>
        )}
      </div>

      {waiting && prompt?.kind === "device" && (
        <div className="flex items-center gap-5 border-b border-border-soft bg-sunken px-4 py-4">
          <div className="min-w-0 flex-1">
            <div className="text-[13px] text-muted">
              Enter this code at <span className="font-mono text-text">{prompt.verificationUri.replace(/^https?:\/\//, "")}</span>
            </div>
            <div className="mt-1.5 font-mono text-[26px] font-semibold tracking-[0.12em]">{prompt.userCode}</div>
            <div className="mt-1 flex items-center gap-1.5 text-[12px] text-faint">
              <Loader2 size={12} className="animate-spin" /> Waiting for you to approve it…
            </div>
          </div>
          <div className="flex shrink-0 flex-col gap-2">
            <button type="button" className="btn h-8" onClick={() => void copy(prompt.userCode, "code")}>
              {copied === "code" ? <Check size={13} /> : <Copy size={13} strokeWidth={1.5} />} Copy code
            </button>
            <button type="button" className="btn h-8" onClick={() => void openExternal(prompt.verificationUriComplete || prompt.verificationUri)}>
              <ExternalLink size={13} strokeWidth={1.5} /> Open page
            </button>
          </div>
        </div>
      )}
      {waiting && prompt?.kind !== "device" && (
        <div className="flex items-center gap-2 border-b border-border-soft bg-sunken px-4 py-3 text-[13px] text-muted">
          <Loader2 size={13} className="shrink-0 animate-spin" />
          <span className="min-w-0 flex-1">Finish signing in in your browser. This page updates when you&apos;re done.</span>
          {prompt?.kind === "browser" && (
            <button type="button" className="btn-ghost shrink-0" onClick={() => void copy(prompt.url, "link")}>
              {copied === "link" ? <Check size={13} /> : <Copy size={13} strokeWidth={1.5} />} Copy sign-in link
            </button>
          )}
        </div>
      )}

      {token && (
        <div className="px-4 py-3">
          <div className="flex items-center gap-2">
            <code className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-muted" title={reveal ? token.accessToken : undefined}>
              {reveal ? token.accessToken : maskToken(token.accessToken)}
            </code>
            <button type="button" className="icon-btn h-7 w-7" aria-label={reveal ? "Hide token" : "Show token"} onClick={() => setReveal((v) => !v)}>
              {reveal ? <EyeOff size={13} strokeWidth={1.6} /> : <Eye size={13} strokeWidth={1.6} />}
            </button>
            <button type="button" className="icon-btn h-7 w-7" aria-label="Copy token" onClick={() => void copy(token.accessToken, "token")}>
              {copied === "token" ? <Check size={13} /> : <Copy size={13} strokeWidth={1.5} />}
            </button>
          </div>
          <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-faint">
            <span>Type {token.tokenType || "Bearer"}</span>
            {token.scope && <span>Scope {token.scope}</span>}
            <span>{token.refreshToken ? "Has a refresh token" : "No refresh token"}</span>
            {token.idToken && <span>Has an ID token</span>}
          </div>
          {introspection && (
            <pre className="mt-2.5 max-h-40 overflow-auto rounded-lg border border-border bg-panel p-2.5 font-mono text-[11.5px] text-muted">
              {JSON.stringify(introspection, null, 2)}
            </pre>
          )}
        </div>
      )}
      {error && <p className="border-t border-border-soft px-4 py-2.5 text-[12.5px] text-danger">{error}</p>}
      <p className="border-t border-border-soft px-4 py-2.5 text-[12px] text-faint">
        Tokens are kept on this computer in <span className="font-mono">.trax/state.json</span>, one per environment&apos;s settings, and never committed.
      </p>
    </section>
  );
}
