import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Preset } from "@bindings";
import { mockApi, renderApp } from "../test/render";

const WAIT = { timeout: 4000 };

const ARCHIVE: Preset = {
  name: "Archive",
  inputFilter: { mode: "all" },
  outputs: [{ codec: { format: "jxl", lossless: true } }],
};

/** The outputs column, also while a drawer or menu hides it from the accessibility tree. */
function outputs() {
  return screen.getByRole("region", { name: "Outputs", hidden: true });
}

async function addOutput(user: ReturnType<typeof userEvent.setup>, format: RegExp) {
  await user.click(await screen.findByRole("button", { name: "Add output" }));
  await user.click(await screen.findByRole("menuitem", { name: format }));
}

describe("Output settings", () => {
  it("shows real settings on the card and edits them in the drawer", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await addOutput(user, /^WebP/);
    expect(await within(outputs()).findByText("Lossy · quality 85 · method 4")).toBeInTheDocument();

    await user.click(within(outputs()).getByRole("button", { hidden: true, name: "Edit WebP settings" }));
    const drawer = await screen.findByRole("dialog", { name: "WebP settings" });
    expect(within(drawer).getByRole("slider", { name: "Quality" })).toBeInTheDocument();
    await user.click(within(drawer).getByRole("button", { name: "Lossless" }));

    // Lossless has no quality.
    await vi.waitFor(() => expect(within(drawer).queryByRole("slider", { name: "Quality" })).not.toBeInTheDocument());
    expect((await api.preset()).outputs[0]?.codec).toEqual({ format: "webp", lossless: true });
    expect(within(outputs()).getByText("Lossless · method 4")).toBeInTheDocument();

    // Back to what it inherits.
    await user.click(within(drawer).getByRole("button", { name: "use default" }));
    await vi.waitFor(async () => expect((await api.preset()).outputs[0]?.codec).toEqual({ format: "webp" }));
  });

  it("makes an output's settings the user's defaults", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await addOutput(user, /^AVIF/);
    await user.click(await within(outputs()).findByRole("button", { hidden: true, name: "Edit AVIF settings" }));
    const drawer = await screen.findByRole("dialog", { name: "AVIF settings" });

    await user.click(within(drawer).getByRole("button", { name: /^60 small/ }));
    await vi.waitFor(async () => expect((await api.preset()).outputs[0]?.codec).toEqual({ format: "avif", quality: 60 }));
    await user.click(within(drawer).getByRole("button", { name: "Make these my defaults" }));

    await vi.waitFor(async () => expect((await api.defaults()).codecs).toEqual([{ format: "avif", quality: 60 }]));
    // The output now inherits the same value.
    await vi.waitFor(async () => expect((await api.preset()).outputs[0]?.codec).toEqual({ format: "avif" }));
    expect(await within(outputs()).findByText("Lossy · quality 60 · effort 4")).toBeInTheDocument();
    expect(within(drawer).getByRole("button", { name: "Make these my defaults" })).toBeDisabled();
  });

  it("picks the colour transparent areas become in JPEG", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await addOutput(user, /^JPEG(?! XL)/);
    await user.click(await within(outputs()).findByRole("button", { hidden: true, name: "Edit JPEG settings" }));
    const drawer = await screen.findByRole("dialog", { name: "JPEG settings" });

    await user.click(within(drawer).getByRole("button", { name: "Black" }));
    await vi.waitFor(async () =>
      expect((await api.preset()).outputs[0]?.codec).toEqual({ format: "jpeg", background: "#000000" }),
    );
    expect(within(outputs()).getByText("Quality 85 · progressive · background #000000")).toBeInTheDocument();
  });

  it("gives one output its own file names", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await addOutput(user, /^WebP/);
    await user.click(await within(outputs()).findByRole("button", { hidden: true, name: "Edit WebP settings" }));
    const drawer = await screen.findByRole("dialog", { name: "WebP settings" });

    await user.click(within(drawer).getByRole("button", { name: /^File names:/ }));
    await user.click(await screen.findByRole("checkbox", { name: "Same as all outputs" }));
    await user.type(screen.getByRole("textbox", { name: "After the name" }), "_web");

    expect(screen.getByText("beach_web.webp")).toBeInTheDocument();
    await vi.waitFor(async () => expect((await api.preset()).outputs[0]?.naming?.suffix).toBe("_web"));
    expect(within(outputs()).getByText("own file names")).toBeInTheDocument();
  });
});

describe("Where and how", () => {
  it("previews file names and holds back half-typed tokens", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "File names: Unchanged" }));
    const suffix = await screen.findByRole("textbox", { name: "After the name" });

    // `{` starts a token; it's typed with `{{` in user-event.
    await user.type(suffix, "_{{da");
    expect(screen.getByRole("alert")).toHaveTextContent(/Braces/);
    // The last name that made sense stays applied.
    await vi.waitFor(async () => expect((await api.preset()).naming?.suffix).toBe("_"));

    await user.type(suffix, "te}");
    expect(await screen.findByText("beach_20261015.webp")).toBeInTheDocument();
    await vi.waitFor(async () => expect((await api.preset()).naming?.suffix).toBe("_{date}"));
  });

  // The backend leaves an empty prefix or suffix out of the preset, which
  // once crashed the window as soon as the editor opened again.
  it("reopens the file names with one part left empty", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "File names: Unchanged" }));
    await user.type(await screen.findByRole("textbox", { name: "Before the name" }), "{{date}_");
    const suffix = screen.getByRole("textbox", { name: "After the name" });
    await user.type(suffix, "x");
    await user.clear(suffix);
    await vi.waitFor(async () => expect((await api.preset()).naming).not.toHaveProperty("suffix"));
    await user.keyboard("{Escape}");

    await user.click(await screen.findByRole("button", { name: "File names: {date}_‹name›" }));
    expect(await screen.findByRole("textbox", { name: "After the name" })).toHaveValue("");
    expect(screen.getByRole("textbox", { name: "Before the name" })).toHaveValue("{date}_");
  });

  it("reopens an output's own file names with one part left empty", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await addOutput(user, /^WebP/);
    const edit = await within(outputs()).findByRole("button", { hidden: true, name: "Edit WebP settings" });
    await user.click(edit);
    let drawer = await screen.findByRole("dialog", { name: "WebP settings" });
    await user.click(within(drawer).getByRole("button", { name: /^File names:/ }));
    await user.click(await screen.findByRole("checkbox", { name: "Same as all outputs" }));
    await user.type(screen.getByRole("textbox", { name: "Before the name" }), "web_");
    await vi.waitFor(async () => expect((await api.preset()).outputs[0]?.naming).not.toHaveProperty("suffix"));
    await user.keyboard("{Escape}");
    await user.keyboard("{Escape}");

    await user.click(edit);
    drawer = await screen.findByRole("dialog", { name: "WebP settings" });
    await user.click(within(drawer).getByRole("button", { name: "File names: web_‹name›" }));
    expect(await screen.findByRole("textbox", { name: "After the name" })).toHaveValue("");
  });

  it("chooses what happens to existing files and metadata", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "If it exists: Skip it" }));
    await user.click(await screen.findByRole("radio", { name: /^Replace it/ }));
    await vi.waitFor(async () => expect((await api.preset()).collision).toBe("overwrite"));

    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Metadata: Keep all" }));
    await user.click(await screen.findByRole("radio", { name: /^Copyright only/ }));
    await vi.waitFor(async () => expect((await api.preset()).metadata).toBe("copyrightOnly"));
  });

  it("leaves source formats out", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Add folder" }));
    await user.click(await screen.findByRole("button", { name: "All formats" }));
    await user.click(await screen.findByRole("menuitemcheckbox", { name: /^HEIF/ }));

    await vi.waitFor(async () =>
      expect((await api.preset()).inputFilter).toEqual({ mode: "exclude", formats: ["heif"] }),
    );
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("button", { name: "All but HEIF" })).toBeInTheDocument();
  });
});

describe("Presets", () => {
  it("saves the setup as a preset, shows edits and undoes them", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await addOutput(user, /^WebP/);
    await within(outputs()).findByText("Lossy · quality 85 · method 4");

    fireEvent.keyDown(window, { key: "S", ctrlKey: true, shiftKey: true });
    const dialog = await screen.findByRole("dialog", { name: "Save as a new preset" });
    expect(within(dialog).getByText("WebP: Lossy · quality 85 · method 4")).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox", { name: "Name" }), "Web");
    await user.click(within(dialog).getByRole("button", { name: "Save preset" }));

    expect(await screen.findByRole("button", { name: "Preset: Web" })).toBeInTheDocument();
    expect((await api.storedPresets()).map((p) => p.id)).toEqual(["web"]);

    // An edit marks it; undo brings the stored one back.
    await addOutput(user, /^AVIF/);
    const trigger = await screen.findByRole("button", { name: "Preset: Web, edited" }, WAIT);
    await user.click(trigger);
    await user.click(await screen.findByRole("menuitem", { name: "Undo my changes" }));
    expect(await screen.findByRole("button", { name: "Preset: Web" })).toBeInTheDocument();
    expect((await api.preset()).outputs).toHaveLength(1);
  });

  it("switches presets and starts over", async () => {
    const { api } = renderApp(mockApi({ presets: { archive: ARCHIVE } }));
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Preset: Untitled" }));
    await user.click(await screen.findByRole("menuitem", { name: /^Archive/ }));
    expect(await screen.findByRole("button", { name: "Preset: Archive" })).toBeInTheDocument();
    expect(await within(outputs()).findByText(/^Lossless · effort automatic/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Preset: Archive" }));
    await user.click(await screen.findByRole("menuitem", { name: /^Untitled \(no preset\)/ }));
    expect(await screen.findByRole("button", { name: "Preset: Untitled" })).toBeInTheDocument();
    expect((await api.preset()).outputs).toEqual([]);
  });

  it("renames, deletes and imports presets", async () => {
    const { api } = renderApp(mockApi({ presets: { archive: ARCHIVE } }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Preset: Untitled" }));
    await user.click(await screen.findByRole("menuitem", { name: /^Manage presets/ }));
    const dialog = await screen.findByRole("dialog", { name: "Presets" });

    await user.click(within(dialog).getByRole("button", { name: "Rename" }));
    const field = within(dialog).getByRole("textbox", { name: "New name for Archive" });
    await user.clear(field);
    await user.type(field, "Keepers{Enter}");
    expect(await within(dialog).findByText("Keepers")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Import…" }));
    expect(await within(dialog).findByText("Shared")).toBeInTheDocument();

    await user.click(within(dialog).getAllByRole("button", { name: "Delete" })[0] as HTMLElement);
    await user.click(within(within(dialog).getByRole("alert")).getByRole("button", { name: "Delete" }));
    await vi.waitFor(async () => expect((await api.storedPresets()).map((p) => p.preset.name)).toEqual(["Shared"]));
  });
});

describe("Settings", () => {
  it("switches between light, dark and the system's theme", async () => {
    const { api, platform } = renderApp();
    const user = userEvent.setup();
    // jsdom can't tell the system's preference, which reads as dark.
    await vi.waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
    await user.click(await screen.findByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });
    const appearance = within(dialog).getByRole("group", { name: "Appearance" });
    expect(within(appearance).getByRole("button", { name: "System", pressed: true })).toBeInTheDocument();

    await user.click(within(appearance).getByRole("button", { name: "Light" }));
    await vi.waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    expect(platform.windowTheme).toBe("light");
    await vi.waitFor(async () => expect((await api.settings()).theme).toBe("light"));

    await user.click(within(appearance).getByRole("button", { name: "System" }));
    await vi.waitFor(() => expect(document.documentElement.dataset.theme).toBe("dark"));
    expect(platform.windowTheme).toBeNull();
    await vi.waitFor(async () => expect(await api.settings()).not.toHaveProperty("theme"));
  });

  it("sets how many files convert at once and the default metadata", async () => {
    const { api } = renderApp();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Settings" }));
    const dialog = await screen.findByRole("dialog", { name: "Settings" });

    await user.click(within(dialog).getByRole("checkbox", { name: /^Automatic/ }));
    await vi.waitFor(async () => expect((await api.settings()).workers).toBeGreaterThan(0));

    await user.click(within(dialog).getByRole("button", { name: "Metadata: Keep all" }));
    await user.click(await screen.findByRole("radio", { name: /^Remove all/ }));
    await vi.waitFor(async () => expect((await api.defaults()).metadata).toBe("stripAll"));
  });
});
