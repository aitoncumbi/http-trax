"use client";

import { Check, Copy, Link2, Save } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { api, errorMessage } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { HostingProvider, PrContext, SigningFormat, SigningSettings, SparseCheckoutStatus, VaultProviderStatus, VaultSettings } from "@/lib/types";
import { cx } from "@/lib/util";

/** Settings shared by the team workflow: vaults, pull requests, signing, sparse checkout and onboarding. */

function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <div>
      <label className="label">{label}</label>
      {children}
      {hint && <p className="mt-1 text-[11.5px] text-faint">{hint}</p>}
    </div>
  );
}

const opt = (v: string) => (v.trim() ? v.trim() : undefined);

export function VaultSettingsForm({ workspaceId }: { workspaceId: string }) {
  const { toast } = useStore.getState();
  const [settings, setSettings] = useState<VaultSettings | null>(null);
  const [providers, setProviders] = useState<VaultProviderStatus[]>([]);
  const [reference, setReference] = useState("");
  const [tested, setTested] = useState<{ ok: boolean; text: string } | null>(null);

  const loadProviders = () => api.vaultProvidersStatus(workspaceId).then(setProviders).catch(() => undefined);
  useEffect(() => {
    api.getVaultSettings(workspaceId).then(setSettings).catch((e) => toast("error", errorMessage(e)));
    void loadProviders();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  if (!settings) return null;
  const set = (patch: Partial<VaultSettings>) => setSettings({ ...settings, ...patch });
  const text = (key: keyof VaultSettings, placeholder: string, secret = false) => (
    <input
      className="input"
      type={secret ? "password" : "text"}
      autoComplete="off"
      placeholder={placeholder}
      value={(settings[key] as string | undefined) ?? ""}
      onChange={(e) => set({ [key]: opt(e.target.value) })}
    />
  );

  return (
    <>
      <label className="mb-2 flex items-center gap-2 text-[12.5px]">
        <input type="checkbox" className="accent-[var(--accent)]" checked={settings.useOsKeychain} onChange={(e) => set({ useOsKeychain: e.target.checked })} />
        Keep secret values in the OS keychain instead of <code className="font-mono">.trax/state.json</code>
      </label>
      <p className="mb-3 text-[11.5px] leading-relaxed text-faint">
        A secret variable can also hold a vault reference, which is shared with the team through git and read when a request is sent:{" "}
        <code className="font-mono">op://vault/item/field</code>, <code className="font-mono">vault://mount/path#field</code>,{" "}
        <code className="font-mono">azkv://vault-name/secret</code>.
      </p>
      <div className="mb-3 divide-y divide-border-soft rounded-[10px] border border-border-mid">
        {providers.map((p) => (
          <div key={p.id} className="flex items-center gap-2.5 px-3.5 py-2">
            <span className={cx("h-2 w-2 shrink-0 rounded-full", p.available ? "bg-ok" : "bg-checkbox-off")} aria-label={p.available ? "available" : "not available"} />
            <span className="w-[150px] shrink-0 text-[13px]">{p.name}</span>
            <span className="min-w-0 flex-1 truncate text-[12px] text-faint" title={p.detail}>
              {p.detail}
            </span>
          </div>
        ))}
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Field label="1Password Connect URL">{text("onepasswordConnectUrl", "Empty: use the op CLI")}</Field>
        <Field label="1Password token">{text("onepasswordToken", "Connect or service account token", true)}</Field>
        <Field label="HashiCorp Vault address">{text("hashicorpAddr", "https://vault.example.com:8200")}</Field>
        <Field label="Vault token">{text("hashicorpToken", "Empty: VAULT_TOKEN or the vault CLI", true)}</Field>
        <Field label="Vault namespace">{text("hashicorpNamespace", "Enterprise only")}</Field>
        <Field label="Azure Key Vault">{text("azureVaultName", "Default vault name")}</Field>
        <Field label="Azure access token">{text("azureToken", "Empty: az login", true)}</Field>
      </div>
      <p className="mt-2 text-[11.5px] text-faint">Tokens are kept in the OS keychain on this machine, never in the workspace.</p>
      <button
        className="btn-primary mt-2"
        onClick={async () => {
          try {
            await api.saveVaultSettings(workspaceId, settings);
            await loadProviders();
            toast("success", "Secret settings saved");
          } catch (e) {
            toast("error", errorMessage(e));
          }
        }}
      >
        <Save size={13} /> Save
      </button>
      <div className="mt-4">
        <label className="label">Test a reference</label>
        <form
          className="flex gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            setTested(null);
            try {
              const value = await api.resolveSecretReference(workspaceId, reference.trim());
              setTested({ ok: true, text: `Resolved: ${value.length} character${value.length === 1 ? "" : "s"}` });
            } catch (err) {
              setTested({ ok: false, text: errorMessage(err) });
            }
          }}
        >
          <input className="input font-mono text-[12px]" placeholder="op://Engineering/Stripe/credential" value={reference} onChange={(e) => setReference(e.target.value)} />
          <button type="submit" className="btn shrink-0" disabled={!reference.trim()}>
            Test
          </button>
        </form>
        {tested && <p className={cx("mt-1.5 text-[12px]", tested.ok ? "text-ok" : "text-danger")}>{tested.text}</p>}
      </div>
    </>
  );
}

export function PullRequestSettingsForm({ workspaceId }: { workspaceId: string }) {
  const { toast } = useStore.getState();
  const [ctx, setCtx] = useState<PrContext | null>(null);

  useEffect(() => {
    api.getPrSettings(workspaceId).then(setCtx).catch((e) => toast("error", errorMessage(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  if (!ctx) return null;
  const s = ctx.settings;
  const set = (patch: Partial<typeof s>) => setCtx({ ...ctx, settings: { ...s, ...patch } });
  const hosting = ctx.hosting;

  return (
    <>
      <p className="mb-3 text-[12px] text-faint">
        {hosting
          ? `origin is ${hosting.provider === "gitlab" ? "GitLab" : "GitHub"}: ${hosting.fullPath} on ${hosting.host}.`
          : "origin isn't recognised as GitHub or GitLab. Pick the provider and API URL for a self-hosted server."}
      </p>
      <div className="grid grid-cols-2 gap-2">
        <Field label="Provider">
          <select
            className="input"
            value={s.providerOverride ?? ""}
            onChange={(e) => set({ providerOverride: (e.target.value || undefined) as HostingProvider | undefined })}
          >
            <option value="">Detect from origin</option>
            <option value="github">GitHub</option>
            <option value="gitlab">GitLab</option>
          </select>
        </Field>
        <Field label="API URL">
          <input className="input" placeholder={hosting?.apiBase ?? "https://github.example.com/api/v3"} value={s.apiBaseUrl ?? ""} onChange={(e) => set({ apiBaseUrl: opt(e.target.value) })} />
        </Field>
        <Field label="GitHub token" hint="Empty: GITHUB_TOKEN or gh auth token">
          <input className="input" type="password" autoComplete="off" value={s.githubToken ?? ""} onChange={(e) => set({ githubToken: opt(e.target.value) })} />
        </Field>
        <Field label="GitLab token" hint="Empty: GITLAB_TOKEN">
          <input className="input" type="password" autoComplete="off" value={s.gitlabToken ?? ""} onChange={(e) => set({ gitlabToken: opt(e.target.value) })} />
        </Field>
      </div>
      <button
        className="btn-primary mt-3"
        onClick={async () => {
          try {
            await api.savePrSettings(workspaceId, ctx.settings);
            setCtx(await api.getPrSettings(workspaceId));
            toast("success", "Pull request settings saved");
          } catch (e) {
            toast("error", errorMessage(e));
          }
        }}
      >
        <Save size={13} /> Save
      </button>
    </>
  );
}

const FORMAT_HINT: Record<SigningFormat, string> = {
  gpg: "GPG key id, e.g. 3AA5C34371567BD2",
  ssh: "Path to a public key, e.g. ~/.ssh/id_ed25519.pub",
  x509: "Certificate id (gpgsm / smimesign)",
};

export function SigningSettingsForm({ workspaceId }: { workspaceId: string }) {
  const gitEpoch = useStore((st) => st.gitEpoch);
  const { toast } = useStore.getState();
  const [s, setS] = useState<SigningSettings | null>(null);

  useEffect(() => {
    api.gitSigningSettings(workspaceId).then(setS).catch(() => setS(null));
  }, [workspaceId, gitEpoch]);

  if (!s) return <p className="text-[12px] text-faint">Available once the workspace is in a git repository.</p>;
  return (
    <>
      <p className="mb-2 text-[11.5px] text-faint">Stored in this repository&apos;s git config, so each team&apos;s repository can have its own key and format.</p>
      <label className="mb-2 flex items-center gap-2 text-[12.5px]">
        <input type="checkbox" className="accent-[var(--accent)]" checked={s.enabled} onChange={(e) => setS({ ...s, enabled: e.target.checked })} />
        Sign every commit made here
      </label>
      <div className="grid grid-cols-2 gap-2">
        <Field label="Format">
          <select className="input" value={s.format} onChange={(e) => setS({ ...s, format: e.target.value as SigningFormat })}>
            <option value="ssh">SSH key</option>
            <option value="gpg">GPG (OpenPGP)</option>
            <option value="x509">X.509</option>
          </select>
        </Field>
        <Field label="Signing key">
          <input className="input font-mono text-[12px]" placeholder={FORMAT_HINT[s.format]} value={s.signingKey ?? ""} onChange={(e) => setS({ ...s, signingKey: opt(e.target.value) ?? null })} />
        </Field>
        <Field label="Program (optional)">
          <input className="input font-mono text-[12px]" placeholder={s.format === "ssh" ? "ssh-keygen" : "gpg"} value={s.program ?? ""} onChange={(e) => setS({ ...s, program: opt(e.target.value) ?? null })} />
        </Field>
      </div>
      <button
        className="btn-primary mt-3"
        onClick={async () => {
          try {
            await api.gitSetSigningSettings(workspaceId, s);
            toast("success", s.enabled ? "Commits will be signed" : "Commit signing turned off");
          } catch (e) {
            toast("error", errorMessage(e));
          }
        }}
      >
        <Save size={13} /> Save
      </button>
    </>
  );
}

export function SparseCheckoutForm({ workspaceId }: { workspaceId: string }) {
  const { toast, reloadTree, bumpGit } = useStore.getState();
  const [status, setStatus] = useState<SparseCheckoutStatus | null>(null);
  const [patterns, setPatterns] = useState("");

  const load = () =>
    api
      .gitSparseCheckoutStatus(workspaceId)
      .then((st) => {
        setStatus(st);
        setPatterns(st.patterns.join("\n"));
      })
      .catch(() => setStatus(null));
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  if (!status) return <p className="text-[12px] text-faint">Available once the workspace is in a git repository.</p>;
  const apply = async (disable: boolean) => {
    try {
      if (disable) await api.gitSparseCheckoutDisable(workspaceId);
      else await api.gitSparseCheckoutSet(workspaceId, patterns.split("\n").map((p) => p.trim()).filter(Boolean), status.cone);
      await load();
      await reloadTree();
      bumpGit();
      toast("success", disable ? "Checking out the whole repository" : "Sparse checkout updated");
    } catch (e) {
      toast("error", errorMessage(e));
    }
  };
  return (
    <>
      <p className="mb-2 text-[11.5px] text-faint">
        In a large monorepo, check out only the folders you need, e.g. <code className="font-mono">services/payments</code>. The workspace folder must stay included.
      </p>
      <textarea
        aria-label="Sparse checkout folders"
        className="input h-20 py-1.5 font-mono text-[12px]"
        placeholder={"services/payments\nshared/api-tests"}
        value={patterns}
        onChange={(e) => setPatterns(e.target.value)}
      />
      <label className="mt-2 flex items-center gap-2 text-[12.5px]">
        <input type="checkbox" className="accent-[var(--accent)]" checked={status.cone} onChange={(e) => setStatus({ ...status, cone: e.target.checked })} />
        Folders only (cone mode, fastest)
      </label>
      <div className="mt-2 flex gap-2">
        <button className="btn-primary" disabled={!patterns.trim()} onClick={() => void apply(false)}>
          {status.enabled ? "Update" : "Turn on"}
        </button>
        {status.enabled && (
          <button className="btn" onClick={() => void apply(true)}>
            Check out everything
          </button>
        )}
      </div>
    </>
  );
}

export function OnboardingLinkForm({ workspaceId }: { workspaceId: string }) {
  const { toast } = useStore.getState();
  const [link, setLink] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  return (
    <>
      <p className="mb-2 text-[11.5px] text-faint">
        A link a teammate pastes into Workspaces → Join with a link: it clones the repository (sparse if you use it), opens this workspace and sets up
        the first shared environment, asking for its secrets. Secret values are never put in the link.
      </p>
      {link ? (
        <div className="flex gap-2">
          <input className="input font-mono text-[12px]" readOnly value={link} onFocus={(e) => e.target.select()} aria-label="Onboarding link" />
          <button
            className="btn shrink-0"
            onClick={async () => {
              await navigator.clipboard?.writeText(link);
              setCopied(true);
              setTimeout(() => setCopied(false), 1200);
            }}
          >
            {copied ? <Check size={13} /> : <Copy size={13} />} {copied ? "Copied" : "Copy"}
          </button>
        </div>
      ) : (
        <button
          className="btn"
          onClick={async () => {
            try {
              setLink(await api.createOnboardingLink(workspaceId));
            } catch (e) {
              toast("error", errorMessage(e));
            }
          }}
        >
          <Link2 size={13} /> Create onboarding link
        </button>
      )}
    </>
  );
}
