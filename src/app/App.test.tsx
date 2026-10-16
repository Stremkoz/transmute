import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { mockApi, renderApp } from "../test/render";

const WAIT = { timeout: 3000 };

async function addFolderAndWebp() {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add folder" }));
  await screen.findByRole("list", { name: "Added files and folders" });
  await user.click(screen.getByRole("button", { name: "Add output" }));
  await user.click(await screen.findByRole("menuitem", { name: "WebP" }));
  await screen.findByText(/× 1 output →/, undefined, WAIT);
  return user;
}

describe("App", () => {
  it("starts with nothing added and nothing picked", async () => {
    renderApp();

    expect(await screen.findByText("Drop images or folders here")).toBeInTheDocument();
    const outputs = screen.getByRole("region", { name: "Outputs" });
    expect(within(outputs).getByText("Pick at least one format to convert to.")).toBeInTheDocument();
    expect(within(outputs).getByRole("radio", { name: "Beside each source" })).toBeChecked();
    expect(screen.getByRole("button", { name: "Convert" })).toBeDisabled();
    expect(screen.getByText("Add images to get started.")).toBeInTheDocument();
  });

  it("adds a folder, then plans as soon as an output is picked", async () => {
    renderApp();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Add folder" }));
    const roots = await screen.findByRole("list", { name: "Added files and folders" });
    expect(within(roots).getByText("C:\\Pictures\\Holiday")).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Images by format" })).toHaveTextContent("JPEG");
    expect(screen.getByText("Pick an output format.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Add output" }));
    await user.click(await screen.findByRole("menuitem", { name: "WebP" }));

    expect(await screen.findByText(/images? × 1 output →/, undefined, WAIT)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Convert" })).toBeEnabled();
  });

  it("adds files dropped on the window", async () => {
    const { platform } = renderApp();
    await screen.findByText("Drop images or folders here");

    act(() => platform.hover(true));
    expect(screen.getByText("Drop to add")).toBeInTheDocument();
    act(() => platform.drop(["C:\\Dropped\\a.jpg", "C:\\Dropped\\b.png"]));

    const roots = await screen.findByRole("list", { name: "Added files and folders" });
    expect(within(roots).getAllByRole("listitem")).toHaveLength(2);
    expect(screen.queryByText("Drop to add")).not.toBeInTheDocument();
  });

  it("removes outputs and inputs again", async () => {
    renderApp();
    const user = await addFolderAndWebp();

    await user.click(screen.getByRole("button", { name: "Remove WebP output" }));
    expect(await screen.findByText("Pick an output format.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove C:\\Pictures\\Holiday" }));
    expect(await screen.findByText("Drop images or folders here")).toBeInTheDocument();
  });

  it("offers each format once and greys out formats no tool writes", async () => {
    renderApp();
    const user = await addFolderAndWebp();

    await user.click(screen.getByRole("button", { name: "Add output" }));
    const menu = await screen.findByRole("menu");
    expect(within(menu).queryByRole("menuitem", { name: "WebP" })).not.toBeInTheDocument();
    expect(within(menu).getByRole("menuitem", { name: /HEIF/ })).toHaveAttribute("aria-disabled", "true");
  });

  it("sends converted files to a chosen folder", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    const outputs = await screen.findByRole("region", { name: "Outputs" });

    await user.click(within(outputs).getByRole("radio", { name: "A folder of my choice" }));
    expect(await within(outputs).findByText("D:\\Converted")).toBeInTheDocument();
    await user.click(within(outputs).getByRole("checkbox", { name: "Separate folder per format" }));

    await screen.findByRole("checkbox", { name: "Separate folder per format", checked: true });
    expect((await api.preset()).destination).toEqual({
      mode: "custom",
      root: "D:\\Converted",
      perFormatSubfolders: true,
    });
  });

  it("keeps the destination when the folder picker is cancelled", async () => {
    const { api, platform } = renderApp();
    platform.destination = null;
    const user = userEvent.setup();

    await user.click(await screen.findByRole("radio", { name: "A folder of my choice" }));

    expect(screen.getByRole("radio", { name: "Beside each source" })).toBeChecked();
    expect((await api.preset()).destination).toBeUndefined();
  });

  it("converts, shows the summary, and re-plans what's left", async () => {
    renderApp();
    const user = await addFolderAndWebp();

    await user.click(screen.getByRole("button", { name: "Convert" }));

    expect(await screen.findByRole("heading", { name: "Finished" }, WAIT)).toBeInTheDocument();
    expect(screen.getByText(/^Converted .* files? in/)).toBeInTheDocument();
    expect(screen.getByText(/smaller/)).toBeInTheDocument();
    // The outputs exist now, so the next plan skips them.
    expect(await screen.findByText(/skipped/, undefined, WAIT)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Convert" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Back to inputs" }));
    expect(screen.getByRole("list", { name: "Added files and folders" })).toBeInTheDocument();
  });

  it("pauses, resumes and cancels a run, and locks edits meanwhile", async () => {
    renderApp(mockApi({ runStepMs: 40 }));
    const user = await addFolderAndWebp();

    await user.click(screen.getByRole("button", { name: "Convert" }));
    expect(await screen.findByRole("heading", { name: "Converting" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove WebP output" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add output" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Pause" }));
    expect(await screen.findByRole("heading", { name: "Paused" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Resume" }));
    expect(await screen.findByRole("heading", { name: "Converting" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(await screen.findByRole("heading", { name: "Finished" }, WAIT)).toBeInTheDocument();
    expect(screen.getByText(/cancelled\./)).toBeInTheDocument();
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
