import { Component, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { S } from "./strings";
import "@fontsource-variable/inter";
import "./styles.css";

/** A render error shows what broke instead of a blank window. Stop and the hotkey keep working (they live in Rust). */
class Guard extends Component<{ children: ReactNode }, { err: string }> {
  state = { err: "" };
  static getDerivedStateFromError(e: unknown) { return { err: e instanceof Error ? `${e.message}\n${e.stack ?? ""}` : String(e) }; }
  render() {
    if (!this.state.err) return this.props.children;
    return (
      <div className="crash">
        <h2>{S.crash.title}</h2>
        <p>{S.crash.body}</p>
        <button className="btn primary" onClick={() => location.reload()}>{S.crash.reload}</button>
        <pre>{this.state.err}</pre>
      </div>
    );
  }
}

createRoot(document.getElementById("root")!).render(<Guard><App /></Guard>);
