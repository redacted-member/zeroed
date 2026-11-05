import { Component, type ErrorInfo, type ReactNode } from 'react';

import { strings } from '../../lib/strings';

import './ErrorBoundary.scss';

interface ErrorBoundaryProps {
  children: ReactNode;
  /** Changing this clears a caught error, for instance when a new form opens. */
  resetKey: string;
}

interface ErrorBoundaryState {
  error: Error | null;
  key: string;
}

/** Keeps a failing part of the window from taking the rest down with it. */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null, key: this.props.resetKey };

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error };
  }

  static getDerivedStateFromProps(
    props: ErrorBoundaryProps,
    state: ErrorBoundaryState,
  ): Partial<ErrorBoundaryState> | null {
    return props.resetKey === state.key ? null : { error: null, key: props.resetKey };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('a panel failed to draw', error, info.componentStack);
  }

  override render(): ReactNode {
    if (this.state.error === null) {
      return this.props.children;
    }
    return (
      <div className="error-boundary" role="alert">
        <p className="error-boundary__title">{strings.errorBoundary.title}</p>
        <p className="error-boundary__message">{this.state.error.message}</p>
      </div>
    );
  }
}
