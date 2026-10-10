"use client";

import { Archive, GitPullRequest, Loader2, ShieldAlert, ShieldCheck, Tag as TagIcon } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import { api, errorMessage, openExternal } from "@/lib/api";
import { useStore } from "@/lib/store";
import type { Commit, ConflictPreview, MergeOutcome, MergeSide, PrContext, PullRequest, StashEntry, Tag } from "@/lib/types";
import { cx, timeAgo } from "@/lib/util";

import { CodeEditor } from "../editors/CodeEditor";
import { useGit } from "./gitStore";

const SIDE_LABEL: Record<MergeSide, string> = { ours: "Mine", theirs: "Theirs", base: "Original" };

/** Merge labels quote names in backticks ("Header `Authorization`"). */
const plain = (label: string) => label.replace(/`/g, "");

function conflictMessage(v: unknown): string {
  const outcome = v as MergeOutcome;
  return outcome.kind === "conflicts"
    ? `Conflicts in ${outcome.files.length} file${outcome.files.length === 1 ? "" : "s"}. Resolve them in Source control.`
    : "Done";
}

/** Signature verdict next to a commit; nothing for unsigned commits. */
export function SignatureBadge({ commit, withLabel }: { commit: Commit; withLabel?: boolean }) {
  const st = commit.signatureStatus;
  if (!st) return null;
  const good = st === "good";
  const label = good ? "Verified" : st === "untrusted" ? "Unverified key" : st === "cannot-check" ? "Can't verify" : `Signature ${st}`;
  const title = `${label}${commit.signer ? ` · ${commit.signer}` : ""}${commit.signingKey ? ` · key ${commit.signingKey}` : ""}`;
  return (
    <span title={title} className={cx("inline-flex items-center gap-1 align-[-2px]", good ? "text-ok" : "text-warn")}>
      {good ? <ShieldCheck size={12} aria-label={label} /> : <ShieldAlert size={12} aria-label={label} />}
      {withLabel && <span className="text-[11.5px] font-medium">{label}</span>}
    </span>
  );
}

/**
 * Field-level resolution of a conflicted request, environment or collection
 * file: automatic merges are listed, overlapping edits are picked per field,
 * and the merged YAML can be adjusted by hand before it's written.
 */
export function ConflictEditor({ workspaceId, path, fallback }: { workspaceId: string; path: string; fallback: ReactNode }) {
  const { run } = useGit.getState();
  const [preview, setPreview] = useState<ConflictPreview | null>(null);
  const [choices, setChoices] = useState<Record<string, MergeSide>>({});
  const [error, setError] = useState<string | null>(null);
  const [manual, setManual] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .gitConflictPreview(workspaceId, path, choices)
      .then((p) => !cancelled && setPreview(p))
      .catch((e) => !cancelled && setError(errorMessage(e)));
    return () => {
      cancelled = true;
    };
  }, [workspaceId, path, choices]);

  if (error) return <p className="mx-4 rounded-lg bg-danger-soft p-3 text-[13px] text-danger">{error}</p>;
  if (!preview) {
    return (
      <div className="flex flex-1 items-center justify-center text-faint">
        <Loader2 size={16} className="animate-spin" />
      </div>
    );
  }
  const semantic = preview.semantic;
  if (!semantic) return <>{fallback}</>;

  const apply = () =>
    void run("resolve", (w) => api.gitResolveSemantic(w, path, choices, manual), () => "Merged and marked resolved");

  return (
    <div className="min-h-0 flex-1 overflow-auto px-4 pb-4">
      <div className="mb-3 flex flex-wrap items-center gap-2 rounded-[10px] border border-warn/30 bg-warn/8 px-3.5 py-2.5 text-[13px]">
        <span className="min-w-0 flex-1 text-muted">
          {semantic.conflicts.length === 0
            ? "Both sides changed this file, but in different fields. The merge below keeps both."
            : `Both sides changed ${semantic.conflicts.length} field${semantic.conflicts.length === 1 ? "" : "s"}. Pick a version of each, then apply.`}
        </span>
        <button type="button" className="btn h-8" onClick={() => setManual(manual === null ? semantic.mergedYaml : null)}>
          {manual === null ? "Edit by hand" : "Back to fields"}
        </button>
        <button type="button" className="btn-primary h-8" onClick={apply}>
          Apply merge
        </button>
      </div>

      {manual !== null ? (
        <div className="h-[420px] overflow-hidden rounded-[10px] border border-border-mid bg-sunken">
          <CodeEditor value={manual} onChange={setManual} />
        </div>
      ) : (
        <>
          {semantic.conflicts.length > 0 && (
            <div className="overflow-hidden rounded-[10px] border border-border-mid">
              {semantic.conflicts.map((c) => (
                <fieldset key={c.id} className="border-b border-border px-3.5 py-3 last:border-b-0">
                  <legend className="sr-only">{plain(c.label)}</legend>
                  <div className="flex items-center gap-2.5">
                    <span className="shrink-0 rounded bg-danger/14 px-[7px] py-0.5 text-[11.5px] leading-4 font-semibold text-danger">Conflict</span>
                    <span className="min-w-0 truncate text-[13px]">{plain(c.label)}</span>
                    <span className="flex-1" />
                    <span className="shrink-0 text-[11.5px] text-faint">{c.section}</span>
                  </div>
                  <div className="mt-2 grid gap-1.5">
                    {(["ours", "theirs", "base"] as const).map((side) => {
                      const value = c[side];
                      const picked = c.chosen === side;
                      return (
                        <label
                          key={side}
                          className={cx(
                            "flex cursor-pointer items-start gap-2.5 rounded-md border px-2.5 py-[6px] text-[12.5px]",
                            picked ? "border-accent-line bg-accent-soft" : "border-border hover:bg-hover",
                          )}
                        >
                          <input
                            type="radio"
                            name={`conflict-${c.id}`}
                            checked={picked}
                            onChange={() => setChoices((prev) => ({ ...prev, [c.id]: side }))}
                            className="mt-[3px] accent-[var(--accent)]"
                          />
                          <span className="w-[58px] shrink-0 font-medium text-text-2">{SIDE_LABEL[side]}</span>
                          <span className={cx("min-w-0 flex-1 font-mono break-all whitespace-pre-wrap", value == null && "text-faint italic")}>
                            {value ?? "(removed)"}
                          </span>
                        </label>
                      );
                    })}
                  </div>
                </fieldset>
              ))}
            </div>
          )}
          {semantic.autoMerged.length > 0 && (
            <div className="mt-3">
              <div className="pb-1.5 text-[12.5px] text-faint">Merged automatically</div>
              <ul className="space-y-1 text-[12.5px] text-text-2">
                {semantic.autoMerged.map((m) => (
                  <li key={m} className="flex items-center gap-2">
                    <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-ok" aria-hidden />
                    {plain(m)}
                  </li>
                ))}
              </ul>
            </div>
          )}
          <details className="mt-3">
            <summary className="cursor-pointer text-[12.5px] text-faint hover:text-text">Merged file</summary>
            <pre className="mt-2 overflow-auto rounded-[10px] border border-border-mid bg-sunken p-3 font-mono text-[12px] leading-[18px]">{semantic.mergedYaml}</pre>
          </details>
        </>
      )}
    </div>
  );
}

function PanelHint({ children }: { children: ReactNode }) {
  return <p className="px-2 pt-1 pb-2 text-[12px] leading-[17px] text-dim">{children}</p>;
}

/** Stashed work: apply, pop or drop. */
export function StashesPanel({ hasChanges }: { hasChanges: boolean }) {
  const gitEpoch = useStore((s) => s.gitEpoch);
  const workspaceId = useGit((s) => s.workspaceId);
  const busy = useGit((s) => s.busy);
  const { run } = useGit.getState();
  const { prompt, confirm } = useStore.getState();
  const [stashes, setStashes] = useState<StashEntry[] | null>(null);

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    api
      .gitStashes(workspaceId)
      .then((s) => !cancelled && setStashes(s))
      .catch(() => !cancelled && setStashes([]));
    return () => {
      cancelled = true;
    };
  }, [workspaceId, gitEpoch]);

  const stash = async () => {
    const message = await prompt({ title: "Stash changes", label: "Note (optional)", initial: "WIP", confirm: "Stash" });
    if (message !== null) void run("stash", (w) => api.gitStashPush(w, message, true), () => "Changes stashed");
  };

  return (
    <div className="px-2 pt-2 pb-3">
      <button type="button" className="btn mb-2 h-7 w-full justify-center text-[12.5px]" disabled={!hasChanges || !!busy} onClick={() => void stash()}>
        <Archive size={13} /> Stash changes
      </button>
      {stashes === null && <PanelHint>Loading…</PanelHint>}
      {stashes?.length === 0 && <PanelHint>Nothing stashed. Stash puts your uncommitted changes aside so you can switch branches.</PanelHint>}
      {stashes?.map((st) => (
        <div key={st.reference} className="group rounded-[7px] p-2 hover:bg-hover">
          <div className="text-[13px] leading-[18px]">{st.message}</div>
          <div className="mt-[3px] text-[11.5px] text-faint">
            <span className="font-mono">{st.reference}</span> · {timeAgo(st.date)}
          </div>
          <div className="mt-1.5 flex gap-1">
            <button type="button" className="btn h-6 px-2 text-[12px]" disabled={!!busy} onClick={() => void run("stash", (w) => api.gitStashApply(w, st.index, true), conflictMessage)}>
              Pop
            </button>
            <button type="button" className="btn h-6 px-2 text-[12px]" disabled={!!busy} onClick={() => void run("stash", (w) => api.gitStashApply(w, st.index, false), conflictMessage)}>
              Apply
            </button>
            <button
              type="button"
              className="btn-ghost h-6 px-2 text-[12px] text-danger"
              disabled={!!busy}
              onClick={async () => {
                if (await confirm({ title: `Drop ${st.reference}?`, message: `${st.message}\n\nThese changes are deleted.`, confirm: "Drop", danger: true })) {
                  void run("stash", (w) => api.gitStashDrop(w, st.index), () => "Stash dropped");
                }
              }}
            >
              Drop
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

/** Tags: create on HEAD, push, delete. */
export function TagsPanel({ hasRemote }: { hasRemote: boolean }) {
  const gitEpoch = useStore((s) => s.gitEpoch);
  const workspaceId = useGit((s) => s.workspaceId);
  const busy = useGit((s) => s.busy);
  const { run } = useGit.getState();
  const { confirm } = useStore.getState();
  const [tags, setTags] = useState<Tag[] | null>(null);

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    api
      .gitTags(workspaceId)
      .then((t) => !cancelled && setTags(t))
      .catch(() => !cancelled && setTags([]));
    return () => {
      cancelled = true;
    };
  }, [workspaceId, gitEpoch]);

  return (
    <div className="px-2 pt-2 pb-3">
      <button type="button" className="btn mb-2 h-7 w-full justify-center text-[12.5px]" disabled={!!busy} onClick={() => void createTag(null)}>
        <TagIcon size={13} /> Tag the latest commit
      </button>
      {tags === null && <PanelHint>Loading…</PanelHint>}
      {tags?.length === 0 && <PanelHint>No tags yet. Tag a commit to mark a release of your collections.</PanelHint>}
      {tags?.map((t) => (
        <div key={t.name} className="rounded-[7px] p-2 hover:bg-hover">
          <div className="flex items-center gap-1.5 text-[13px] font-medium">
            <TagIcon size={12} className="text-faint" /> {t.name}
          </div>
          <div className="mt-[3px] truncate text-[11.5px] text-faint">
            <span className="font-mono">{t.shortHash}</span>
            {t.date ? ` · ${timeAgo(t.date)}` : ""}
            {t.message ? ` · ${t.message}` : ""}
          </div>
          <div className="mt-1.5 flex gap-1">
            <button type="button" className="btn h-6 px-2 text-[12px]" disabled={!!busy || !hasRemote} onClick={() => void run("push", (w) => api.gitPushTag(w, t.name), () => `Pushed ${t.name}`)}>
              Push
            </button>
            <button
              type="button"
              className="btn-ghost h-6 px-2 text-[12px] text-danger"
              disabled={!!busy}
              onClick={async () => {
                if (await confirm({ title: `Delete tag ${t.name}?`, message: "Only the local tag is deleted.", confirm: "Delete", danger: true })) {
                  void run("tag", (w) => api.gitDeleteTag(w, t.name), () => `Deleted ${t.name}`);
                }
              }}
            >
              Delete
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

/** Ask for a tag name (and optional annotation), then tag `target` (HEAD when null). */
export async function createTag(target: string | null) {
  const { prompt } = useStore.getState();
  const name = await prompt({ title: target ? `Tag ${target.slice(0, 7)}` : "Tag the latest commit", label: "Tag name", initial: "v", confirm: "Next" });
  if (!name) return;
  const message = await prompt({ title: `Tag ${name}`, label: "Message (makes an annotated tag)", initial: `Release ${name}`, confirm: "Create tag" });
  if (message === null) return;
  void useGit.getState().run("tag", (w) => api.gitCreateTag(w, name, target, message || null), () => `Tagged ${name}`);
}

/** Cherry-pick or revert a commit on the current branch. */
export function commitActions(commit: Commit, current: string) {
  const { confirm } = useStore.getState();
  const { run } = useGit.getState();
  return {
    cherryPick: async () => {
      if (await confirm({ title: `Cherry-pick onto ${current}?`, message: `"${commit.subject}" is applied on top of ${current} as a new commit.`, confirm: "Cherry-pick" })) {
        void run("cherry-pick", (w) => api.gitCherryPick(w, commit.hash), (v) => ((v as MergeOutcome).kind === "conflicts" ? conflictMessage(v) : `Cherry-picked ${commit.shortHash}`));
      }
    },
    revert: async () => {
      if (await confirm({ title: `Revert ${commit.shortHash}?`, message: `A new commit on ${current} undoes "${commit.subject}". History is kept.`, confirm: "Revert" })) {
        void run("revert", (w) => api.gitRevert(w, commit.hash), (v) => ((v as MergeOutcome).kind === "conflicts" ? conflictMessage(v) : `Reverted ${commit.shortHash}`));
      }
    },
    tag: () => void createTag(commit.hash),
  };
}

/** Local branch name when it exists, else its remote-tracking name. */
function reviewRef(name: string, localBranches: string[]): string {
  return localBranches.includes(name) ? name : `origin/${name}`;
}

/** Open pull/merge requests on GitHub or GitLab; one opens as a review tab. */
export function PullRequestsPanel() {
  const workspaceId = useGit((s) => s.workspaceId);
  const branches = useGit((s) => s.branches);
  const gitEpoch = useStore((s) => s.gitEpoch);
  const { openReview, setView } = useStore.getState();
  const [ctx, setCtx] = useState<PrContext | null>(null);
  const [state, setState] = useState<"open" | "all">("open");
  const [pulls, setPulls] = useState<PullRequest[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceId) return;
    let cancelled = false;
    api
      .getPrSettings(workspaceId)
      .then(async (c) => {
        if (cancelled) return;
        setCtx(c);
        setError(null);
        if (!c.hosting) return setPulls([]);
        const list = await api.listPullRequests(workspaceId, state);
        if (!cancelled) setPulls(list);
      })
      .catch((e) => {
        if (!cancelled) {
          setError(errorMessage(e));
          setPulls([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [workspaceId, state, gitEpoch]);

  const local = branches.filter((b) => !b.remote).map((b) => b.name);
  const providerName = ctx?.hosting?.provider === "gitlab" ? "GitLab" : "GitHub";

  return (
    <div className="px-2 pt-2 pb-3">
      <div className="mb-2 flex items-center justify-between px-0.5">
        <div className="segmented">
          <button type="button" aria-pressed={state === "open"} onClick={() => setState("open")}>
            Open
          </button>
          <button type="button" aria-pressed={state === "all"} onClick={() => setState("all")}>
            All
          </button>
        </div>
        {ctx?.hosting && <span className="truncate pl-2 text-[11.5px] text-faint">{ctx.hosting.fullPath}</span>}
      </div>
      {pulls === null && <PanelHint>Loading…</PanelHint>}
      {ctx && !ctx.hosting && !error && <PanelHint>Pull requests need an origin remote on GitHub or GitLab.</PanelHint>}
      {error && (
        <div className="px-2 pb-2 text-[12px] leading-[17px] text-danger">
          {error}
          <button type="button" className="mt-1 block text-accent-text hover:underline" onClick={() => setView("settings")}>
            Add a token in Settings
          </button>
        </div>
      )}
      {ctx?.hosting && pulls?.length === 0 && !error && <PanelHint>No {state === "open" ? "open " : ""}pull requests on {providerName}.</PanelHint>}
      {pulls?.map((pr) => (
        <div key={pr.number} className="rounded-[7px] p-2 hover:bg-hover">
          <button
            type="button"
            className="block w-full text-left"
            onClick={() => void openReview(reviewRef(pr.targetBranch, local), reviewRef(pr.sourceBranch, local))}
            title="Review the changes"
          >
            <span className="flex items-start gap-1.5 text-[13px] leading-[18px]">
              <GitPullRequest size={13} className={cx("mt-[3px] shrink-0", pr.state === "merged" ? "text-purple" : pr.state === "closed" ? "text-danger" : "text-ok")} />
              <span>
                {pr.title} <span className="text-faint">#{pr.number}</span>
              </span>
            </span>
            <span className="mt-[3px] block truncate pl-[19px] text-[11.5px] text-faint">
              {pr.author} · {pr.sourceBranch} → {pr.targetBranch} · {timeAgo(pr.updatedAt)}
            </span>
          </button>
          <div className="mt-1 flex items-center gap-1.5 pl-[19px] text-[11.5px]">
            {pr.draft && <span className="rounded bg-selected px-1.5 py-px text-muted">Draft</span>}
            {pr.approved && <span className="rounded bg-ok/14 px-1.5 py-px font-medium text-ok">Approved</span>}
            {pr.state !== "open" && <span className="rounded bg-selected px-1.5 py-px text-muted capitalize">{pr.state}</span>}
            {pr.commentCount > 0 && <span className="text-faint">{pr.commentCount} comment{pr.commentCount === 1 ? "" : "s"}</span>}
            <span className="flex-1" />
            <button type="button" className="text-faint hover:text-text" onClick={() => void openExternal(pr.webUrl)}>
              Open on {providerName}
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
