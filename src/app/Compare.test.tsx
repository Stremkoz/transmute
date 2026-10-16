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
    // The dialog opens before the converted file's preview has loaded.
    expect(await within(viewer).findByRole("img", { name: "Output" }, WAIT)).toBeInTheDocument();
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
    await vi.waitFor(() => expect(screen.getAllByRole("img", { name: /Original|Output/ })).toHaveLength(2), WAIT);
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

/** A dialog's name, from the title it's labelled by. */
function accessibleName(dialog: HTMLElement): string {
  const id = dialog.getAttribute("aria-labelledby");
  return (id && document.getElementById(id)?.textContent) || "";
}

describe("Automatic lossless", () => {
  it("says which copy Auto kept for each file", async () => {
    renderApp(mockApi({ runStepMs: 5 }));
    const user = userEvent.setup();
    await addFolderAndWebp(user);
    await user.click(screen.getByRole("button", { name: "Edit WebP settings" }));
    const drawer = await screen.findByRole("dialog", { name: "WebP settings" });
    await user.click(within(drawer).getByRole("button", { name: "Auto" }));
    expect(await within(drawer).findByRole("button", { name: "Auto", pressed: true })).toBeInTheDocument();
    expect(await screen.findByText("Auto lossless · quality 85 · method 4")).toBeInTheDocument();

    // The example is a JPEG: a photo, encoded once, lossy.
    // Choosing Auto re-plans; the button waits for that.
    const tryIt = await within(drawer).findByRole("button", { name: "Try on one file…" }, WAIT);
    await vi.waitFor(() => expect(tryIt).toBeEnabled(), WAIT);
    await user.click(tryIt);
    let viewer = await screen.findByRole("dialog", { description: /trial/ }, WAIT);
    expect(await within(viewer).findByRole("img", { name: "Output" }, WAIT)).toBeInTheDocument();
    expect(within(viewer).queryByText(/Auto kept/)).not.toBeInTheDocument();

    // A PNG is tried both ways: step through the files to one.
    const trial = () => screen.getByRole("dialog", { description: /trial/ });
    for (let i = 0; i < 20 && !/\.png$/.test(accessibleName(trial())); i++) {
      const before = accessibleName(trial());
      fireEvent.keyDown(window, { key: "ArrowRight" });
      await vi.waitFor(() => expect(accessibleName(trial())).not.toBe(before), WAIT);
    }
    viewer = trial();
    expect(accessibleName(viewer)).toMatch(/\.png$/);
    expect(await within(viewer).findByText(/Auto kept lossless: 10% smaller than lossy/, undefined, WAIT)).toBeInTheDocument();
    expect(within(viewer).getByText(/^WebP, lossless · method 4/)).toBeInTheDocument();
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
