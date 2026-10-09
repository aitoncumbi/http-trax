"use client";

import { Play } from "lucide-react";
import { useState } from "react";

import { tabDirty, useStore, type GroupTab } from "@/lib/store";
import type { GroupMeta } from "@/lib/types";
import { AuthEditor } from "../editors/AuthEditor";
import { KeyValueEditor, makeKv, makeVar } from "../editors/KeyValueEditor";
import { ScriptsEditor } from "../editors/ScriptsEditor";
import { TabStrip } from "../ui/Segmented";
import { PageHeader, SaveButton } from "./PageHeader";

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
    { id: "scripts", label: "Scripts", badge: g.scripts?.preRequest?.trim() || g.scripts?.postResponse?.trim() ? "•" : undefined },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader eyebrow={tab.isCollection ? "COLLECTION" : "FOLDER"} title={g.name} onTitle={(name) => set({ name })}>
        <button type="button" className="btn h-9 px-3.5" onClick={() => void runGroup(tab.groupId)}>
          <Play size={14} strokeWidth={1.6} /> Run
        </button>
        <SaveButton dirty={dirty} onSave={() => void saveTab(tab.id)} />
      </PageHeader>
      <TabStrip<Section> label="Sections" tabs={sections} value={section} onChange={setSection} className="px-2" />
      <div className="min-h-0 flex-1 overflow-auto">
        {section === "overview" && (
          <div className="max-w-[720px] px-5 py-4">
            <label htmlFor="group-description" className="label">
              Description
            </label>
            <textarea
              id="group-description"
              className="input h-40 resize-y py-2 leading-[21px]"
              placeholder="What this API does, how to authenticate, links to docs…"
              value={g.description ?? ""}
              onChange={(e) => set({ description: e.target.value })}
            />
            <p className="mt-3 text-[12.5px] leading-[19px] text-faint">
              Headers, auth and scripts set here apply to every request inside. Requests can override headers and choose their own auth.
            </p>
          </div>
        )}
        {section === "variables" && (
          <div className="px-4 py-3">
            <KeyValueEditor rows={g.variables ?? []} onChange={(variables) => set({ variables })} make={makeVar} keyPlaceholder="Variable" />
            <p className="mx-1 mt-3 text-[12.5px] text-faint">Collection variables are shared through git. Environment and runtime variables override them.</p>
          </div>
        )}
        {section === "headers" && (
          <div className="px-4 py-3">
            <KeyValueEditor rows={g.headers ?? []} onChange={(headers) => set({ headers })} make={makeKv} keyPlaceholder="Header" describe />
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
