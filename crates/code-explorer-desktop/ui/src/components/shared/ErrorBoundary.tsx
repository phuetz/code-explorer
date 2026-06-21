import React, { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  resetKey: number;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null, resetKey: 0 };
  }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error("[Code Explorer ErrorBoundary]", {
      error: error.message,
      stack: error.stack,
      componentStack: errorInfo.componentStack,
    });
  }

  render() {
    if (this.state.hasError) {
      return (
        this.props.fallback ?? (
          <div className="h-full flex flex-col items-center justify-center gap-3 p-6 text-center">
            <p className="text-[var(--danger)] font-medium">Something went wrong</p>
            <p className="text-xs text-[var(--text-muted)] max-w-md">
              {this.state.error?.message}
            </p>
            <button
              onClick={() => this.setState((s) => ({ hasError: false, error: null, resetKey: s.resetKey + 1 }))}
              className="px-3 py-1.5 rounded bg-[var(--accent)] text-white text-xs hover:opacity-90 transition-opacity"
            >
              Retry
            </button>
          </div>
        )
      );
    }

    return <React.Fragment key={this.state.resetKey}>{this.props.children}</React.Fragment>;
  }
}
