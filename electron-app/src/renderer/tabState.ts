import { SandboxInfo } from "./api";

export interface Tab {
  id: string;
  kind: string;
  title: string;
  sandbox: SandboxInfo;
}

export function createTabFromSandbox(sb: SandboxInfo): Tab {
  return {
    id: sb.id,
    kind: sb.kind?.type || "cli",
    title: sb.kind?.detail?.command || sb.id.slice(0, 8),
    sandbox: sb,
  };
}

export function syncTabs(
  prev: Tab[],
  list: SandboxInfo[]
): { tabs: Tab[]; removedIds: string[] } {
  const next: Tab[] = [];
  for (const sb of list) {
    next.push(createTabFromSandbox(sb));
  }
  const newIds = new Set(next.map(t => t.id));
  const removedIds = prev.filter(t => !newIds.has(t.id)).map(t => t.id);
  return { tabs: next, removedIds };
}

export function selectAfterClose(
  tabs: Tab[],
  closedId: string,
  currentActiveId: string | null
): string | null {
  const remaining = tabs.filter(t => t.id !== closedId);
  if (remaining.length === 0) return null;
  if (currentActiveId !== closedId) return currentActiveId;
  const closedIdx = tabs.findIndex(t => t.id === closedId);
  const newIdx = Math.min(closedIdx, remaining.length - 1);
  return remaining[newIdx].id;
}
