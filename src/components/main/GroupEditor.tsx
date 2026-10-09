"use client";

import { Play, Save } from "lucide-react";
import { useState } from "react";

import { tabDirty, useStore, type GroupTab } from "@/lib/store";
import type { GroupMeta } from "@/lib/types";
import { cx } from "@/lib/util";

import { AuthEditor } from "../editors/AuthEditor";
import { KeyValueEditor, makeKv, makeVar } from "../editors/KeyValueEditor";
import { ScriptsEditor } from "../editors/ScriptsEditor";
import { TabStrip } from "../ui/Segmented";

type Section = "overview" | "variables" | "headers" | "auth" | "scripts";

/** Settings shared by every request in a collection or folder. */
export function GroupEditor({ tab }: { tab: GroupTab }) {
  const { updateTab, saveTab, runGroup } = useStore.getState();
  const [section, setSection] = useState<Section>("overview");
  const g = tab.draft;
  const dirty = tabDirty(tab);
  const set = (patch: Partial<GroupMeta>) => updateTab<GroupTab>(tab.id, (t) => ({ ...t, draft: { ...t.draft, ...patch } }));

  const sections: { id: Section; label: string; badge?: number | string }[] = [
    { id: "overview", label: "Overview" },
    ...(tab.isCollection ? [{ id: "variables" as const, label: "Variables", badge: g.variables?.length || undefined }] : []),
    { id: "headers", label: "Headers", badge: g.headers?.length || undefined },
    { id: "auth", label: "Auth", badge: g.auth && g.auth.type !== "inherit" ? g.auth.type : undefined },
    { id: "scripts", label: "Scripts", badge: g.scripts?.preRequest?.trim() || g.scripts?.postResponse?.trim() ? "●" : undefined },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-3 px-4 pt-4 pb-3">
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-semibold tracking-wider text-faint uppercase">{tab.isCollection ? "Collection" : "Folder"}</div>
          <input
            className="w-full rounded bg-transparent px-1 py-0.5 text-[17px] font-semibold outline-none hover:bg-hover focus:bg-panel-2"
            value={g.name}
            onChange={(e) => set({ name: e.target.value })}
            aria-label="Name"
          />
        </div>
        <button className="btn" onClick={() => void runGroup(tab.groupId)}>
          <Play size={14} /> Run
        </button>
        <button className={cx("btn", dirty && "border-accent text-accent")} disabled={!dirty} onClick={() => void saveTab(tab.id)}>
          <Save size={14} /> Save
        </button>
      </div>
      <TabStrip<Section> tabs={sections} value={section} onChange={setSection} />
      <div className="min-h-0 flex-1 overflow-auto">
        {section === "overview" && (
          <div className="max-w-[720px] p-4">
            <label className="label">Description</label>
            <textarea
              className="input h-40 resize-y py-2"
              placeholder="What this API does, how to authenticate, links to docs…"
              value={g.description ?? ""}
              onChange={(e) => set({ description: e.target.value })}
            />
            <p className="mt-3 text-[12px] leading-relaxed text-faint">
              Headers, auth and scripts set here apply to every request inside. Requests can override headers and choose their own auth.
            </p>
          </div>
        )}
        {section === "variables" && (
          <div className="p-3">
            <KeyValueEditor rows={g.variables ?? []} onChange={(variables) => set({ variables })} make={makeVar} keyPlaceholder="Variable" />
            <p className="mt-2 text-[11.5px] text-faint">Collection variables are shared through git. Environment and runtime variables override them.</p>
          </div>
        )}
        {section === "headers" && (
          <div className="p-3">
            <KeyValueEditor rows={g.headers ?? []} onChange={(headers) => set({ headers })} make={makeKv} keyPlaceholder="Header" />
          </div>
        )}
        {section === "auth" && <AuthEditor auth={g.auth ?? { type: "inherit" }} onChange={(auth) => set({ auth })} allowInherit={!tab.isCollection} />}
        {section === "scripts" && (
          <ScriptsEditor
            scripts={g.scripts ?? {}}
            onChange={(scripts) => set({ scripts })}
            scopeNote={`Runs for every request in this ${tab.isCollection ? "collection" : "folder"}, before the request's own script.`}
          />
        )}
      </div>
    </div>
  );
}
