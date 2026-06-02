import { test, expect } from "./fixtures";

test.describe("Tab Screenshot Capture", () => {
  test("terminal area has dark theme, not white background", async ({ mockedPage: page }) => {
    await page.emulateMedia({ colorScheme: "dark" });

    await page.route("**/sandbox/list", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([{
          id: "sb-1",
          kind: { type: "cli", detail: { command: "zsh", args: [] } },
          status: { type: "Running" },
          pty_pid: 100,
          port: 15801,
        }]),
      });
    });

    await page.goto("/");
    await expect(page.locator(".tab-item")).toHaveCount(1, { timeout: 10000 });

    // Wait for terminal to render
    await page.waitForSelector(".terminal-container", { timeout: 5000 });

    // Verify terminal container has dark background (not white)
    const termContainer = page.locator(".terminal-container");
    const bgColor = await termContainer.evaluate((el) => {
      return window.getComputedStyle(el).backgroundColor;
    });

    // Should be dark, NOT white
    expect(bgColor).not.toBe("rgb(255, 255, 255)");
    expect(bgColor).not.toBe("rgba(255, 255, 255, 1)");
  });

  test("entire page has dark theme", async ({ mockedPage: page }, testInfo) => {
    // Skip screenshot comparison on CI — cross-platform rendering differs too much
    testInfo.skip(!!process.env.CI, "Visual regression snapshots are platform-specific");
    await page.emulateMedia({ colorScheme: "dark" });

    await page.route("**/sandbox/list", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([{
          id: "sb-1",
          kind: { type: "cli", detail: { command: "zsh", args: [] } },
          status: { type: "Running" },
          pty_pid: 100,
          port: 15801,
        }]),
      });
    });

    await page.goto("/");
    await expect(page.locator(".tab-item")).toHaveCount(1, { timeout: 10000 });

    // Screenshot the entire page for visual regression
    await expect(page).toHaveScreenshot("terminal-dark-theme.png", {
      mask: [page.locator(".statusbar-dot")], // mask dynamic elements
      maxDiffPixels: 500,
    });
  });
});
