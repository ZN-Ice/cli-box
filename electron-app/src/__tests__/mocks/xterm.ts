export class MockCell {
  constructor(private chars: string = " ", private fg: number = 0) {}
  getChars() { return this.chars; }
  getFgColor() { return this.fg; }
}

export class MockBufferLine {
  cells: MockCell[];
  length: number;

  constructor(text: string, fg?: number) {
    this.cells = [...text].map(ch => new MockCell(ch, fg ?? 0));
    this.length = this.cells.length;
  }

  getCell(x: number) { return this.cells[x] ?? null; }
}

export class MockBuffer {
  private lines: MockBufferLine[];
  constructor(lines: MockBufferLine[]) { this.lines = lines; }
  getLine(y: number) { return this.lines[y] ?? null; }
}

export class MockTerminal {
  cols = 80;
  rows = 24;
  buffer = { active: new MockBuffer([]) };
  element: HTMLElement | null = null;

  constructor(lines?: MockBufferLine[]) {
    if (lines) {
      this.buffer = { active: new MockBuffer(lines) };
    }
  }
}
