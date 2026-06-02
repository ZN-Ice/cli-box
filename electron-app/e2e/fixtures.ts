import { test as base, Page } from "@playwright/test";

export const test = base.extend<{ mockedPage: Page }>({
  mockedPage: async ({ page }, use) => {
    // Mock window.sandbox IPC bridge
    await page.addInitScript(() => {
      (window as any).sandbox = {
        getDaemonPort: () => Promise.resolve(15801),
        createTab: () => Promise.resolve(),
        switchTab: () => Promise.resolve(),
        closeTab: () => Promise.resolve(),
        listTabs: () => Promise.resolve([]),
        onSwitchTab: () => {},
        onWindowClosing: () => {},
        sendCloseResponse: () => Promise.resolve(),
      };
    });

    // Mock daemon HTTP API
    await page.route("**/sandbox/list", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([]),
      });
    });

    await page.route("**/health", (route) => {
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ status: "ok" }),
      });
    });

    await use(page);
  },
});

export { expect } from "@playwright/test";
