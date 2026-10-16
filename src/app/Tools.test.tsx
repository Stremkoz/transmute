import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MOCK_TOOLS } from "../api/mockApi";
import { mockApi, renderApp } from "../test/render";

function toolRow(dialog: HTMLElement, name: RegExp) {
  const row = within(dialog)
    .getAllByRole("listitem")
    .find((item) => name.test(item.textContent ?? ""));
  if (!row) throw new Error(`no tool row matching ${name}`);
  return row;
}

describe("Tools dialog", () => {
  it("says what each tool does and how to get a missing one, without flagging optional ones", async () => {
    const user = userEvent.setup();
    renderApp();
    // FFmpeg is missing, but it's optional: nothing to fix.
    await user.click(await screen.findByRole("button", { name: "Tools" }));
    const dialog = screen.getByRole("dialog", { name: "Tools" });

    const ffmpeg = toolRow(dialog, /^FFmpeg/);
    expect(within(ffmpeg).getByText("Not installed")).toBeInTheDocument();
    expect(within(ffmpeg).getByText("winget install Gyan.FFmpeg")).toBeInTheDocument();
    const cjxl = toolRow(dialog, /JPEG XL encoder/);
    expect(within(cjxl).getByText("0.11.1")).toBeInTheDocument();
    expect(cjxl).toHaveTextContent("The only installed tool that writes JPEG XL");
  });

  it("flags a missing bundled tool", async () => {
    const tools = MOCK_TOOLS.map((info) =>
      info.tool === "cwebp" ? { tool: "cwebp" as const, status: { status: "missing" as const }, reads: [], writes: [], notes: [] } : info,
    );
    renderApp(mockApi({ tools }));
    expect(await screen.findByRole("button", { name: /Tools.*1 to fix/ })).toBeInTheDocument();
  });

  it("uses another copy of a tool, and goes back to the one found", async () => {
    const api = mockApi();
    const setSettings = vi.spyOn(api, "setSettings");
    const user = userEvent.setup();
    renderApp(api);
    await user.click(await screen.findByRole("button", { name: "Tools" }));
    const dialog = screen.getByRole("dialog", { name: "Tools" });

    await user.click(within(toolRow(dialog, /^cwebp/)).getByRole("button", { name: "Use another copy…" }));
    await vi.waitFor(() => expect(setSettings).toHaveBeenLastCalledWith({ toolPaths: { cwebp: "D:\\Tools\\cwebp.exe" } }));

    await user.click(await within(toolRow(dialog, /^cwebp/)).findByRole("button", { name: "Use the one Transmute finds" }));
    await vi.waitFor(() => expect(setSettings).toHaveBeenLastCalledWith({ toolPaths: {} }));
  });
});
