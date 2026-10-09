"use client";

import {
  AlertTriangle,
  Copy,
  Crosshair,
  FolderPlus,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Settings2,
  Star,
  Trash2,
} from "lucide-react";
import { useCallback, useMemo, useState } from "react";

import { useStore, useWorkspaceTabs } from "@/lib/store";
import type { GroupNode, RequestSummary, TreeItem } from "@/lib/types";
import { countRequests, cx } from "@/lib/util";

import { MethodPill } from "../ui/Brand";
import { MenuButton, useContextMenu, type MenuItem } from "../ui/Menu";
import { SidebarIconButton, SidebarSection } from "./Sidebar";

const EXPANDED_KEY = "http-trax.expanded.";
const INDENT = 14;

function loadExpanded(workspaceId: string | null): Set<string> {
  if (!workspaceId) return new Set();
  try {
    const raw = localStorage.getItem(EXPANDED_KEY + workspaceId);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}

/** Expanded folders, remembered per workspace. The panel is keyed by workspace. */
function useExpanded(workspaceId: string | null) {
  const [expanded, setExpanded] = useState<Set<string>>(() => loadExpanded(workspaceId));
  const toggle = useCallback(
    (id: string, open?: boolean) =>
      setExpanded((prev) => {
        const next = new Set(prev);
        const shouldOpen = open ?? !next.has(id);
        if (shouldOpen) next.add(id);
        else next.delete(id);
        try {
          if (workspaceId) localStorage.setItem(EXPANDED_KEY + workspaceId, JSON.stringify([...next]));
        } catch {
          // storage unavailable
        }
        return next;
      }),
    [workspaceId],
  );
  return { expanded, toggle };
}

function FolderIcon({ open }: { open: boolean }) {
  return open ? (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" className="shrink-0 text-muted" aria-hidden>
      <path d="M2 5.5V4a1.5 1.5 0 0 1 1.5-1.5h2.4l1.3 1.5h4.8A1.5 1.5 0 0 1 13.5 5.5V6" />
      <path d="M2.3 12.6 3.9 7.3a1 1 0 0 1 1-.7h9a.9.9 0 0 1 .9 1.2L13.1 12a1.5 1.5 0 0 1-1.4 1H3.8a1.4 1.4 0 0 1-1.5-.4z" />
    </svg>
  ) : (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" className="shrink-0 text-muted" aria-hidden>
      <path d="M1.8 4.4A1.5 1.5 0 0 1 3.3 2.9h2.4L7 4.4h5.7a1.5 1.5 0 0 1 1.5 1.5v5.8a1.5 1.5 0 0 1-1.5 1.5H3.3a1.5 1.5 0 0 1-1.5-1.5z" />
    </svg>
  );
}

/** Vertical guides showing nesting, one per ancestor level. */
function Guides({ depth }: { depth: number }) {
  return (
    <>
      {Array.from({ length: depth }, (_, i) => (
        <span key={i} aria-hidden className="absolute top-0 bottom-0 w-px bg-border-mid" style={{ left: 16 + i * INDENT }} />
      ))}
    </>
  );
}

export function CollectionsPanel() {
  const tree = useStore((s) => s.tree);
  const treeError = useStore((s) => s.treeError);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const entry = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId) ?? null);
  const createCollection = useStore((s) => s.createCollection);
  const setScope = useStore((s) => s.setScope);
  const showImportDialog = useStore((s) => s.showImportDialog);
  const { expanded, toggle } = useExpanded(workspaceId);

  const scoped = entry?.prefs.scopedCollection ?? null;
  const collections = useMemo(() => tree?.collections ?? [], [tree]);
  const scopedCollection = scoped ? collections.find((c) => c.meta.id === scoped) : undefined;

  const { pinned, others } = useMemo(() => {
    const bookmarks = entry?.prefs.bookmarks ?? [];
    const visible = scopedCollection ? [scopedCollection] : collections;
    // Bookmarked collections stay on top, in the order they were pinned.
    const pinned = bookmarks.map((id) => visible.find((c) => c.meta.id === id)).filter((c): c is GroupNode => !!c);
    const others = visible.filter((c) => !bookmarks.includes(c.meta.id));
    return { pinned, others };
  }, [collections, scopedCollection, entry]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <SidebarSection
        title={scopedCollection ? "Collection" : "Collections"}
        actions={
          <>
            <SidebarIconButton label="Import (cURL or Postman)" onClick={() => showImportDialog()}>
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M8 3v7.5M5 7.5l3 3 3-3M3.5 13h9" />
              </svg>
            </SidebarIconButton>
            <SidebarIconButton label="New collection" onClick={() => void createCollection()}>
              <Plus size={14} strokeWidth={1.6} />
            </SidebarIconButton>
          </>
        }
      />

      {scopedCollection && (
        <div className="mx-1.5 mt-1 mb-1.5 flex items-center gap-2 rounded-lg bg-accent-soft py-1.5 pr-1.5 pl-2.5 text-[12.5px] text-accent-text-2">
          <Crosshair size={13} className="shrink-0" />
          <span className="flex-1 truncate">
            Only <strong className="font-semibold">{scopedCollection.meta.name}</strong>
          </span>
          <button type="button" className="rounded-md px-2 py-0.5 text-[12px] font-medium hover:bg-accent-soft" onClick={() => void setScope(null)}>
            Show all
          </button>
        </div>
      )}

      {tree && tree.problems.length > 0 && (
        <div
          className="mx-1.5 mt-1 mb-1.5 flex items-start gap-2 rounded-lg bg-warn/12 px-2.5 py-1.5 text-[12.5px] text-warn"
          title={tree.problems.map((p) => `${p.path}: ${p.message}`).join("\n")}
        >
          <AlertTriangle size={13} className="mt-0.5 shrink-0" />
          <span>
            {tree.problems.length} file{tree.problems.length > 1 ? "s" : ""} couldn&apos;t be loaded. Check Source control for merge conflicts.
          </span>
        </div>
      )}

      <nav aria-label="Collection tree" className="flex min-h-0 flex-1 flex-col gap-px overflow-y-auto pt-0.5 pb-2.5" role="tree">
        {treeError && <p className="px-2 py-3 text-[13px] text-danger">{treeError}</p>}
        {!tree && !treeError && <p className="px-2 py-3 text-[13px] text-faint">Loading…</p>}
        {tree && collections.length === 0 && (
          <div className="mx-1 mt-3 rounded-[10px] border border-dashed border-border-strong px-4 py-5 text-center">
            <p className="text-[13.5px] text-text-2">No collections yet</p>
            <p className="mt-1 text-[12.5px] text-faint">Group the requests for one API, import from Postman, or let an AI agent create them over MCP.</p>
            <button type="button" className="btn-primary mt-4" onClick={() => void createCollection()}>
              <Plus size={14} /> New collection
            </button>
          </div>
        )}
        {pinned.length > 0 && (
          <>
            <SectionLabel icon={<Star size={10} className="fill-current" />} label="Pinned" />
            {pinned.map((c) => (
              <GroupRow key={c.meta.id} node={c} depth={0} isCollection bookmarked expanded={expanded} toggle={toggle} forceOpen={false} />
            ))}
            {others.length > 0 && <div className="mx-2 my-1.5 h-px bg-border" />}
          </>
        )}
        {others.map((c) => (
          <GroupRow key={c.meta.id} node={c} depth={0} isCollection bookmarked={false} expanded={expanded} toggle={toggle} forceOpen={false} />
        ))}
      </nav>
    </div>
  );
}

function SectionLabel({ label, icon }: { label: string; icon?: React.ReactNode }) {
  return (
    <div className="mt-1 mb-0.5 flex items-center gap-1.5 px-2.5 text-[12px] font-medium text-faint">
      {icon}
      {label}
    </div>
  );
}

interface NodeProps {
  expanded: Set<string>;
  toggle: (id: string, open?: boolean) => void;
  forceOpen: boolean;
}

function groupMenu(node: GroupNode, isCollection: boolean, open: () => void): MenuItem[] {
  const s = useStore.getState();
  const bookmarked = s.activeEntry()?.prefs.bookmarks.includes(node.meta.id) ?? false;
  const scoped = s.activeEntry()?.prefs.scopedCollection === node.meta.id;
  const items: MenuItem[] = [
    { label: "New request", icon: <Plus size={14} />, onSelect: () => (open(), void s.createRequest(node.meta.id)) },
    { label: "New folder", icon: <FolderPlus size={14} />, onSelect: () => (open(), void s.createFolder(node.meta.id)) },
    { label: isCollection ? "Run collection" : "Run folder", icon: <Play size={14} />, onSelect: () => void s.runGroup(node.meta.id) },
    { label: "", separator: true },
  ];
  if (isCollection) {
    items.push(
      {
        label: bookmarked ? "Remove bookmark" : "Bookmark (keep on top)",
        icon: <Star size={14} />,
        onSelect: () => void s.toggleBookmark(node.meta.id),
      },
      {
        label: scoped ? "Show all collections" : "Show only this collection",
        icon: <Crosshair size={14} />,
        onSelect: () => void s.setScope(scoped ? null : node.meta.id),
      },
      { label: "", separator: true },
    );
  }
  items.push(
    { label: "Settings, variables and scripts", icon: <Settings2 size={14} />, onSelect: () => void s.openGroup(node.meta.id) },
    { label: "Rename", icon: <Pencil size={14} />, onSelect: () => void s.renameGroup(node.meta.id) },
    { label: "Delete", icon: <Trash2 size={14} />, danger: true, onSelect: () => void s.deleteGroup(node.meta.id) },
  );
  return items;
}

function GroupRow({
  node,
  depth,
  isCollection,
  bookmarked,
  ...props
}: NodeProps & { node: GroupNode; depth: number; isCollection: boolean; bookmarked: boolean }) {
  const open = props.forceOpen || props.expanded.has(node.meta.id);
  const { toggleBookmark, setScope, createRequest } = useStore.getState();
  const scoped = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.scopedCollection === node.meta.id);
  const ctx = useContextMenu(() => groupMenu(node, isCollection, () => props.toggle(node.meta.id, true)));
  const count = countRequests(node.items);

  return (
    <div role="treeitem" aria-expanded={open} aria-selected={false}>
      <div className="relative">
        <Guides depth={depth} />
        <div
          onContextMenu={ctx.open}
          className={cx("group flex h-[30px] items-center rounded-[7px] pr-1 hover:bg-hover", ctx.isOpen && "bg-hover")}
          style={{ paddingLeft: 8 + depth * INDENT }}
        >
          <button
            type="button"
            className="flex h-full min-w-0 flex-1 items-center gap-2 text-left text-[13.5px] text-text"
            onClick={() => props.toggle(node.meta.id)}
          >
            <FolderIcon open={open} />
            <span className="truncate">{node.meta.name}</span>
          </button>
          {isCollection && (
            <>
              <button
                type="button"
                title={bookmarked ? "Remove bookmark" : "Bookmark: keep this collection on top"}
                aria-label={bookmarked ? `Remove bookmark from ${node.meta.name}` : `Bookmark ${node.meta.name}`}
                aria-pressed={bookmarked}
                onClick={() => void toggleBookmark(node.meta.id)}
                className={cx(
                  "flex h-6 w-6 items-center justify-center rounded-md hover:bg-selected",
                  bookmarked ? "text-warn" : "text-faint opacity-0 group-hover:opacity-100 focus:opacity-100",
                )}
              >
                <Star size={13} className={bookmarked ? "fill-current" : undefined} />
              </button>
              <button
                type="button"
                title={scoped ? "Show all collections" : "Show only this collection"}
                aria-label={scoped ? "Show all collections" : `Show only ${node.meta.name}`}
                aria-pressed={scoped}
                onClick={() => void setScope(scoped ? null : node.meta.id)}
                className={cx(
                  "flex h-6 w-6 items-center justify-center rounded-md hover:bg-selected",
                  scoped ? "text-accent-text" : "text-faint opacity-0 group-hover:opacity-100 focus:opacity-100",
                )}
              >
                <Crosshair size={13} />
              </button>
            </>
          )}
          {!isCollection && (
            <button
              type="button"
              aria-label={`New request in ${node.meta.name}`}
              title="New request"
              onClick={() => {
                props.toggle(node.meta.id, true);
                void createRequest(node.meta.id);
              }}
              className="flex h-6 w-6 items-center justify-center rounded-md text-faint opacity-0 group-hover:opacity-100 hover:bg-selected focus:opacity-100"
            >
              <Plus size={14} />
            </button>
          )}
          <MenuButton
            items={() => groupMenu(node, isCollection, () => props.toggle(node.meta.id, true))}
            className="flex h-6 w-6 items-center justify-center rounded-md text-faint opacity-0 group-hover:opacity-100 hover:bg-selected focus:opacity-100"
            title="More"
          >
            <MoreHorizontal size={14} />
          </MenuButton>
          {!open && count > 0 && <span className="w-5 text-right text-[12px] text-faint group-hover:hidden">{count}</span>}
        </div>
      </div>
      {open && <Items items={node.items} depth={depth + 1} {...props} />}
      {ctx.menu}
    </div>
  );
}

function Items({ items, depth, ...props }: NodeProps & { items: TreeItem[]; depth: number }) {
  if (items.length === 0) {
    return (
      <div className="relative">
        <Guides depth={depth} />
        <p className="py-1.5 text-[12.5px] text-faint" style={{ paddingLeft: 12 + depth * INDENT }}>
          Empty
        </p>
      </div>
    );
  }
  return (
    <div role="group" className="flex flex-col gap-px">
      {items.map((item) =>
        item.kind === "folder" ? (
          <GroupRow key={item.meta.id} node={item} depth={depth} isCollection={false} bookmarked={false} {...props} />
        ) : (
          <RequestRow key={item.id} request={item} depth={depth} />
        ),
      )}
    </div>
  );
}

function RequestRow({ request, depth }: { request: RequestSummary; depth: number }) {
  const { active } = useWorkspaceTabs();
  const { openRequest, duplicateRequest, deleteRequest } = useStore.getState();
  const selected = active?.kind === "request" && active.requestId === request.id;
  const ctx = useContextMenu(() => [
    { label: "Open", onSelect: () => void openRequest(request.id) },
    { label: "Duplicate", icon: <Copy size={14} />, onSelect: () => void duplicateRequest(request.id) },
    { label: "", separator: true },
    { label: "Delete", icon: <Trash2 size={14} />, danger: true, onSelect: () => void deleteRequest(request.id) },
  ]);
  return (
    <div className="relative">
      <Guides depth={depth} />
      <button
        type="button"
        role="treeitem"
        aria-selected={selected}
        onClick={() => void openRequest(request.id)}
        onContextMenu={ctx.open}
        title={`${request.method} ${request.url}`}
        className={cx(
          "relative flex h-8 w-full items-center gap-2.5 rounded-[7px] pr-2 text-left text-[13.5px]",
          selected ? "bg-selected text-text" : "text-text-2 hover:bg-hover",
          ctx.isOpen && "bg-hover",
        )}
        style={{ paddingLeft: 12 + depth * INDENT }}
      >
        {selected && <span className="absolute top-0 bottom-0 left-0 w-[3px] rounded-sm bg-accent-line" />}
        <MethodPill method={request.method} />
        <span className="truncate">{request.name}</span>
      </button>
      {ctx.menu}
    </div>
  );
}
