import ReactDOM from "react-dom/client";

function App() {
  return (
    <div className="flex h-screen items-center justify-center">
      <p>System Test Sandbox — Electron</p>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<App />);
