import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { mockApi, renderApp } from "../test/render";

const WAIT = { timeout: 4000 };

function filesGrid() {
  return screen.getByRole("grid", { name: "Files" });
}

function bar() {
  return screen.getByRole("contentinfo");
}

/** Adds the test platform's folder and a WebP output, and waits for the plan. */
async function addFolderAndWebp() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add folder" }));
  await screen.findByRole("row", { name: "Holiday" });
  await user.click(screen.getByRole("button", { name: "Add output" }));
  await user.click(await screen.findByRole("menuitem", { name: /^WebP/ }));
  await within(bar()).findByText(/× 1 output →/, undefined, WAIT);
  return user;
}

async function convertAndFinish(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(bar()).getByRole("button", { name: "Convert" }));
  await within(bar()).findByText(/^Finished in/, undefined, WAIT);
}

describe("App", () => {
  it("starts with nothing added and nothing picked", async () => {
    renderApp();

    expect(await screen.findByText("Drop images or folders here")).toBeInTheDocument();
    const outputs = screen.getByRole("region", { name: "Outputs" });
    expect(within(outputs).getByText("Pick at least one format to convert to.")).toBeInTheDocument();
    expect(within(outputs).getByRole("button", { name: "Save to: Beside the originals" })).toBeInTheDocument();
    expect(within(bar()).getByRole("button", { name: "Convert" })).toBeDisabled();
    expect(within(bar()).getByText("Add images, then pick at least one output.")).toBeInTheDocument();
  });

  it("lists an added folder as a tree, opened at the top, and plans once an output is picked", async () => {
    renderApp();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Add folder" }));
    expect(await screen.findByRole("row", { name: "Holiday" })).toBeInTheDocument();
    const grid = filesGrid();
    expect(await within(grid).findByRole("row", { name: "Day 1" })).toBeInTheDocument();
    expect(within(grid).getByRole("row", { name: "photo-001.jpg" })).toBeInTheDocument();
    expect(within(grid).getByRole("row", { name: "notes.txt" })).toHaveTextContent("left alone");
    // Subfolders open on demand.
    expect(within(grid).queryByRole("row", { name: "Day 1/photo-002.jpg" })).not.toBeInTheDocument();
    await user.click(within(grid).getByRole("button", { name: "Open Day 1" }));
    expect(await within(grid).findByRole("row", { name: "Day 1/photo-002.jpg" })).toBeInTheDocument();
    expect(within(bar()).getByText("Pick an output format.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Add output" }));
    await user.click(await screen.findByRole("menuitem", { name: /^WebP/ }));

    expect(await within(bar()).findByText(/images? × 1 output →/, undefined, WAIT)).toBeInTheDocument();
    expect(within(bar()).getByRole("button", { name: "Convert" })).toBeEnabled();
    // Each file now shows its output, waiting.
    await vi.waitFor(() => expect(within(grid).getByRole("row", { name: "photo-001.jpg" })).toHaveTextContent("WebP"), WAIT);
    // The input's own row counts what's planned too.
    expect(within(grid).getByRole("row", { name: "Holiday" })).toHaveTextContent(/\d+ to convert/);
    expect(screen.getByRole("button", { name: /^Ready/ })).toBeInTheDocument();
  });

  it("adds files dropped on the window", async () => {
    const { platform } = renderApp();
    await screen.findByText("Drop images or folders here");

    act(() => platform.hover(true));
    expect(screen.getByText("Drop to add")).toBeInTheDocument();
    act(() => platform.drop(["C:\\Dropped\\a.jpg", "C:\\Dropped\\b.png"]));

    expect(await screen.findByRole("row", { name: "a.jpg" })).toBeInTheDocument();
    expect(screen.getByRole("row", { name: "b.png" })).toBeInTheDocument();
    expect(screen.queryByText("Drop to add")).not.toBeInTheDocument();
  });

  it("excludes a file, and the plan follows", async () => {
    renderApp();
    const user = await addFolderAndWebp();
    const before = within(bar()).getByText(/× 1 output →/).textContent ?? "";

    await user.click(within(filesGrid()).getByRole("checkbox", { name: "Include photo-001.jpg" }));

    await within(filesGrid()).findByRole("checkbox", { name: "Include photo-001.jpg", checked: false });
    await vi.waitFor(() => expect(within(bar()).getByText(/× 1 output →/).textContent).not.toBe(before), WAIT);
    expect(within(filesGrid()).getByRole("row", { name: "photo-001.jpg" })).toHaveTextContent("Excluded");
    expect(screen.getByRole("button", { name: /^Excluded/ })).toBeInTheDocument();
  });

  it("removes outputs and inputs again", async () => {
    renderApp();
    const user = await addFolderAndWebp();

    await user.click(screen.getByRole("button", { name: "Remove WebP output" }));
    expect(await within(bar()).findByText("Pick an output format.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove C:\\Pictures\\Holiday" }));
    expect(await screen.findByText("Drop images or folders here")).toBeInTheDocument();
  });

  it("describes each format, marks added ones and greys out formats no tool writes", async () => {
    renderApp();
    const user = await addFolderAndWebp();

    await user.click(screen.getByRole("button", { name: "Add output" }));
    const menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: /^WebPadded/ })).toHaveTextContent("opens in every browser");
    expect(within(menu).getByRole("menuitem", { name: /HEIF/ })).toHaveAttribute("aria-disabled", "true");
  });

  it("sends converted files to a chosen folder", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    const outputs = await screen.findByRole("region", { name: "Outputs" });

    await user.click(within(outputs).getByRole("button", { name: /^Save to:/ }));
    await user.click(await screen.findByRole("radio", { name: "A folder of my choice" }));
    expect(await within(outputs).findByRole("button", { name: "Save to: D:\\Converted" })).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "A folder per format inside it" }));

    await screen.findByRole("checkbox", { name: "A folder per format inside it", checked: true });
    expect((await api.preset()).destination).toEqual({ mode: "custom", root: "D:\\Converted", perFormatSubfolders: true });
    expect(within(outputs).getByRole("button", { name: "Save to: D:\\Converted, a folder per format" })).toBeInTheDocument();
  });

  it("keeps the destination when the folder picker is cancelled", async () => {
    const { api, platform } = renderApp();
    platform.destination = null;
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: /^Save to:/ }));
    await user.click(await screen.findByRole("radio", { name: "A folder of my choice" }));

    expect(screen.getByRole("radio", { name: /^Beside the originals/ })).toBeChecked();
    expect((await api.preset()).destination).toBeUndefined();
  });

  it("converts and keeps each file's results on screen", async () => {
    renderApp(mockApi({ runStepMs: 5, failEvery: 0 }));
    const user = await addFolderAndWebp();

    await convertAndFinish(user);

    expect(within(bar()).getByText(/written/)).toBeInTheDocument();
    expect(within(bar()).getByText(/^saved/)).toBeInTheDocument();
    // Nothing failed, so there's nothing left to convert.
    expect(within(bar()).getByRole("button", { name: "Convert" })).toBeDisabled();
    await vi.waitFor(
      () => expect(within(filesGrid()).getByRole("row", { name: "photo-001.jpg" })).toHaveTextContent(/WebP \d/),
      WAIT,
    );
    expect(await screen.findByRole("button", { name: /^Done/ }, WAIT)).toBeInTheDocument();
    const lastRun = await screen.findByRole("region", { name: "Last run" });
    expect(within(lastRun).getByText(/written ·/)).toBeInTheDocument();
  });

  it("shows failures with their reasons and converts what's left", async () => {
    renderApp(mockApi({ runStepMs: 5, failEvery: 4 }));
    const user = await addFolderAndWebp();
    await convertAndFinish(user);

    const failedLink = within(bar()).getByRole("button", { name: /failed$/ });
    const failed = Number.parseInt(failedLink.textContent ?? "0", 10);
    expect(failed).toBeGreaterThan(0);
    await user.click(failedLink);

    // A flat list of the failed files, with why.
    expect(screen.getByRole("button", { name: /^Failed/, pressed: true })).toBeInTheDocument();
    await vi.waitFor(() => expect(within(filesGrid()).getAllByRole("row", { name: /photo-/ })).toHaveLength(failed), WAIT);
    expect(within(filesGrid()).getAllByRole("row", { name: /photo-/ })[0]).toHaveTextContent("stopped with an error");

    await user.click(within(bar()).getByRole("button", { name: `Convert ${failed} left` }));
    await vi.waitFor(() => expect(within(bar()).getByText(/^Finished in/)).toBeInTheDocument(), WAIT);
  });

  it("clears finished files but keeps the failures", async () => {
    renderApp(mockApi({ runStepMs: 5, failEvery: 4 }));
    const user = await addFolderAndWebp();
    await convertAndFinish(user);
    const failed = Number.parseInt(within(bar()).getByRole("button", { name: /failed$/ }).textContent ?? "0", 10);

    await user.click(screen.getByRole("button", { name: "Clear" }));
    await user.click(await screen.findByRole("menuitem", { name: /Clear finished files/ }));

    // What's left are the failed files, ready to try again.
    await vi.waitFor(() => expect(within(bar()).getByText(/× 1 output →/)).toHaveTextContent(`→ ${failed} file`), WAIT);
  });

  it("clears the whole list from the finished bar", async () => {
    renderApp(mockApi({ runStepMs: 5, failEvery: 0 }));
    const user = await addFolderAndWebp();
    await convertAndFinish(user);

    await user.click(within(bar()).getByRole("button", { name: "Clear list" }));

    expect(await screen.findByText("Drop images or folders here")).toBeInTheDocument();
    // The outputs stay picked.
    expect(within(screen.getByRole("region", { name: "Outputs" })).getByText("WebP")).toBeInTheDocument();
  });

  it("shows a file's details, and its outputs once converted", async () => {
    const api = mockApi({ runStepMs: 5, failEvery: 0 });
    const reveal = vi.spyOn(api, "reveal");
    renderApp(api);
    const user = await addFolderAndWebp();

    await user.click(within(filesGrid()).getByRole("row", { name: "photo-001.jpg" }));
    const drawer = await screen.findByRole("dialog", { name: "photo-001.jpg" });
    expect(within(drawer).getByText(/recognised from its content/)).toBeInTheDocument();
    expect(within(drawer).getByText("EXIF, ICC profile")).toBeInTheDocument();
    expect(await within(drawer).findByText("Waiting")).toBeInTheDocument();
    await user.click(within(drawer).getByRole("button", { name: "Close" }));

    await convertAndFinish(user);
    await user.click(within(filesGrid()).getByRole("row", { name: "photo-001.jpg" }));
    const after = await screen.findByRole("dialog", { name: "photo-001.jpg" });
    expect(await within(after).findByText(/Made with cwebp/)).toBeInTheDocument();
    await user.click(within(after).getByRole("button", { name: "Show in folder" }));
    expect(reveal).toHaveBeenCalledWith({ kind: "output", job: expect.any(Number) });
  });

  it("saves the report where the user picks", async () => {
    const api = mockApi({ runStepMs: 5, failEvery: 0 });
    const saveReport = vi.spyOn(api, "saveReport");
    renderApp(api);
    const user = await addFolderAndWebp();
    await convertAndFinish(user);

    await user.click(await screen.findByRole("button", { name: "Save report…" }, WAIT));

    expect(saveReport).toHaveBeenCalledWith("D:\\report.txt");
  });

  it("pauses, resumes and cancels a run, and locks edits meanwhile", async () => {
    renderApp(mockApi({ runStepMs: 40 }));
    const user = await addFolderAndWebp();

    await user.click(within(bar()).getByRole("button", { name: "Convert" }));
    expect(await within(bar()).findByText("Converting")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove WebP output" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Add output" })).not.toBeInTheDocument();
    expect(screen.getByRole("note")).toHaveTextContent("locked while converting");
    expect(screen.getByRole("button", { name: "Add folder" })).toBeDisabled();

    await user.click(within(bar()).getByRole("button", { name: "Pause" }));
    expect(await within(bar()).findByText("Paused")).toBeInTheDocument();
    await user.click(within(bar()).getByRole("button", { name: "Resume" }));
    expect(await within(bar()).findByText("Converting")).toBeInTheDocument();
    await user.click(within(bar()).getByRole("button", { name: "Cancel" }));

    expect(await within(bar()).findByText(/^Finished in/, undefined, WAIT)).toBeInTheDocument();
    expect(within(bar()).getByText(/cancelled$/)).toBeInTheDocument();
    expect(within(bar()).getByRole("button", { name: /^Convert \d+ left$/ })).toBeEnabled();
  });

  it("shows a failed command and lets it be dismissed", async () => {
    const api = mockApi();
    api.addInputs = () => Promise.reject({ code: "io", message: "can't read C:\\Pictures\\Holiday" });
    renderApp(api);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Add folder" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("can't read C:\\Pictures\\Holiday");
    await user.click(within(screen.getByRole("alert")).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("lists the tools, with missing ones flagged", async () => {
    const user = userEvent.setup();
    renderApp();

    await user.click(await screen.findByRole("button", { name: /Tools.*2 missing/ }));

    const dialog = screen.getByRole("dialog", { name: "Tools" });
    const ffmpeg = within(dialog).getByRole("row", { name: /^FFmpeg/ });
    expect(within(ffmpeg).getByText("Not found")).toBeInTheDocument();
    const cjxl = within(dialog).getByRole("row", { name: /JPEG XL encoder/ });
    expect(within(cjxl).getByText("0.11.1")).toBeInTheDocument();
  });

  it("shows notices from the overview and from events, and dismisses them", async () => {
    const user = userEvent.setup();
    const api = mockApi({ notices: ["settings.json was damaged and has been backed up"] });
    renderApp(api);

    expect(await screen.findByText("settings.json was damaged and has been backed up")).toBeInTheDocument();
    act(() => api.emit({ type: "notice", text: "a run couldn't start" }));
    expect(screen.getByText("a run couldn't start")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("a run couldn't start")).not.toBeInTheDocument();
    expect((await api.overview()).notices).toEqual([]);
  });

  it("reports a session that fails to load", async () => {
    const api = mockApi();
    api.overview = () => Promise.reject({ code: "store", message: "can't tell where to keep settings" });
    renderApp(api);

    expect(await screen.findByRole("alert")).toHaveTextContent("can't tell where to keep settings");
  });
});
