"use client";

import { Bot, FolderOpen, GitBranch, Import, Plus } from "lucide-react";
import type { ReactNode } from "react";

import { useStore } from "@/lib/store";

import { CourionMark } from "../ui/Brand";

function Action({ icon, title, text, kbd, onClick }: { icon: ReactNode; title: string; text: string; kbd?: string; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} className="flex h-14 w-full items-center gap-3.5 rounded-[10px] px-3.5 text-left hover:bg-hover">
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border-mid text-muted">{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block text-[14px] font-medium">{title}</span>
        <span className="block truncate text-[12.5px] text-faint">{text}</span>
      </span>
      {kbd && <span className="kbd">{kbd}</span>}
    </button>
  );
}

export function Welcome() {
  const tree = useStore((s) => s.tree);
  const { createCollection, setView, newRequestHere, showImportDialog } = useStore.getState();
  const hasCollections = (tree?.collections.length ?? 0) > 0;
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="w-full max-w-[440px]">
        <h1 className="text-[22px] font-semibold tracking-[-0.02em]">{tree?.manifest.name ?? "Courion"}</h1>
        <p className="mt-1.5 text-[14px] leading-[21px] text-muted">
          {hasCollections
            ? "Pick a request from the sidebar, or start a new one."
            : "Start with a collection, or let an AI agent fill this workspace through MCP."}
        </p>
        <div className="-mx-3.5 mt-6">
          <Action icon={<Plus size={16} strokeWidth={1.6} />} title="New request" text="In the open collection" kbd="Ctrl N" onClick={() => void newRequestHere()} />
          <Action icon={<Import size={16} strokeWidth={1.5} />} title="Import from cURL" text="Paste a command, get a request" onClick={() => showImportDialog()} />
          <Action icon={<FolderOpen size={16} strokeWidth={1.5} />} title="New collection" text="Group the requests for one API" onClick={() => void createCollection()} />
          <Action icon={<Bot size={16} strokeWidth={1.5} />} title="Connect an AI agent" text="Let it register and test the endpoints it builds" onClick={() => setView("mcp")} />
          <Action icon={<GitBranch size={16} strokeWidth={1.5} />} title="Source control" text="Commit, pull, push and review with your team" onClick={() => setView("git")} />
        </div>
      </div>
    </div>
  );
}

export function NoWorkspace() {
  const show = useStore((s) => s.showWorkspaceDialog);
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-[460px] text-center">
        <div className="flex justify-center">
          <CourionMark size={40} />
        </div>
        <h1 className="mt-5 text-[22px] font-semibold tracking-[-0.02em]">Welcome to Courion</h1>
        <p className="mt-2 text-[14px] leading-[21px] text-muted">
          Workspaces are folders of plain YAML you keep in git, one per team or per service. Create one, open an existing folder, or clone your
          team&apos;s repository.
        </p>
        <div className="mt-6 flex justify-center gap-2">
          <button type="button" className="btn-primary h-9 px-3.5" onClick={() => show("create")}>
            <Plus size={14} /> New workspace
          </button>
          <button type="button" className="btn h-9 px-3.5" onClick={() => show("open")}>
            <FolderOpen size={14} /> Open folder
          </button>
          <button type="button" className="btn h-9 px-3.5" onClick={() => show("clone")}>
            <GitBranch size={14} /> Clone
          </button>
        </div>
      </div>
    </div>
  );
}
