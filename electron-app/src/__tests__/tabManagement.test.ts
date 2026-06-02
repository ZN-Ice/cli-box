import { describe, it, expect } from "vitest";
import { createTabFromSandbox, syncTabs, selectAfterClose, Tab } from "../renderer/tabState";
import { SandboxInfo } from "../renderer/api";

function makeSandbox(id: string, command: string, status = "Running"): SandboxInfo {
  return {
    id,
    kind: { type: "cli", detail: { command, args: [] } },
    status: { type: status },
    pty_pid: 100,
    port: 15801,
  };
}

describe("createTabFromSandbox", () => {
  it("uses command as title", () => {
    const tab = createTabFromSandbox(makeSandbox("abc", "zsh"));
    expect(tab.title).toBe("zsh");
    expect(tab.kind).toBe("cli");
  });

  it("falls back to id prefix when no command", () => {
    const sb = makeSandbox("abcdefgh", "");
    sb.kind.detail.command = "";
    const tab = createTabFromSandbox(sb);
    expect(tab.title).toBe("abcdefgh");
  });
});

describe("syncTabs", () => {
  it("adds new sandboxes", () => {
    const { tabs } = syncTabs([], [makeSandbox("a", "zsh"), makeSandbox("b", "claude")]);
    expect(tabs).toHaveLength(2);
    expect(tabs.map(t => t.id)).toEqual(["a", "b"]);
  });

  it("removes disappeared sandboxes", () => {
    const prev: Tab[] = [
      { id: "a", kind: "cli", title: "zsh", sandbox: makeSandbox("a", "zsh") },
      { id: "b", kind: "cli", title: "claude", sandbox: makeSandbox("b", "claude") },
    ];
    const { tabs, removedIds } = syncTabs(prev, [makeSandbox("a", "zsh")]);
    expect(tabs).toHaveLength(1);
    expect(removedIds).toEqual(["b"]);
  });

  it("handles empty list", () => {
    const prev: Tab[] = [
      { id: "a", kind: "cli", title: "zsh", sandbox: makeSandbox("a", "zsh") },
    ];
    const { tabs, removedIds } = syncTabs(prev, []);
    expect(tabs).toHaveLength(0);
    expect(removedIds).toEqual(["a"]);
  });
});

describe("selectAfterClose", () => {
  const tabs: Tab[] = [
    { id: "a", kind: "cli", title: "zsh", sandbox: makeSandbox("a", "zsh") },
    { id: "b", kind: "cli", title: "claude", sandbox: makeSandbox("b", "claude") },
    { id: "c", kind: "cli", title: "node", sandbox: makeSandbox("c", "node") },
  ];

  it("selects next tab when closing active tab", () => {
    expect(selectAfterClose(tabs, "a", "a")).toBe("b");
  });

  it("selects previous tab when closing last tab", () => {
    expect(selectAfterClose(tabs, "c", "c")).toBe("b");
  });

  it("keeps current active when closing non-active tab", () => {
    expect(selectAfterClose(tabs, "a", "b")).toBe("b");
  });

  it("returns null when closing last remaining tab", () => {
    expect(selectAfterClose([tabs[0]], "a", "a")).toBeNull();
  });
});
