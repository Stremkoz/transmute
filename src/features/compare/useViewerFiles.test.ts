import type { FileRow } from "@bindings";
import { describe, expect, it } from "vitest";
import { treeOrder } from "./useViewerFiles";

const row = (root: number, relativePath: string) => ({ root, relativePath }) as FileRow;

describe("viewer order", () => {
  it("follows the tree: inputs in turn, folders before files, then names", () => {
    const rows = [row(0, "b.jpg"), row(1, "a.jpg"), row(0, "Day 2/x.jpg"), row(0, "a.jpg"), row(0, "Day 1/z/y.jpg"), row(0, "Day 1/c.jpg")];
    expect(rows.sort(treeOrder).map((r) => `${r.root}:${r.relativePath}`)).toEqual([
      "0:Day 1/z/y.jpg",
      "0:Day 1/c.jpg",
      "0:Day 2/x.jpg",
      "0:a.jpg",
      "0:b.jpg",
      "1:a.jpg",
    ]);
  });
});
