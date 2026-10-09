"use client";

import { Bot, FolderOpen, GitBranch, Plus, Star } from "lucide-react";

import { useStore } from "@/lib/store";

export function Welcome() {
  const tree = useStore((s) => s.tree);
  const { createCollection, setView } = useStore.getState();
  const hasCollections = (tree?.collections.length ?? 0) > 0;
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-[520px] text-center">
        <h1 className="text-[20px] font-semibold tracking-tight">{tree?.manifest.name ?? "http-trax"}</h1>
        <p className="mt-2 text-[13px] text-muted">
          {hasCollections
            ? "Pick a request from the sidebar, or create a new one."
            : "Start with a collection, or let an AI agent fill this workspace through MCP."}
        </p>
        <div className="mt-6 grid grid-cols-2 gap-3 text-left">
          <button className="rounded-lg border border-border bg-panel p-3 hover:border-border-strong hover:bg-hover" onClick={() => void createCollection()}>
            <Plus size={16} className="mb-1.5 text-accent" />
            <div className="font-medium">New collection</div>
            <div className="text-[12px] text-faint">Group requests for one API</div>
          </button>
          <button className="rounded-lg border border-border bg-panel p-3 hover:border-border-strong hover:bg-hover" onClick={() => setView("mcp")}>
            <Bot size={16} className="mb-1.5 text-accent" />
            <div className="font-medium">Connect an AI agent</div>
            <div className="text-[12px] text-faint">Let it register and test endpoints</div>
          </button>
          <button className="rounded-lg border border-border bg-panel p-3 hover:border-border-strong hover:bg-hover" onClick={() => setView("git")}>
            <GitBranch size={16} className="mb-1.5 text-accent" />
            <div className="font-medium">Source control</div>
            <div className="text-[12px] text-faint">Commit, pull, push, review</div>
          </button>
          <div className="rounded-lg border border-border bg-panel p-3">
            <Star size={16} className="mb-1.5 text-warn" />
            <div className="font-medium">Tip</div>
            <div className="text-[12px] text-faint">Star a collection to pin it; use the crosshair to hide all the others.</div>
          </div>
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
        <h1 className="text-[22px] font-semibold tracking-tight">Welcome to http-trax</h1>
        <p className="mt-2 text-[13px] leading-relaxed text-muted">
          Workspaces are folders of plain YAML you keep in git — one per team or per service. Create one, open an existing folder, or clone your
          team&apos;s repository.
        </p>
        <div className="mt-6 flex justify-center gap-2">
          <button className="btn-primary" onClick={() => show("create")}>
            <Plus size={14} /> New workspace
          </button>
          <button className="btn" onClick={() => show("open")}>
            <FolderOpen size={14} /> Open folder
          </button>
          <button className="btn" onClick={() => show("clone")}>
            <GitBranch size={14} /> Clone
          </button>
        </div>
      </div>
    </div>
  );
}
