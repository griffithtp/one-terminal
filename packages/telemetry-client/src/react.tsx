import { Component, type ErrorInfo, type ReactNode } from "react";
import type { Telemetry } from "./index";

interface Props {
  telemetry: Telemetry;
  /** Rendered instead of `children` after a render error. Default: nothing. */
  fallback?: ReactNode;
  children: ReactNode;
}

interface State {
  failed: boolean;
}

/**
 * Reports render errors in `children` through `telemetry` and shows
 * `fallback` instead of an empty page. Errors in event handlers and async
 * code aren't render errors — `createTelemetry`'s `error` /
 * `unhandledrejection` hooks catch those.
 */
export class TelemetryErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.props.telemetry.error(error, {
      attributes: { "react.component_stack": (info.componentStack ?? "").trim().slice(0, 256) },
    });
  }

  render() {
    return this.state.failed ? (this.props.fallback ?? null) : this.props.children;
  }
}
