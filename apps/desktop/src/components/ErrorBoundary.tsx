import { Component, type ErrorInfo, type ReactNode } from "react";
import { S } from "../strings";
import { ErrorNotice } from "./ErrorNotice";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * Keeps a render error in one screen from blanking the window: the screen is
 * replaced by the error and a Reload button, and the navigation stays usable.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  override componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("A screen failed to render", error, info.componentStack);
  }

  override render() {
    const { error } = this.state;
    if (!error) {
      return this.props.children;
    }
    return (
      <div className="stack">
        <ErrorNotice
          error={{
            code: "UI_RENDER",
            message: error.message || S.errors.unexpected,
            fix: S.errors.renderFix,
          }}
        />
        <div>
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => {
              window.location.reload();
            }}
          >
            {S.errors.reload}
          </button>
        </div>
      </div>
    );
  }
}
