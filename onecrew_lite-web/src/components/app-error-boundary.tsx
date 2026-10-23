import { Component, type ErrorInfo, type ReactNode } from "react";

import { appI18n } from "@/i18n";
import zhCNShell from "@/i18n/locales/zh-CN/shell";

type AppErrorBoundaryProps = {
  children: ReactNode;
};

type AppErrorBoundaryState = {
  failed: boolean;
  message: string;
};

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  const message = String(error);
  return message === "null" || message === "undefined" ? appErrorCopy("unknown") : message;
}

type AppErrorCopyKey = keyof typeof zhCNShell.appError;

function appErrorCopy(key: AppErrorCopyKey): string {
  return appI18n.isInitialized
    ? appI18n.t(`appError.${key}`, { ns: "shell" })
    : zhCNShell.appError[key];
}

export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = { failed: false, message: "" };

  static getDerivedStateFromError(error: unknown): AppErrorBoundaryState {
    return { failed: true, message: errorMessage(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("Unhandled desktop render error", error, info.componentStack);
  }

  render() {
    if (!this.state.failed) {
      return this.props.children;
    }

    return (
      <main className="grid min-h-screen place-items-center bg-background p-6 text-foreground">
        <section className="w-full max-w-xl rounded-xl border bg-card p-6 shadow-sm">
          <p className="text-sm font-medium text-destructive">{appErrorCopy("eyebrow")}</p>
          <h1 className="mt-2 text-xl font-semibold">{appErrorCopy("title")}</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            {appErrorCopy("description")}
          </p>
          <pre className="mt-4 max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-muted p-3 text-xs">
            {this.state.message}
          </pre>
          <button
            type="button"
            className="mt-4 inline-flex h-9 items-center justify-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground"
            onClick={() => window.location.reload()}
          >
            {appErrorCopy("reload")}
          </button>
        </section>
      </main>
    );
  }
}
