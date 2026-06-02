import { test, expect } from "./fixtures";

test.describe("Close Confirmation Dialog", () => {
  test("shows confirmation when closing running tab", async ({ mockedPage: page }) => {
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

    // Click close button on tab
    await page.locator(".tab-close").click();

    // Confirmation dialog should appear
    await expect(page.locator(".dialog-title")).toHaveText("Close Terminal");
    await expect(page.locator(".dialog-message")).toContainText("still running");
  });

  test("cancel dismisses dialog without closing", async ({ mockedPage: page }) => {
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

    await page.locator(".tab-close").click();
    await expect(page.locator(".dialog-title")).toHaveText("Close Terminal");

    // Click Cancel
    await page.getByRole("button", { name: "Cancel" }).click();

    // Dialog should be gone, tab should still be there
    await expect(page.locator(".dialog-title")).not.toBeVisible();
    await expect(page.locator(".tab-item")).toHaveCount(1);
  });

  test("close button calls POST /sandbox/{id}/close (not DELETE)", async ({ mockedPage: page }) => {
    const closeRequests: { method: string; url: string }[] = [];

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

    // Track ALL requests to sandbox/sb-1* endpoints
    await page.route("**/sandbox/sb-1**", (route) => {
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

    // Verify POST /sandbox/sb-1/close was called — NOT DELETE /sandbox/sb-1
    const postClose = closeRequests.find(r => r.method === "POST" && r.url.includes("/close"));
    const deleteWrong = closeRequests.find(r => r.method === "DELETE");
    expect(postClose, "Expected POST /sandbox/{id}/close to be called").toBeTruthy();
    expect(deleteWrong, "Should NOT use DELETE method").toBeFalsy();
  });

  test("close button actually removes tab via polling", async ({ mockedPage: page }) => {
    let closed = false;

    await page.route("**/sandbox/list", (route) => {
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

    // Mock the correct close endpoint: POST /sandbox/{id}/close
    await page.route("**/sandbox/sb-1/close", (route) => {
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
});
