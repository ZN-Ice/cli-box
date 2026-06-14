import React from "react";

export function DaemonWaiting() {
  return (
    <div className="daemon-waiting">
      <div className="daemon-waiting-content">
        <h2>Waiting for cli-box-daemon...</h2>
        <p>To start the daemon, run in a terminal:</p>
        <code>cli-box start</code>
        <p className="daemon-waiting-hint">
          This window will connect automatically once the daemon is running.
        </p>
      </div>
    </div>
  );
}