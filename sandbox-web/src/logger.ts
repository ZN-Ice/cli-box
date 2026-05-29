const isDebugEnabled = (): boolean => {
  try {
    // Vite exposes env vars via import.meta.env
    const level = import.meta.env.VITE_LOG_LEVEL;
    if (level) return level.toLowerCase() === "debug";
    // Also check URL param for quick toggle: ?log=debug
    const params = new URLSearchParams(window.location.search);
    return params.get("log") === "debug";
  } catch {
    return false;
  }
};

export function debugLog(...args: unknown[]): void {
  if (isDebugEnabled()) {
    console.log("[DEBUG-FE]", ...args);
  }
}

export function debugWarn(...args: unknown[]): void {
  if (isDebugEnabled()) {
    console.warn("[DEBUG-FE]", ...args);
  }
}

export function debugError(...args: unknown[]): void {
  if (isDebugEnabled()) {
    console.error("[DEBUG-FE]", ...args);
  }
}
