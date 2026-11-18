import type { ReactNode } from "react";

/** Simple record illustration used by every empty state. */
function Illustration() {
  return (
    <svg viewBox="0 0 120 84" width="7.5rem" height="5.25rem" aria-hidden="true" focusable="false">
      <rect x="6" y="6" width="72" height="72" rx="3" fill="var(--surface)" />
      <circle cx="78" cy="42" r="36" fill="var(--bg-panel)" stroke="var(--border-strong)" />
      <circle cx="78" cy="42" r="24" fill="none" stroke="var(--border)" />
      <circle cx="78" cy="42" r="10" fill="var(--accent)" />
      <circle cx="78" cy="42" r="2" fill="var(--bg)" />
    </svg>
  );
}

interface EmptyStateProps {
  title: string;
  message: ReactNode;
  action?: ReactNode;
}

export function EmptyState({ title, message, action }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <Illustration />
      <h2>{title}</h2>
      <p className="muted">{message}</p>
      {action}
    </div>
  );
}
