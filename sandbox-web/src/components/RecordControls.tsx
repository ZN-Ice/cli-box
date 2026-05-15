import { useState } from "react";

type RecordStatus = "idle" | "recording" | "playing";

interface RecordControlsProps {
  onRecordStart: () => void;
  onRecordStop: () => void;
  onPlay: (speed: number) => void;
  onPlayStop: () => void;
  status: RecordStatus;
  actionCount?: number;
}

export default function RecordControls({
  onRecordStart,
  onRecordStop,
  onPlay,
  onPlayStop,
  status,
  actionCount = 0,
}: RecordControlsProps) {
  const [speed, setSpeed] = useState(1.0);

  return (
    <div className="flex flex-col h-full bg-gray-900 border-t border-gray-700">
      <div className="px-3 py-2 bg-gray-800 border-b border-gray-700 text-xs font-medium text-gray-300">
        Recording & Playback
      </div>

      <div className="flex items-center gap-2 px-3 py-2">
        {/* Record */}
        {status === "recording" ? (
          <button
            onClick={onRecordStop}
            className="flex items-center gap-1 px-3 py-1.5 bg-red-700 hover:bg-red-600 rounded text-white text-xs font-medium transition-colors"
          >
            <span className="inline-block w-2 h-2 bg-white rounded-sm animate-pulse" />
            Stop
          </button>
        ) : (
          <button
            onClick={onRecordStart}
            disabled={status === "playing"}
            className="flex items-center gap-1 px-3 py-1.5 bg-red-800 hover:bg-red-700 disabled:bg-gray-700 disabled:text-gray-500 rounded text-white text-xs font-medium transition-colors"
          >
            ● Record
          </button>
        )}

        {/* Play */}
        {status === "playing" ? (
          <button
            onClick={onPlayStop}
            className="flex items-center gap-1 px-3 py-1.5 bg-yellow-700 hover:bg-yellow-600 rounded text-white text-xs font-medium transition-colors"
          >
            ■ Stop
          </button>
        ) : (
          <button
            onClick={() => onPlay(speed)}
            disabled={status === "recording" || actionCount === 0}
            className="flex items-center gap-1 px-3 py-1.5 bg-green-700 hover:bg-green-600 disabled:bg-gray-700 disabled:text-gray-500 rounded text-white text-xs font-medium transition-colors"
          >
            ▶ Play
          </button>
        )}

        {/* Speed */}
        <select
          value={speed}
          onChange={(e) => setSpeed(Number(e.target.value))}
          className="px-2 py-1.5 bg-gray-800 border border-gray-600 rounded text-xs text-gray-300 focus:outline-none focus:border-blue-500"
        >
          <option value="0.25">0.25x</option>
          <option value="0.5">0.5x</option>
          <option value="1">1x</option>
          <option value="2">2x</option>
          <option value="4">4x</option>
        </select>
      </div>

      {/* Status */}
      <div className="px-3 py-1 text-xs text-gray-500 flex items-center justify-between">
        <span>
          Status:{" "}
          <span
            className={
              status === "recording"
                ? "text-red-400"
                : status === "playing"
                  ? "text-green-400"
                  : "text-gray-400"
            }
          >
            {status}
          </span>
        </span>
        <span>Actions: {actionCount}</span>
      </div>

      {/* Timeline scrubber placeholder */}
      {actionCount > 0 && (
        <div className="px-3 py-2">
          <div className="relative h-1 bg-gray-700 rounded-full">
            <div
              className="absolute h-1 bg-blue-500 rounded-full transition-all"
              style={{
                width: `${status === "idle" ? 0 : status === "recording" ? 100 : 50}%`,
              }}
            />
          </div>
          <div className="flex justify-between text-[10px] text-gray-600 mt-0.5">
            <span>0</span>
            <span>{actionCount} actions</span>
          </div>
        </div>
      )}
    </div>
  );
}
