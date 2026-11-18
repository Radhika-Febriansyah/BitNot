import { useEffect, useRef } from "react";
import { useApp } from "../state/AppContext";
import { Icon } from "./Icon";

interface TopBarProps {
  query: string;
  onQuery: (query: string) => void;
  /** Called when the user confirms a search with Enter. */
  onSubmit: () => void;
}

export function TopBar({ query, onQuery, onSubmit }: TopBarProps) {
  const { back, forward, canBack, canForward, go, route, activeProfile } = useApp();
  const initial = activeProfile.name.trim().charAt(0).toUpperCase() || "?";
  const input = useRef<HTMLInputElement>(null);

  // Opening Search from the sidebar puts the cursor in the field.
  useEffect(() => {
    if (route.page === "search") input.current?.focus();
  }, [route.page]);

  return (
    <div className="topbar">
      <button className="icon-btn solid" aria-label="Back" disabled={!canBack} onClick={back}>
        <Icon name="chevron-left" size={1.25} />
      </button>
      <button
        className="icon-btn solid"
        aria-label="Forward"
        disabled={!canForward}
        onClick={forward}
      >
        <Icon name="chevron-right" size={1.25} />
      </button>

      <label className="field top-search">
        <Icon name="search" size={1.125} />
        <input
          ref={input}
          type="search"
          placeholder="Search your music"
          aria-label="Search your music"
          value={query}
          onChange={(event) => {
            onQuery(event.target.value);
            if (route.page !== "search") go({ page: "search" });
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") onSubmit();
          }}
        />
        {query && (
          <button className="field-clear" aria-label="Clear search" onClick={() => onQuery("")}>
            <Icon name="x" size={1} />
          </button>
        )}
      </label>

      <div className="spacer" />

      <button
        className="icon-btn"
        aria-label="Settings"
        aria-current={route.page === "settings" ? "page" : undefined}
        onClick={() => go({ page: "settings" })}
      >
        <Icon name="settings" size={1.25} />
      </button>
      <button
        className="avatar"
        aria-label={`Profile: ${activeProfile.name}. Open profile settings`}
        title={activeProfile.name}
        onClick={() => go({ page: "settings" })}
      >
        {initial}
      </button>
    </div>
  );
}
