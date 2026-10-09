"use client";

import { Plus } from "lucide-react";
import { useState, type ReactNode } from "react";

import { useStore } from "@/lib/store";
import { envTone } from "@/lib/util";

import { WorkspaceAvatar } from "../ui/Brand";
import { MenuButton, type MenuItem } from "../ui/Menu";
import { WorkspaceSwitcher } from "../WorkspaceSwitcher";

function CircleButton({ label, onClick, children, disabled }: { label: string; onClick: () => void; children: ReactNode; disabled?: boolean }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-border-strong text-text-2 hover:bg-hover hover:text-text disabled:opacity-40"
    >
      {children}
    </button>
  );
}

function Header() {
  const entry = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId) ?? null);
  const local = useStore((s) => {
    const state = s.activeWorkspaceId ? s.sync[s.activeWorkspaceId]?.state : undefined;
    return state === "local-only" || state === "not-repo";
  });
  const { openPalette, newRequestHere } = useStore.getState();
  const [open, setOpen] = useState(false);
  return (
    <div className="relative flex items-center gap-2 pt-0.5 pb-3">
      <button
        type="button"
        aria-label={`Workspace: ${entry?.name ?? "none"}`}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg pr-1.5 pl-1 text-left text-[14px] leading-none font-semibold hover:bg-hover"
      >
        {entry && <WorkspaceAvatar id={entry.id} name={entry.name} local={local && !entry.team} />}
        <span className="truncate">{entry?.name ?? "No workspace"}</span>
        <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" className="shrink-0 text-faint" aria-hidden>
          <path d="m4 6 4 4 4-4" />
        </svg>
      </button>
      <CircleButton label="Search or run a command (Ctrl K)" onClick={() => openPalette()}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden>
          <circle cx="7" cy="7" r="4.5" />
          <path d="m10.5 10.5 3 3" />
        </svg>
      </CircleButton>
      <CircleButton label="New request (Ctrl N)" onClick={() => void newRequestHere()} disabled={!entry}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M3 13l.6-3L10.8 2.8a1.4 1.4 0 0 1 2 2L5.6 12 3 13zM9.5 4.2l2.3 2.3" />
        </svg>
      </CircleButton>
      {open && <WorkspaceSwitcher onClose={() => setOpen(false)} />}
    </div>
  );
}

function EnvironmentPicker() {
  const tree = useStore((s) => s.tree);
  const activeEnvId = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.activeEnvironment ?? null);
  const { setActiveEnvironment, createEnvironment, setView } = useStore.getState();
  const envs = tree?.environments ?? [];
  const active = envs.find((e) => e.id === activeEnvId) ?? null;
  const dot = (name: string | null) => <span className="h-2 w-2 rounded-full" style={{ background: envTone(name) }} />;
  const items: MenuItem[] = [
    { label: "No Environment", icon: dot(null), hint: active ? undefined : "Active", onSelect: () => void setActiveEnvironment(null) },
    ...envs.map((e) => ({
      label: e.name,
      icon: dot(e.name),
      hint: e.id === activeEnvId ? "Active" : e.protected ? "Protected" : undefined,
      onSelect: () => void setActiveEnvironment(e.id),
    })),
    { label: "", separator: true },
    { label: "Manage environments", onSelect: () => setView("environments") },
    { label: "New environment…", icon: <Plus size={14} />, onSelect: () => void createEnvironment() },
  ];
  return (
    <div className="flex items-center gap-2 pt-2.5">
      <MenuButton
        items={items}
        minWidth={220}
        title="Environment (Ctrl+E)"
        className="flex h-[30px] min-w-0 flex-1 items-center gap-2 rounded-lg border border-border-mid px-2.5 text-left text-[13px] leading-none text-text-2 hover:bg-hover"
      >
        <span className="h-[7px] w-[7px] shrink-0 rounded-full" style={{ background: envTone(active?.name) }} aria-hidden />
        <span className="min-w-0 flex-1 truncate">{active?.name ?? "No Environment"}</span>
        <svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" className="shrink-0 text-faint" aria-hidden>
          <path d="m4 6 4 4 4-4" />
        </svg>
      </MenuButton>
    </div>
  );
}

/** The sidebar frame: workspace header, the panel for the current view, environment picker. */
export function Sidebar({ width, children }: { width: number; children: ReactNode }) {
  return (
    <aside aria-label="Sidebar" className="flex min-h-0 shrink-0 flex-col pt-1 pr-2 pb-2.5 pl-1" style={{ width }}>
      <Header />
      <div className="flex min-h-0 flex-1 flex-col">{children}</div>
      <EnvironmentPicker />
    </aside>
  );
}

/** Section title inside the sidebar ("Collections", "Clients"), with optional actions. */
export function SidebarSection({ title, actions }: { title: string; actions?: ReactNode }) {
  return (
    <div className="flex shrink-0 items-center justify-between pt-3 pr-1.5 pb-1 pl-2.5">
      <span className="text-[12px] font-medium text-faint">{title}</span>
      {actions && <span className="inline-flex gap-0.5">{actions}</span>}
    </div>
  );
}

export function SidebarIconButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted hover:bg-hover hover:text-text"
    >
      {children}
    </button>
  );
}
