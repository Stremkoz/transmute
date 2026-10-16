// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { COMMANDS, tauriApi } from "./tauriApi";

const shellSource = (file: string) => readFileSync(new URL(`../../src-tauri/src/${file}`, import.meta.url), "utf8");

// The shell and the client are kept in step by hand; these tests catch a
// command added on one side only.
describe("tauriApi", () => {
  it("names exactly the commands the shell defines", () => {
    const defined = [...shellSource("commands.rs").matchAll(/#\[tauri::command\]\s*pub async fn (\w+)/g)].map(
      (match) => match[1],
    );
    expect(Object.values(COMMANDS).sort()).toEqual(defined.sort());
  });

  it("names exactly the commands the shell registers", () => {
    const registered = [...shellSource("main.rs").matchAll(/commands::(\w+),/g)].map((match) => match[1]);
    expect(Object.values(COMMANDS).sort()).toEqual(registered.sort());
  });

  it("has a client method for every command", () => {
    for (const method of Object.keys(COMMANDS)) {
      expect(tauriApi).toHaveProperty(method);
    }
  });
});
