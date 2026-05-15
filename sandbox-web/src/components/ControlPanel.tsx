import { useState } from "react";

interface ControlPanelProps {
  onScreenshot: () => void;
  onSpawnApp: (path: string) => void;
  onSpawnCli: (command: string, args: string[]) => void;
  onClick: (x: number, y: number, button: string) => void;
  onTypeText: (text: string) => void;
  onPressKey: (key: string, modifiers: string[]) => void;
  screenshotLoading?: boolean;
}

export default function ControlPanel({
  onScreenshot,
  onSpawnApp,
  onSpawnCli,
  onClick,
  onTypeText,
  onPressKey,
  screenshotLoading = false,
}: ControlPanelProps) {
  const [appPath, setAppPath] = useState("");
  const [cliCommand, setCliCommand] = useState("");
  const [cliArgs, setCliArgs] = useState("");
  const [clickX, setClickX] = useState("100");
  const [clickY, setClickY] = useState("100");
  const [typeText, setTypeText] = useState("");
  const [keyName, setKeyName] = useState("Return");
  const [modifiers, setModifiers] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);

  const toggle = (section: string) => {
    setExpanded(expanded === section ? null : section);
  };

  return (
    <div className="flex flex-col h-full bg-gray-900 border-l border-gray-700">
      <div className="px-3 py-2 bg-gray-800 border-b border-gray-700 text-xs font-medium text-gray-300">
        Control Panel
      </div>

      <div className="flex-1 overflow-y-auto text-xs">
        {/* Screenshot */}
        <Section
          title="Screenshot"
          expanded={expanded === "screenshot"}
          onToggle={() => toggle("screenshot")}
        >
          <button
            onClick={onScreenshot}
            disabled={screenshotLoading}
            className="w-full px-3 py-1.5 bg-blue-600 hover:bg-blue-500 disabled:bg-blue-800 disabled:cursor-wait rounded text-white font-medium transition-colors"
          >
            {screenshotLoading ? "Capturing..." : "Capture Screenshot"}
          </button>
        </Section>

        {/* App Spawn */}
        <Section
          title="Launch App"
          expanded={expanded === "spawnApp"}
          onToggle={() => toggle("spawnApp")}
        >
          <input
            type="text"
            value={appPath}
            onChange={(e) => setAppPath(e.target.value)}
            placeholder="/Applications/Example.app"
            className="w-full px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-2"
          />
          <button
            onClick={() => appPath && onSpawnApp(appPath)}
            className="w-full px-3 py-1 bg-green-700 hover:bg-green-600 rounded text-white transition-colors"
          >
            Launch App
          </button>
        </Section>

        {/* CLI Spawn */}
        <Section
          title="Spawn CLI"
          expanded={expanded === "spawnCli"}
          onToggle={() => toggle("spawnCli")}
        >
          <input
            type="text"
            value={cliCommand}
            onChange={(e) => setCliCommand(e.target.value)}
            placeholder="Command (e.g., echo)"
            className="w-full px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-2"
          />
          <input
            type="text"
            value={cliArgs}
            onChange={(e) => setCliArgs(e.target.value)}
            placeholder="Args (space separated)"
            className="w-full px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-2"
          />
          <button
            onClick={() =>
              cliCommand &&
              onSpawnCli(cliCommand, cliArgs.split(" ").filter(Boolean))
            }
            className="w-full px-3 py-1 bg-green-700 hover:bg-green-600 rounded text-white transition-colors"
          >
            Spawn CLI
          </button>
        </Section>

        {/* Click */}
        <Section
          title="Mouse Click"
          expanded={expanded === "click"}
          onToggle={() => toggle("click")}
        >
          <div className="flex gap-2 mb-2">
            <input
              type="number"
              value={clickX}
              onChange={(e) => setClickX(e.target.value)}
              placeholder="X"
              className="w-1/2 px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500"
            />
            <input
              type="number"
              value={clickY}
              onChange={(e) => setClickY(e.target.value)}
              placeholder="Y"
              className="w-1/2 px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500"
            />
          </div>
          <button
            onClick={() => onClick(Number(clickX), Number(clickY), "left")}
            className="w-full px-3 py-1 bg-indigo-700 hover:bg-indigo-600 rounded text-white transition-colors"
          >
            Click Left
          </button>
        </Section>

        {/* Type Text */}
        <Section
          title="Type Text"
          expanded={expanded === "typeText"}
          onToggle={() => toggle("typeText")}
        >
          <input
            type="text"
            value={typeText}
            onChange={(e) => setTypeText(e.target.value)}
            placeholder="Text to type..."
            className="w-full px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-2"
          />
          <button
            onClick={() => typeText && onTypeText(typeText)}
            className="w-full px-3 py-1 bg-indigo-700 hover:bg-indigo-600 rounded text-white transition-colors"
          >
            Type
          </button>
        </Section>

        {/* Key Press */}
        <Section
          title="Key Press"
          expanded={expanded === "keyPress"}
          onToggle={() => toggle("keyPress")}
        >
          <input
            type="text"
            value={keyName}
            onChange={(e) => setKeyName(e.target.value)}
            placeholder="Key name (Return, Tab, Space)"
            className="w-full px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-2"
          />
          <input
            type="text"
            value={modifiers}
            onChange={(e) => setModifiers(e.target.value)}
            placeholder="Modifiers (cmd, shift, alt)"
            className="w-full px-2 py-1 bg-gray-800 border border-gray-600 rounded text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 mb-2"
          />
          <button
            onClick={() =>
              onPressKey(
                keyName,
                modifiers
                  .split(",")
                  .map((m) => m.trim())
                  .filter(Boolean),
              )
            }
            className="w-full px-3 py-1 bg-indigo-700 hover:bg-indigo-600 rounded text-white transition-colors"
          >
            Press Key
          </button>
        </Section>
      </div>
    </div>
  );
}

function Section({
  title,
  expanded,
  onToggle,
  children,
}: {
  title: string;
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-gray-700/50">
      <button
        onClick={onToggle}
        className="w-full flex items-center justify-between px-3 py-2 text-gray-400 hover:text-gray-200 hover:bg-gray-800/50 transition-colors text-left"
      >
        <span className="font-medium">{title}</span>
        <span
          className={`transform transition-transform ${expanded ? "rotate-90" : ""}`}
        >
          ▶
        </span>
      </button>
      {expanded && <div className="px-3 pb-2">{children}</div>}
    </div>
  );
}
