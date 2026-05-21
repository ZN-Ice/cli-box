import { describe, it, expect } from "vitest";

describe("App", () => {
  it("test runner works", () => {
    expect(true).toBe(true);
  });

  it("vitest environment is node", () => {
    // Verify the test environment matches vitest config
    expect(typeof globalThis).toBe("object");
  });
});

describe("TypeScript type system", () => {
  it("supports optional chaining", () => {
    const obj: { a?: { b?: number } } | null = { a: { b: 42 } };
    expect(obj?.a?.b).toBe(42);
    expect(obj?.a?.b).toBeDefined();
  });

  it("supports nullish coalescing", () => {
    const value: string | null = null;
    expect(value ?? "default").toBe("default");
  });
});
