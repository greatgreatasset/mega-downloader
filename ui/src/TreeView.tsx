import { useMemo, useState } from "react";
import { api } from "./api";
import {
  formatBytes,
  type ProgressMap,
  type Tree,
  type TreeNode,
} from "./types";

interface UiNode extends TreeNode {
  children: UiNode[];
  /** aggregate bytes for folders (sum of descendant files) */
  totalSize: number;
}

/** Build a nested tree from the flat node list using parent handles. */
function buildHierarchy(tree: Tree): UiNode[] {
  const byHandle = new Map<string, UiNode>();
  for (const n of tree.nodes) {
    byHandle.set(n.handle, { ...n, children: [], totalSize: n.size });
  }
  const roots: UiNode[] = [];
  for (const node of byHandle.values()) {
    if (node.parent && byHandle.has(node.parent)) {
      byHandle.get(node.parent)!.children.push(node);
    } else {
      roots.push(node);
    }
  }
  const sortRec = (n: UiNode): number => {
    n.children.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    let sum = n.kind === "file" ? n.size : 0;
    for (const c of n.children) sum += sortRec(c);
    n.totalSize = sum;
    return sum;
  };
  roots.forEach(sortRec);
  return roots;
}

/** All file handles in a subtree (folders contribute their descendant files). */
function fileHandlesOf(node: UiNode): string[] {
  if (node.kind === "file") return [node.handle];
  return node.children.flatMap(fileHandlesOf);
}

/** Sum downloaded bytes for a folder subtree. */
function doneBytes(node: UiNode, progress: ProgressMap): number {
  if (node.kind === "file") {
    const p = progress[node.handle];
    return p ? Math.min(p.bytesDone, node.size || p.bytesTotal) : 0;
  }
  return node.children.reduce((acc, c) => acc + doneBytes(c, progress), 0);
}

function Bar({ frac, error }: { frac: number; error?: boolean }) {
  return (
    <div className="h-1 w-full rounded bg-neutral-800 overflow-hidden">
      <div
        className={`h-full ${error ? "bg-rose-500" : frac >= 1 ? "bg-emerald-500" : "bg-indigo-500"}`}
        style={{ width: `${Math.min(100, Math.max(0, frac * 100))}%` }}
      />
    </div>
  );
}

function Row({
  node,
  depth,
  progress,
  jobId,
  onZip,
  selected,
  onToggle,
}: {
  node: UiNode;
  depth: number;
  progress: ProgressMap;
  jobId: string | null;
  onZip?: () => void;
  /** Selected file handles (selection mode is on when this is provided). */
  selected?: Set<string>;
  onToggle?: (handles: string[], checked: boolean) => void;
}) {
  const [open, setOpen] = useState(true);
  const isFolder = node.kind === "folder";
  const pad = { paddingLeft: `${depth * 16 + 8}px` };

  const fileProg = !isFolder ? progress[node.handle] : undefined;
  const done = isFolder ? doneBytes(node, progress) : (fileProg?.bytesDone ?? 0);
  const total = isFolder ? node.totalSize : node.size;
  const frac = total > 0 ? done / total : fileProg?.status === "done" ? 1 : 0;
  const showBar = isFolder ? done > 0 && done < total : !!fileProg;
  const isError = fileProg?.status === "error";

  // Selection state: a folder is checked when all its files are selected,
  // indeterminate when only some are.
  const subtreeHandles = useMemo(() => fileHandlesOf(node), [node]);
  const selCount = selected ? subtreeHandles.filter((h) => selected.has(h)).length : 0;
  const allSel = subtreeHandles.length > 0 && selCount === subtreeHandles.length;
  const someSel = selCount > 0 && !allSel;

  return (
    <div>
      <div
        className="flex items-center gap-2 py-1 pr-2 rounded hover:bg-neutral-800/60 cursor-default text-sm"
        style={pad}
        onClick={() => isFolder && setOpen((o) => !o)}
      >
        {selected && (
          <input
            type="checkbox"
            checked={allSel}
            ref={(el) => {
              if (el) el.indeterminate = someSel;
            }}
            disabled={subtreeHandles.length === 0}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => onToggle?.(subtreeHandles, e.target.checked)}
            className="h-4 w-4 shrink-0 accent-indigo-500 cursor-pointer disabled:opacity-30"
          />
        )}
        {isFolder ? (
          <span className="w-4 text-neutral-500 select-none">
            {node.children.length ? (open ? "▾" : "▸") : "·"}
          </span>
        ) : (
          <span className="w-4" />
        )}
        <span>{isFolder ? "📁" : "📄"}</span>
        <span
          className={`truncate ${isFolder ? "font-medium" : isError ? "text-rose-400" : "text-neutral-300"}`}
          title={isError ? fileProg?.error : node.name}
        >
          {node.name}
        </span>
        {fileProg?.note && fileProg.status === "active" && (
          <span className="shrink-0 text-xs text-amber-400">{fileProg.note}</span>
        )}
        <span className="ml-auto shrink-0 font-mono text-xs text-neutral-500">
          {isFolder
            ? `${node.children.length} · ${formatBytes(node.totalSize)}`
            : isError
              ? "failed"
              : fileProg && fileProg.status !== "done"
                ? `${formatBytes(done)} / ${formatBytes(total)}`
                : formatBytes(total)}
        </span>
        {fileProg?.status === "done" && (
          <span className="shrink-0 text-emerald-500 text-xs">✓</span>
        )}
        {isFolder && jobId && (
          <a
            href={api(`/api/jobs/${jobId}/zip?prefix=${encodeURIComponent(node.rel_path)}`)}
            onClick={(e) => {
              e.stopPropagation();
              onZip?.();
            }}
            title="Download this folder as a .zip"
            className="shrink-0 text-xs text-neutral-500 hover:text-indigo-400"
          >
            zip
          </a>
        )}
      </div>
      {showBar && (
        <div style={{ paddingLeft: `${depth * 16 + 32}px` }} className="pr-2 pb-1">
          <Bar frac={frac} error={isError} />
        </div>
      )}
      {isFolder && open && (
        <div>
          {node.children.map((c) => (
            <Row key={c.handle} node={c} depth={depth + 1} progress={progress} jobId={jobId} onZip={onZip} selected={selected} onToggle={onToggle} />
          ))}
        </div>
      )}
    </div>
  );
}

export default function TreeView({
  tree,
  progress,
  jobId,
  onZip,
  selected,
  onToggle,
}: {
  tree: Tree;
  progress: ProgressMap;
  jobId: string | null;
  onZip?: () => void;
  /** Selected file handles; when provided, rows show selection checkboxes. */
  selected?: Set<string>;
  onToggle?: (handles: string[], checked: boolean) => void;
}) {
  const roots = useMemo(() => buildHierarchy(tree), [tree]);
  const allHandles = useMemo(() => roots.flatMap(fileHandlesOf), [roots]);
  const selCount = selected ? allHandles.filter((h) => selected.has(h)).length : 0;
  const selectedBytes = useMemo(() => {
    if (!selected) return 0;
    const sel = new Set(allHandles.filter((h) => selected.has(h)));
    return tree.nodes.reduce((acc, n) => (n.kind === "file" && sel.has(n.handle) ? acc + n.size : acc), 0);
  }, [tree, allHandles, selected]);

  return (
    <div className="rounded-xl border border-neutral-800 bg-neutral-900">
      <div className="flex items-center justify-between border-b border-neutral-800 px-4 py-2 text-xs text-neutral-400">
        <span className="font-medium text-neutral-200">{tree.root_name}</span>
        {selected ? (
          <span className="flex items-center gap-3 font-mono">
            <span className="text-neutral-300">
              {selCount}/{allHandles.length} files · {formatBytes(selectedBytes)}
            </span>
            <button onClick={() => onToggle?.(allHandles, true)} className="rounded bg-neutral-700 px-2 py-0.5 font-sans hover:bg-neutral-600">
              All
            </button>
            <button onClick={() => onToggle?.(allHandles, false)} className="rounded bg-neutral-700 px-2 py-0.5 font-sans hover:bg-neutral-600">
              None
            </button>
          </span>
        ) : (
          <span className="font-mono">
            {tree.total_folders} folders · {tree.total_files} files ·{" "}
            {formatBytes(tree.total_bytes)}
          </span>
        )}
      </div>
      <div className="p-2 max-h-[26rem] overflow-auto">
        {roots.map((r) => (
          <Row key={r.handle} node={r} depth={0} progress={progress} jobId={jobId} onZip={onZip} selected={selected} onToggle={onToggle} />
        ))}
      </div>
    </div>
  );
}
