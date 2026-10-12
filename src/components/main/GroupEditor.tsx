"use client";

import { Play } from "lucide-react";
import { useState } from "react";

import { tabDirty, useStore, type GroupTab } from "@/lib/store";
import type { GroupMeta } from "@/lib/types";
import { newRequest } from "@/lib/util";
import { AuthEditor } from "../editors/AuthEditor";
import { KeyValueEditor, makeKv, makeVar } from "../editors/KeyValueEditor";
import { ScriptsEditor } from "../editors/ScriptsEditor";
import { Switch } from "../ui/Controls";
import { TabStrip } from "../ui/Segmented";
import { PageHeader, SaveButton } from "./PageHeader";

type Section = "overview" | "variables" | "headers" | "auth" | "scripts" | "contract";

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
    { id: "contract", label: "Contract", badge: g.contract?.off ? "off" : g.contract?.openapi ? "•" : undefined },
  ];
  const noun = tab.isCollection ? "collection" : "folder";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader eyebrow={tab.isCollection ? "COLLECTION" : "FOLDER"} title={g.name} onTitle={(name) => set({ name })}>
        <button type="button" className="btn h-9 px-3.5" onClick={() => void runGroup(tab.groupId)}>
          <Play size={14} strokeWidth={1.6} /> Run
        </button>
        <SaveButton dirty={dirty} onSave={() => void saveTab(tab.id)} />
      </PageHeader>
      <TabStrip<Section> label="Sections" tabs={sections} value={section} onChange={setSection} />
      <div className="min-h-0 flex-1 overflow-auto">
        {section === "overview" && (
          <div className="max-w-[720px] px-4 py-4">
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
        {section === "auth" && (
          <AuthEditor
            auth={g.auth ?? { type: "inherit" }}
            onChange={(auth) => set({ auth })}
            allowInherit={!tab.isCollection}
            oauthTarget={{ request: { ...newRequest(), id: "", auth: g.auth ?? { type: "inherit" } }, parentId: tab.groupId }}
          />
        )}
        {section === "contract" && (
          <div className="max-w-[720px] px-4 py-4">
            <label htmlFor="group-openapi" className="label">
              OpenAPI document
            </label>
            <input
              id="group-openapi"
              className="input h-9 font-mono text-[13px]"
              placeholder="../openapi.yaml"
              disabled={g.contract?.off}
              value={g.contract?.openapi ?? ""}
              onChange={(e) => set({ contract: e.target.value ? { openapi: e.target.value } : undefined })}
            />
            <p className="mt-3 text-[12.5px] leading-[19px] text-faint">
              Every response in this {noun} is checked against the operation for its method and path: the status, content type and required
              headers must be documented, and the body must match the schema. The result shows up as a &ldquo;contract&rdquo; test. OpenAPI 3.x
              and Swagger 2.0, YAML or JSON. The path is relative to the workspace folder, so the spec can stay next to the service&rsquo;s code.
            </p>
            {!tab.isCollection && (
              <div className="mt-4 flex min-h-[62px] items-center gap-3.5 border-t border-border-soft">
                <div className="min-w-0 flex-1">
                  <div className="text-[14.5px]">Turn off for this folder</div>
                  <div className="mt-0.5 text-[12.5px] text-faint">Skip the collection&rsquo;s contract for every request in this folder.</div>
                </div>
                <Switch checked={!!g.contract?.off} onChange={(v) => set({ contract: v ? { off: true } : undefined })} label="Turn off for this folder" />
              </div>
            )}
          </div>
        )}
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
