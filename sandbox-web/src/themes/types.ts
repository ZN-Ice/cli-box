/** VS Code-inspired theme system — color tokens prefixed with `--sandbox-` */
export interface SandboxThemeColors {
  bgPrimary: string;
  bgSecondary: string;
  bgTertiary: string;
  fgPrimary: string;
  fgSecondary: string;
  fgTertiary: string;
  border: string;
  accent: string;
  scrollbarBg: string;
  scrollbarFg: string;
  success: string;
  error: string;
  titlebarBg: string;
  titlebarFg: string;
}

/** xterm.js terminal theme (subset of ITerminalOptions['theme']) */
export interface TerminalTheme {
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  selectionForeground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

export interface SandboxTheme {
  id: string;
  name: string;
  kind: "dark" | "light";
  colors: SandboxThemeColors;
  terminal: TerminalTheme;
}
