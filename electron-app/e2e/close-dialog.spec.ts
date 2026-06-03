import { test, expect } from "./fixtures";

test.describe("Close Confirmation Dialog", () => {
  test("shows confirmation when closing running tab", async ({ mockedPage: page }) => {
    await page.route("**/box/list", (route) => {
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

    // Click close button on tab
    await page.locator(".tab-close").click();

    // Confirmation dialog should appear
    await expect(page.locator(".dialog-title")).toHaveText("Close Terminal");
    await expect(page.locator(".dialog-message")).toContainText("still running");
  });

  test("cancel dismisses dialog without closing", async ({ mockedPage: page }) => {
    await page.route("**/box/list", (route) => {
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

    await page.locator(".tab-close").click();
    await expect(page.locator(".dialog-title")).toHaveText("Close Terminal");

    // Click Cancel
    await page.getByRole("button", { name: "Cancel" }).click();

    // Dialog should be gone, tab should still be there
    await expect(page.locator(".dialog-title")).not.toBeVisible();
    await expect(page.locator(".tab-item")).toHaveCount(1);
  });

  test("close button calls POST /box/{id}/close (not DELETE)", async ({ mockedPage: page }) => {
    const closeRequests: { method: string; url: string }[] = [];

    await page.route("**/box/list", (route) => {
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

    // Track ALL requests to sandbox/sb-1* endpoints
    await page.route("**/box/sb-1**", (route) => {
      const method = route.request().method();
      const url = route.request().url();
      closeRequests.push({ method, url });

      if (method === "POST" && url.includes("/close")) {
        route.fulfill({ status: 200, body: JSON.stringify({ closed: "sb-1" }) });
      } else {
        route.continue();
      }
    });

    await page.goto("/");
    await expect(page.locator(".tab-item")).toHaveCount(1, { timeout: 10000 });

    await page.locator(".tab-close").click();
    await expect(page.locator(".dialog-title")).toHaveText("Close Terminal");
    await page.getByRole("button", { name: "Close" }).click();

    // Wait for the close request to be made
    await page.waitForTimeout(1000);

    // Verify POST /box/sb-1/close was called — NOT DELETE /box/sb-1
    const postClose = closeRequests.find(r => r.method === "POST" && r.url.includes("/close"));
    const deleteWrong = closeRequests.find(r => r.method === "DELETE");
    expect(postClose, "Expected POST /box/{id}/close to be called").toBeTruthy();
    expect(deleteWrong, "Should NOT use DELETE method").toBeFalsy();
  });

  test("close button actually removes tab via polling", async ({ mockedPage: page }) => {
    let closed = false;

    await page.route("**/box/list", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: closed
          ? JSON.stringify([])
          : JSON.stringify([{
              id: "sb-1",
              kind: { type: "cli", detail: { command: "zsh", args: [] } },
              status: { type: "Running" },
              pty_pid: 100,
              port: 15801,
            }]),
      });
    });

    // Mock the correct close endpoint: POST /box/{id}/close
    await page.route("**/box/sb-1/close", (route) => {
      if (route.request().method() === "POST") {
        closed = true;
        route.fulfill({ status: 200, body: JSON.stringify({ closed: "sb-1" }) });
      } else {
        route.continue();
      }
    });

    await page.goto("/");
    await expect(page.locator(".tab-item")).toHaveCount(1, { timeout: 10000 });

    await page.locator(".tab-close").click();
    await expect(page.locator(".dialog-title")).toHaveText("Close Terminal");
    await page.getByRole("button", { name: "Close" }).click();

    // After close succeeds, next poll returns empty → tab removed
    await expect(page.locator(".empty-state-text")).toHaveText("No sandbox open", { timeout: 10000 });
  });

  test("Close All Terminals calls POST /box/{id}/close for each tab", async ({ mockedPage: page }) => {
    const closeRequests: { method: string; url: string }[] = [];

    await page.route("**/box/list", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([
          { id: "sb-1", kind: { type: "cli", detail: { command: "zsh", args: [] } }, status: { type: "Running" }, pty_pid: 100, port: 15801 },
          { id: "sb-2", kind: { type: "cli", detail: { command: "claude", args: [] } }, status: { type: "Running" }, pty_pid: 101, port: 15801 },
        ]),
      });
    });

    // Track ALL requests to sandbox endpoints
    await page.route("**/box/sb-**", (route) => {
      const method = route.request().method();
      const url = route.request().url();
      closeRequests.push({ method, url });

      if (method === "POST" && url.includes("/close")) {
        route.fulfill({ status: 200, body: JSON.stringify({ closed: "ok" }) });
      } else {
        route.continue();
      }
    });

    await page.goto("/");
    await expect(page.locator(".tab-item")).toHaveCount(2, { timeout: 10000 });

    // Simulate window close button click
    await page.evaluate(() => (window as any).sandbox.triggerWindowClosing());

    // Window close dialog should appear
    await expect(page.locator(".dialog-title")).toHaveText("Close Window");
    await expect(page.locator(".dialog-message")).toContainText("2 terminal");

    // Click "Close All Terminals"
    await page.getByRole("button", { name: "Close All Terminals" }).click();
    await page.waitForTimeout(1000);

    // Verify POST /box/{id}/close was called for BOTH sandboxes
    const postClose1 = closeRequests.find(r => r.method === "POST" && r.url.includes("/sb-1/close"));
    const postClose2 = closeRequests.find(r => r.method === "POST" && r.url.includes("/sb-2/close"));
    const deleteWrong = closeRequests.find(r => r.method === "DELETE");
    expect(postClose1, "Expected POST /box/sb-1/close").toBeTruthy();
    expect(postClose2, "Expected POST /box/sb-2/close").toBeTruthy();
    expect(deleteWrong, "Should NOT use DELETE method").toBeFalsy();
  });
});
