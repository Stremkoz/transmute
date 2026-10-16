import { useEffect, useMemo, useState } from "react";
import type { FileInfo, FileCounts, RootInfo, TreeNode } from "@bindings";
import { useApi, useSession } from "../../state/sessionContext";

/** One drawn row of the tree. */
export type TreeRow =
  | { kind: "root"; key: string; root: RootInfo; open: boolean }
  | { kind: "folder"; key: string; root: number; name: string; relativePath: string; counts: FileCounts; open: boolean; depth: number }
  | { kind: "file"; key: string; root: number; name: string; relativePath: string; file: FileInfo; depth: number }
  | { kind: "loading"; key: string; depth: number };

const NO_ROOTS: RootInfo[] = [];

/** Folders are keyed by their root's path, which survives renumbering. */
function folderKey(rootPath: string, dir: string): string {
  return `${rootPath}\u0000${dir}`;
}

/**
 * The files list as a tree: inputs, their folders (listed when opened) and
 * files. Opened folders are refetched whenever the session says the list
 * changed, so states and counts follow the plan and the run.
 */
export function useFileTree() {
  const api = useApi();
  const roots = useSession((state) => state.overview?.inputs.roots ?? NO_ROOTS);
  const version = useSession((state) => state.listVersion);
  // Inputs start open, subfolders closed; this records the exceptions.
  const [toggled, setToggled] = useState<Set<string>>(() => new Set());
  const [children, setChildren] = useState<Map<string, TreeNode[]>>(() => new Map());

  const isOpen = (key: string, dir: string) => toggled.has(key) !== (dir === "");

  // What's open (or a file input, which is listed as its one file).
  const wanted = useMemo(() => {
    const keys: Array<{ key: string; root: number; dir: string }> = [];
    for (const root of roots) {
      const top = folderKey(root.path, "");
      if (!root.isFolder || isOpen(top, "")) keys.push({ key: top, root: root.id, dir: "" });
    }
    for (const key of toggled) {
      const [rootPath = "", dir = ""] = key.split("\u0000");
      const root = roots.find((r) => r.path === rootPath);
      if (root && dir !== "") keys.push({ key, root: root.id, dir });
    }
    return keys;
    // isOpen only reads `toggled`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [roots, toggled]);

  useEffect(() => {
    let current = true;
    void Promise.all(
      wanted.map(async ({ key, root, dir }) => {
        try {
          return [key, await api.tree(root, dir)] as const;
        } catch {
          return [key, [] as TreeNode[]] as const;
        }
      }),
    ).then((loaded) => {
      if (current) setChildren(new Map(loaded));
    });
    return () => {
      current = false;
    };
  }, [api, wanted, version]);

  const rows = useMemo(() => {
    const out: TreeRow[] = [];
    const addChildren = (root: RootInfo, dir: string, depth: number) => {
      const nodes = children.get(folderKey(root.path, dir));
      if (!nodes) {
        out.push({ kind: "loading", key: `${folderKey(root.path, dir)}\u0000loading`, depth });
        return;
      }
      for (const node of nodes) {
        const key = folderKey(root.path, node.relativePath);
        if (node.node.kind === "folder") {
          const open = isOpen(key, node.relativePath);
          out.push({ kind: "folder", key, root: root.id, name: node.name, relativePath: node.relativePath, counts: node.node.counts, open, depth });
          if (open) addChildren(root, node.relativePath, depth + 1);
        } else {
          out.push({ kind: "file", key, root: root.id, name: node.name, relativePath: node.relativePath, file: node.node, depth });
        }
      }
    };
    for (const root of roots) {
      const top = folderKey(root.path, "");
      if (!root.isFolder) {
        addChildren(root, "", 0);
        continue;
      }
      const open = isOpen(top, "");
      out.push({ kind: "root", key: top, root, open });
      if (open) addChildren(root, "", 1);
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [roots, children, toggled]);

  const toggle = (key: string) =>
    setToggled((previous) => {
      const next = new Set(previous);
      if (!next.delete(key)) next.add(key);
      return next;
    });

  return { rows, toggle };
}
