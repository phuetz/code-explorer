import { Component, type ErrorInfo, type ReactNode } from 'react';
import { AlertTriangle, Home, RefreshCw } from 'lucide-react';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Code Explorer UI crashed', error, info);
  }

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <main className="flex min-h-full items-center justify-center bg-neutral-950 px-6 py-10 text-neutral-100">
        <section className="w-full max-w-xl rounded-lg border border-red-900/60 bg-red-950/20 p-5 shadow-2xl">
          <div className="mb-4 flex items-center gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md border border-red-800 bg-red-950 text-red-300">
              <AlertTriangle className="h-5 w-5" aria-hidden />
            </div>
            <div className="min-w-0">
              <h1 className="text-base font-semibold text-red-100">Interface Code Explorer interrompue</h1>
              <p className="mt-1 text-sm text-red-200/70">
                Le client React a rencontré une erreur, mais le serveur peut rester opérationnel.
              </p>
            </div>
          </div>
          <pre className="max-h-48 overflow-auto rounded-md border border-red-900/50 bg-neutral-950 p-3 text-xs text-red-100/80">
            {this.state.error.message || String(this.state.error)}
          </pre>
          <div className="mt-4 flex flex-wrap justify-end gap-2">
            <button
              type="button"
              onClick={() => {
                window.location.href = '/';
              }}
              className="inline-flex items-center gap-1.5 rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            >
              <Home className="h-4 w-4" aria-hidden />
              Accueil
            </button>
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="inline-flex items-center gap-1.5 rounded-md border border-red-800 bg-red-900/40 px-3 py-1.5 text-sm text-red-100 hover:bg-red-900/60"
            >
              <RefreshCw className="h-4 w-4" aria-hidden />
              Recharger
            </button>
          </div>
        </section>
      </main>
    );
  }
}
