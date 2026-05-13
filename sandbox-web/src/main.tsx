import React from "react";
import ReactDOM from "react-dom/client";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <div className="flex h-screen bg-gray-900 text-white">
      <div className="flex-1 flex flex-col">
        <header className="h-10 bg-gray-800 flex items-center px-4 text-sm">
          System Test Sandbox
        </header>
        <main className="flex-1 flex items-center justify-center text-gray-500">
          Sandbox terminal and app area — coming soon
        </main>
      </div>
    </div>
  </React.StrictMode>,
);
