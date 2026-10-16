import { describe, expect, it } from "vitest";
import { geometry } from "./geometry";

const STAGE = { width: 800, height: 600 };

describe("compare viewer geometry", () => {
  it("fits a large image without enlarging a small one", () => {
    expect(geometry(STAGE, { width: 4000, height: 3000 }, "fit", { x: 0, y: 0 })).toMatchObject({
      left: 0,
      top: 0,
      width: 800,
      height: 600,
    });
    expect(geometry(STAGE, { width: 400, height: 300 }, "fit", { x: 0, y: 0 })).toMatchObject({
      left: 200,
      top: 150,
      width: 400,
    });
  });

  it("pans a zoomed image only as far as its edges", () => {
    const place = geometry(STAGE, { width: 1600, height: 600 }, 1, { x: 9999, y: 50 });
    expect(place.slack).toEqual({ x: 400, y: 0 });
    expect(place.left).toBe(0);
    expect(place.top).toBe(0);
  });
});
