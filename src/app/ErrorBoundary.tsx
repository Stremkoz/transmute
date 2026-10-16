import { Component, type ErrorInfo, type ReactNode } from "react";
import styles from "./ErrorBoundary.module.css";

interface State {
  error: Error | null;
}

/**
 * Catches a rendering crash anywhere below it. Without this React unmounts
 * everything and the window goes blank; with it, the user sees what broke
 * and can reload. The backend session lives in the shell, so reloading
 * keeps the files and settings.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Transmute's window crashed", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className={styles.crash} role="alert">
        <h1 className={styles.title}>Something went wrong in this window</h1>
        <p className={styles.text}>
          Your files and settings are safe: they're kept outside the window. Reload to carry on. If it keeps
          happening, please report what you were doing, with the message below.
        </p>
        <pre className={styles.detail}>{error.message}</pre>
        <button type="button" className="button button-primary" onClick={() => window.location.reload()}>
          Reload
        </button>
      </div>
    );
  }
}
