"use client";

import { ChevronDown, ChevronRight, MessageSquare } from "lucide-react";
import { useEffect, useState } from "react";

import { api, errorMessage } from "@/lib/api";
import { fieldLabel } from "@/lib/review";
import { useStore } from "@/lib/store";
import type { ReviewComment } from "@/lib/types";
import { timeAgo } from "@/lib/util";

/** Open review comments about this request, with the field each one is on. */
export function RequestComments({ workspaceId, requestId }: { workspaceId: string; requestId: string }) {
  const gitEpoch = useStore((s) => s.gitEpoch);
  const { toast } = useStore.getState();
  const [comments, setComments] = useState<ReviewComment[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api
      .listReviewComments(workspaceId, { requestId })
      .then((c) => !cancelled && setComments(c.filter((x) => !x.resolved)))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [workspaceId, requestId, gitEpoch]);

  if (comments.length === 0) return null;
  return (
    <div className="border-b border-border bg-accent-soft/40 px-4 py-1.5 text-[13px]">
      <button type="button" className="flex items-center gap-2 text-accent-text-2" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        <MessageSquare size={13} />
        {comments.length} open review comment{comments.length === 1 ? "" : "s"}
        {!open && <span className="truncate text-faint">on {[...new Set(comments.map((c) => fieldLabel(c.field)))].join(", ")}</span>}
      </button>
      {open && (
        <ul className="mt-1.5 mb-1 space-y-1.5">
          {comments.map((c) => (
            <li key={c.id} className="flex items-start gap-2.5 pl-[21px]">
              <span className="mt-px shrink-0 rounded bg-accent-soft px-1.5 py-px text-[11.5px] font-medium text-accent-text-2">{fieldLabel(c.field)}</span>
              <span className="min-w-0 flex-1">
                <span className="whitespace-pre-wrap">{c.body}</span>
                <span className="block text-[11.5px] text-faint">
                  {c.author} · {timeAgo(c.createdAt)}
                  {c.prNumber ? ` · #${c.prNumber}` : ""}
                </span>
              </span>
              <button
                type="button"
                className="shrink-0 text-[12px] text-faint hover:text-text"
                onClick={async () => {
                  try {
                    await api.resolveReviewComment(workspaceId, c.id, true);
                    setComments((all) => all.filter((x) => x.id !== c.id));
                  } catch (e) {
                    toast("error", errorMessage(e));
                  }
                }}
              >
                Resolve
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
