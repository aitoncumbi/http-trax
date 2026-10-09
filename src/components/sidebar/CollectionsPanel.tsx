"use client";

import {
  AlertTriangle,
  ChevronRight,
  Copy,
  Crosshair,
  Folder,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Search,
  Settings2,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useMemo, useState } from "react";

import { useStore, useWorkspaceTabs } from "@/lib/store";
import type { GroupNode, RequestSummary, TreeItem } from "@/lib/types";
import { countRequests, cx, filterTree, methodColor } from "@/lib/util";

import { MenuButton, useContextMenu, type MenuItem } from "../ui/Menu";

const EXPANDED_KEY = "http-trax.expanded.";

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

export function CollectionsPanel() {
  const tree = useStore((s) => s.tree);
  const treeError = useStore((s) => s.treeError);
  const workspaceId = useStore((s) => s.activeWorkspaceId);
  const entry = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId) ?? null);
  const createCollection = useStore((s) => s.createCollection);
  const setScope = useStore((s) => s.setScope);
  const [query, setQuery] = useState("");
  const { expanded, toggle } = useExpanded(workspaceId);

  const scoped = entry?.prefs.scopedCollection ?? null;
  const collections = useMemo(() => tree?.collections ?? [], [tree]);
  const scopedCollection = scoped ? collections.find((c) => c.meta.id === scoped) : undefined;

  const { pinned, others } = useMemo(() => {
    const bookmarks = entry?.prefs.bookmarks ?? [];
    const visible = scopedCollection ? [scopedCollection] : collections;
    const filtered = query
      ? visible
          .map((c) => {
            const items = filterTree(c.items, query);
            const nameHit = c.meta.name.toLowerCase().includes(query.trim().toLowerCase());
            return nameHit || items.length ? { ...c, items: nameHit ? c.items : items } : null;
          })
          .filter((c): c is GroupNode => c !== null)
      : visible;
    // Bookmarked collections stay on top, in the order they were pinned.
    const pinned = bookmarks
      .map((id) => filtered.find((c) => c.meta.id === id))
      .filter((c): c is GroupNode => !!c);
    const others = filtered.filter((c) => !bookmarks.includes(c.meta.id));
    return { pinned, others };
  }, [collections, scopedCollection, entry, query]);

  const forceOpen = query.trim().length > 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between px-3 pt-3 pb-2">
        <h2 className="text-[11px] font-semibold tracking-wider text-faint uppercase">Collections</h2>
        <div className="flex items-center">
          <button className="icon-btn" title="New collection" onClick={() => void createCollection()}>
            <Plus size={15} />
          </button>
        </div>
      </div>
      <div className="px-3 pb-2">
        <div className="relative">
          <Search size={13} className="absolute top-1/2 left-2.5 -translate-y-1/2 text-faint" />
          <input
            className="input h-7 pl-7 text-[12.5px]"
            placeholder="Filter requests"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>

      {scopedCollection && (
        <div className="mx-3 mb-2 flex items-center gap-2 rounded-md border border-accent/30 bg-accent-soft px-2.5 py-1.5 text-[12px]">
          <Crosshair size={13} className="shrink-0 text-accent" />
          <span className="flex-1 truncate">
            Scoped to <strong className="font-semibold">{scopedCollection.meta.name}</strong>
          </span>
          <button className="btn-ghost h-6 px-1.5 text-[11.5px] text-accent" onClick={() => void setScope(null)}>
            Show all
          </button>
        </div>
      )}

      {tree && tree.problems.length > 0 && (
        <div
          className="mx-3 mb-2 flex items-start gap-2 rounded-md border border-warn/30 bg-warn/10 px-2.5 py-1.5 text-[12px] text-warn"
          title={tree.problems.map((p) => `${p.path}: ${p.message}`).join("\n")}
        >
          <AlertTriangle size={13} className="mt-0.5 shrink-0" />
          <span>
            {tree.problems.length} file{tree.problems.length > 1 ? "s" : ""} couldn&apos;t be loaded (unresolved merge conflict?). See Source
            control.
          </span>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-4" role="tree">
        {treeError && <p className="px-2 py-3 text-[12.5px] text-danger">{treeError}</p>}
        {!tree && !treeError && <p className="px-2 py-3 text-[12.5px] text-faint">Loading…</p>}
        {tree && collections.length === 0 && (
          <div className="mx-1.5 mt-2 rounded-lg border border-dashed border-border-strong p-4 text-center">
            <p className="text-[12.5px] text-muted">No collections yet.</p>
            <button className="btn-primary mt-3" onClick={() => void createCollection()}>
              <Plus size={14} /> New collection
            </button>
            <p className="mt-3 text-[11.5px] text-faint">Or let an AI agent create them through MCP.</p>
          </div>
        )}
        {pinned.length > 0 && (
          <>
            <SectionLabel icon={<Star size={11} className="fill-current" />} label="Pinned" />
            {pinned.map((c) => (
              <CollectionNode key={c.meta.id} node={c} expanded={expanded} toggle={toggle} forceOpen={forceOpen} bookmarked />
            ))}
          </>
        )}
        {others.length > 0 && pinned.length > 0 && <SectionLabel label="All collections" />}
        {others.map((c) => (
          <CollectionNode key={c.meta.id} node={c} expanded={expanded} toggle={toggle} forceOpen={forceOpen} bookmarked={false} />
        ))}
        {tree && collections.length > 0 && pinned.length + others.length === 0 && (
          <p className="px-2 py-3 text-[12.5px] text-faint">No requests match “{query}”.</p>
        )}
      </div>
    </div>
  );
}

function SectionLabel({ label, icon }: { label: string; icon?: React.ReactNode }) {
  return (
    <div className="mt-2 mb-0.5 flex items-center gap-1.5 px-2 text-[10.5px] font-semibold tracking-wider text-faint uppercase">
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
    { label: "New request", icon: <Plus size={13} />, onSelect: () => (open(), void s.createRequest(node.meta.id)) },
    { label: "New folder", icon: <FolderPlus size={13} />, onSelect: () => (open(), void s.createFolder(node.meta.id)) },
    { label: isCollection ? "Run collection" : "Run folder", icon: <Play size={13} />, onSelect: () => void s.runGroup(node.meta.id) },
    { label: "", separator: true },
  ];
  if (isCollection) {
    items.push(
      {
        label: bookmarked ? "Remove bookmark" : "Bookmark (pin to top)",
        icon: <Star size={13} />,
        onSelect: () => void s.toggleBookmark(node.meta.id),
      },
      {
        label: scoped ? "Show all collections" : "Scope to this collection",
        icon: <Crosshair size={13} />,
        onSelect: () => void s.setScope(scoped ? null : node.meta.id),
      },
      { label: "", separator: true },
    );
  }
  items.push(
    { label: "Settings, variables & scripts", icon: <Settings2 size={13} />, onSelect: () => void s.openGroup(node.meta.id) },
    { label: "Rename", icon: <Pencil size={13} />, onSelect: () => void s.renameGroup(node.meta.id) },
    { label: "Delete", icon: <Trash2 size={13} />, danger: true, onSelect: () => void s.deleteGroup(node.meta.id) },
  );
  return items;
}

function CollectionNode({ node, bookmarked, ...props }: NodeProps & { node: GroupNode; bookmarked: boolean }) {
  const open = props.forceOpen || props.expanded.has(node.meta.id);
  const toggleBookmark = useStore((s) => s.toggleBookmark);
  const setScope = useStore((s) => s.setScope);
  const scoped = useStore((s) => s.workspaces.find((w) => w.id === s.activeWorkspaceId)?.prefs.scopedCollection === node.meta.id);
  const ctx = useContextMenu(() => groupMenu(node, true, () => props.toggle(node.meta.id, true)));
  const count = countRequests(node.items);

  return (
    <div role="treeitem" aria-expanded={open} aria-selected={false}>
      <div
        onContextMenu={ctx.open}
        className={cx("group flex h-8 items-center gap-1 rounded-md pr-1 pl-1 hover:bg-hover", ctx.isOpen && "bg-hover")}
      >
        <button className="flex min-w-0 flex-1 items-center gap-1.5 text-left" onClick={() => props.toggle(node.meta.id)}>
          <ChevronRight size={14} className={cx("shrink-0 text-faint transition-transform", open && "rotate-90")} />
          {open ? <FolderOpen size={14} className="shrink-0 text-accent" /> : <Folder size={14} className="shrink-0 text-accent" />}
          <span className="truncate font-medium">{node.meta.name}</span>
          <span className="shrink-0 text-[11px] text-faint">{count}</span>
        </button>
        <button
          title={bookmarked ? "Remove bookmark" : "Bookmark: keep this collection on top"}
          aria-label="Bookmark"
          aria-pressed={bookmarked}
          onClick={() => void toggleBookmark(node.meta.id)}
          className={cx("icon-btn h-6 w-6", bookmarked ? "text-warn" : "opacity-0 group-hover:opacity-100 focus:opacity-100")}
        >
          <Star size={13} className={bookmarked ? "fill-current" : undefined} />
        </button>
        <button
          title={scoped ? "Show all collections" : "Scope to this collection (hide the others)"}
          aria-label="Scope to this collection"
          aria-pressed={scoped}
          onClick={() => void setScope(scoped ? null : node.meta.id)}
          className={cx("icon-btn h-6 w-6", scoped ? "text-accent" : "opacity-0 group-hover:opacity-100 focus:opacity-100")}
        >
          {scoped ? <X size={13} /> : <Crosshair size={13} />}
        </button>
        <MenuButton
          items={() => groupMenu(node, true, () => props.toggle(node.meta.id, true))}
          className="icon-btn h-6 w-6 opacity-0 group-hover:opacity-100 focus:opacity-100"
          title="More"
        >
          <MoreHorizontal size={14} />
        </MenuButton>
      </div>
      {open && <Items items={node.items} depth={1} {...props} />}
      {ctx.menu}
    </div>
  );
}

function Items({ items, depth, ...props }: NodeProps & { items: TreeItem[]; depth: number }) {
  if (items.length === 0) {
    return (
      <p className="py-1 text-[12px] text-faint" style={{ paddingLeft: depth * 14 + 26 }}>
        Empty
      </p>
    );
  }
  return (
    <div role="group">
      {items.map((item) =>
        item.kind === "folder" ? (
          <FolderNode key={item.meta.id} node={item} depth={depth} {...props} />
        ) : (
          <RequestNode key={item.id} request={item} depth={depth} />
        ),
      )}
    </div>
  );
}

function FolderNode({ node, depth, ...props }: NodeProps & { node: GroupNode; depth: number }) {
  const open = props.forceOpen || props.expanded.has(node.meta.id);
  const createRequest = useStore((s) => s.createRequest);
  const ctx = useContextMenu(() => groupMenu(node, false, () => props.toggle(node.meta.id, true)));
  return (
    <div role="treeitem" aria-expanded={open} aria-selected={false}>
      <div
        onContextMenu={ctx.open}
        className={cx("group flex h-7 items-center gap-1 rounded-md pr-1 hover:bg-hover", ctx.isOpen && "bg-hover")}
        style={{ paddingLeft: depth * 14 + 4 }}
      >
        <button className="flex min-w-0 flex-1 items-center gap-1.5 text-left" onClick={() => props.toggle(node.meta.id)}>
          <ChevronRight size={13} className={cx("shrink-0 text-faint transition-transform", open && "rotate-90")} />
          {open ? <FolderOpen size={13} className="shrink-0 text-muted" /> : <Folder size={13} className="shrink-0 text-muted" />}
          <span className="truncate">{node.meta.name}</span>
        </button>
        <button
          title="New request"
          onClick={() => {
            props.toggle(node.meta.id, true);
            void createRequest(node.meta.id);
          }}
          className="icon-btn h-6 w-6 opacity-0 group-hover:opacity-100 focus:opacity-100"
        >
          <Plus size={13} />
        </button>
        <MenuButton
          items={() => groupMenu(node, false, () => props.toggle(node.meta.id, true))}
          className="icon-btn h-6 w-6 opacity-0 group-hover:opacity-100 focus:opacity-100"
          title="More"
        >
          <MoreHorizontal size={14} />
        </MenuButton>
      </div>
      {open && <Items items={node.items} depth={depth + 1} {...props} />}
      {ctx.menu}
    </div>
  );
}

function RequestNode({ request, depth }: { request: RequestSummary; depth: number }) {
  const { active } = useWorkspaceTabs();
  const { openRequest, duplicateRequest, deleteRequest } = useStore.getState();
  const isActive = active?.kind === "request" && active.requestId === request.id;
  const ctx = useContextMenu(() => [
    { label: "Open", onSelect: () => void openRequest(request.id) },
    { label: "Duplicate", icon: <Copy size={13} />, onSelect: () => void duplicateRequest(request.id) },
    { label: "", separator: true },
    { label: "Delete", icon: <Trash2 size={13} />, danger: true, onSelect: () => void deleteRequest(request.id) },
  ]);
  return (
    <>
      <button
        role="treeitem"
        aria-selected={isActive}
        onClick={() => void openRequest(request.id)}
        onContextMenu={ctx.open}
        title={request.url}
        className={cx(
          "flex h-7 w-full items-center gap-2 rounded-md pr-2 text-left",
          isActive ? "bg-accent-soft text-text" : "hover:bg-hover",
          ctx.isOpen && "bg-hover",
        )}
        style={{ paddingLeft: depth * 14 + 22 }}
      >
        <span className={cx("w-[42px] shrink-0 font-mono text-[10.5px] font-bold", methodColor(request.method))}>
          {request.method.length > 6 ? request.method.slice(0, 6) : request.method}
        </span>
        <span className="truncate">{request.name}</span>
      </button>
      {ctx.menu}
    </>
  );
}
