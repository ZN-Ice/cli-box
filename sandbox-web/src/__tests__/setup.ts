import "@testing-library/jest-dom/vitest";

// Node.js 25 ships a partial localStorage without clear().
// Provide a complete implementation for test environments.
const store = new Map<string, string>();
const fullLocalStorage = {
  getItem(key: string) {
    return store.get(key) ?? null;
  },
  setItem(key: string, value: string) {
    store.set(key, String(value));
  },
  removeItem(key: string) {
    store.delete(key);
  },
  clear() {
    store.clear();
  },
  get length() {
    return store.size;
  },
  key(index: number) {
    const keys = [...store.keys()];
    return keys[index] ?? null;
  },
};
Object.defineProperty(globalThis, "localStorage", {
  value: fullLocalStorage,
  writable: true,
  configurable: true,
});

// xterm.js requires window.matchMedia
Object.defineProperty(globalThis, "matchMedia", {
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }),
  writable: true,
  configurable: true,
});

// api.ts uses URL.createObjectURL for screenshot blob URLs
if (!globalThis.URL.createObjectURL) {
  globalThis.URL.createObjectURL = (obj: Blob) =>
    `blob:test-${Date.now()}-${obj.size}`;
}
if (!globalThis.URL.revokeObjectURL) {
  globalThis.URL.revokeObjectURL = () => {};
}
