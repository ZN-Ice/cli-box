import { test, expect } from "./fixtures";

test.describe("Tab Creation", () => {
  test("shows empty state when no sandboxes", async ({ mockedPage: page }) => {
    await page.goto("/");
    await expect(page.locator(".empty-state-text")).toHaveText("No sandbox open");
  });

  test("creates tab and shows terminal", async ({ mockedPage: page }) => {
    await page.route("**/sandbox/list", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([{
          id: "test-sb-1",
          kind: { type: "cli", detail: { command: "zsh", args: [] } },
          status: { type: "Running" },
          pty_pid: 100,
          port: 15801,
        }]),
      });
    });

    await page.goto("/");
    await expect(page.locator(".tab-item")).toHaveCount(1, { timeout: 10000 });
    await expect(page.locator(".tab-item")).toContainText("zsh");
  });

  test("multiple tabs display correctly", async ({ mockedPage: page }) => {
    await page.route("**/sandbox/list", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([
          { id: "sb-1", kind: { type: "cli", detail: { command: "zsh", args: [] } }, status: { type: "Running" }, pty_pid: 100, port: 15801 },
          { id: "sb-2", kind: { type: "cli", detail: { command: "claude", args: [] } }, status: { type: "Running" }, pty_pid: 101, port: 15801 },
          { id: "sb-3", kind: { type: "cli", detail: { command: "opencode", args: [] } }, status: { type: "Running" }, pty_pid: 102, port: 15801 },
        ]),
      });
    });

    await page.goto("/");
    await expect(page.locator(".tab-item")).toHaveCount(3, { timeout: 10000 });

    // Visual regression: screenshot the tab bar
    // Use maxDiffPixels to handle platform-specific font rendering differences
    await expect(page.locator(".tab-bar")).toHaveScreenshot("tab-bar-3-tabs.png", {
      maxDiffPixels: 500,
    });
  });
});
