import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { mockApi, renderApp } from "../test/render";

const WAIT = { timeout: 4000 };

function grid() {
  return screen.getByRole("grid", { name: "Files", hidden: true });
}

async function addFolderAndWebp(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Add folder" }));
  await screen.findByRole("row", { name: "Holiday" });
  await user.click(screen.getByRole("button", { name: "Add output" }));
  await user.click(await screen.findByRole("menuitem", { name: /^WebP/ }));
  await within(screen.getByRole("contentinfo")).findByText(/× 1 output →/, undefined, WAIT);
}

async function convert(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(screen.getByRole("contentinfo")).getByRole("button", { name: "Convert" }));
  await within(screen.getByRole("contentinfo")).findByText(/^Finished in/, undefined, WAIT);
}

describe("Compare viewer", () => {
  it("compares a converted file with its original and steps through the results", async () => {
    renderApp(mockApi({ runStepMs: 5, failEvery: 0 }));
    const user = userEvent.setup();
    await addFolderAndWebp(user);
    await convert(user);

    await user.click(await within(grid()).findByRole("row", { name: "photo-001.jpg" }));
    const drawer = await screen.findByRole("dialog", { name: "photo-001.jpg" });
    await user.click(await within(drawer).findByRole("button", { name: "Compare" }));

    const viewer = await screen.findByRole("dialog", { name: "photo-001.jpg", description: /file \d+ of \d+/ }, WAIT);
    expect(within(viewer).getByRole("img", { name: "Output" })).toBeInTheDocument();
    expect(within(viewer).getByRole("button", { name: /^WebP · / })).toHaveAttribute("aria-pressed", "true");
    const table = within(viewer).getByRole("table");
    expect(within(table).getByRole("row", { name: /File size/ })).toHaveTextContent(/smaller/);

    // Space shows the original in the output's place.
    fireEvent.keyDown(window, { key: " " });
    expect(within(viewer).queryByRole("img", { name: "Output" })).not.toBeInTheDocument();
    fireEvent.keyUp(window, { key: " " });
    expect(within(viewer).getByRole("img", { name: "Output" })).toBeInTheDocument();

    // → goes to the next converted file.
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(await screen.findByRole("dialog", { name: "photo-004.png", description: /file \d+ of/ }, WAIT)).toBeInTheDocument();

    await user.click(within(screen.getByRole("dialog", { name: "photo-004.png" })).getByRole("button", { name: "Side by side" }));
    expect(screen.getAllByRole("img", { name: /Original|Output/ })).toHaveLength(2);
  });

  it("opens on double-click of a converted row", async () => {
    renderApp(mockApi({ runStepMs: 5, failEvery: 0 }));
    const user = userEvent.setup();
    await addFolderAndWebp(user);
    await convert(user);

    await user.dblClick(await within(grid()).findByRole("row", { name: "photo-001.jpg" }));
    expect(await screen.findByRole("dialog", { name: "photo-001.jpg", description: /file \d+ of \d+/ }, WAIT)).toBeInTheDocument();
  });

  it("says when an output is no longer there", async () => {
    const api = mockApi({ runStepMs: 5, failEvery: 0 });
    const preview = api.preview;
    api.preview = (target) =>
      target.kind === "output"
        ? Promise.reject({ code: "unavailable", message: "gone" })
        : preview(target);
    renderApp(api);
    const user = userEvent.setup();
    await addFolderAndWebp(user);
    await convert(user);

    await user.dblClick(await within(grid()).findByRole("row", { name: "photo-001.jpg" }));
    expect(await screen.findByText("The WebP file is no longer there (moved or deleted?).", undefined, WAIT)).toBeInTheDocument();
  });
});

describe("Try on one file", () => {
  it("converts the example file into a trial from the codec drawer", async () => {
    const api = mockApi({ runStepMs: 5 });
    const sample = vi.spyOn(api, "sample");
    renderApp(api);
    const user = userEvent.setup();
    await addFolderAndWebp(user);

    await user.click(screen.getByRole("button", { name: "Edit WebP settings" }));
    const drawer = await screen.findByRole("dialog", { name: "WebP settings" });
    await user.click(await within(drawer).findByRole("button", { name: "Try on one file…" }, WAIT));

    const viewer = await screen.findByRole("dialog", { description: /trial/ }, WAIT);
    expect(await within(viewer).findByRole("img", { name: "Output" }, WAIT)).toBeInTheDocument();
    expect(within(viewer).getByText(/WebP \(trial\) ·/)).toBeInTheDocument();
    expect(within(within(viewer).getByRole("table")).getByText(/made in/)).toBeInTheDocument();
    expect(sample).toHaveBeenCalledWith(expect.any(Number), 0);
    // Nothing was planned to run because of it.
    expect((await api.overview()).run).toBeNull();
  });
});

describe("Thumbnails", () => {
  it("shows small previews when switched on, and remembers the choice", async () => {
    const api = mockApi();
    const thumbnail = vi.spyOn(api, "thumbnail");
    const { unmount } = renderApp(api);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Add folder" }));
    await screen.findByRole("row", { name: "Holiday" });
    expect(thumbnail).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Thumbnails" }));
    expect(screen.getByRole("button", { name: "Thumbnails" })).toHaveAttribute("aria-pressed", "true");
    await vi.waitFor(() => expect(thumbnail).toHaveBeenCalled());
    const row = within(grid()).getByRole("row", { name: "photo-001.jpg" });
    await vi.waitFor(() => expect(row.querySelector("img")).not.toBeNull());
    unmount();

    renderApp(mockApi());
    await user.click(await screen.findByRole("button", { name: "Add folder" }));
    expect(await screen.findByRole("button", { name: "Thumbnails" })).toHaveAttribute("aria-pressed", "true");
    localStorage.clear();
  });
});
