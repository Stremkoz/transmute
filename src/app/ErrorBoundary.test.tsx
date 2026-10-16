import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function Broken(): never {
  throw new Error("settings.toolPaths is undefined");
}

describe("ErrorBoundary", () => {
  it("shows what broke instead of a blank window", () => {
    const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <ErrorBoundary>
        <Broken />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("settings.toolPaths is undefined");
    expect(screen.getByRole("button", { name: "Reload" })).toBeInTheDocument();
    quiet.mockRestore();
  });
});
