import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { t } from "../i18n";
import { formatDuration, type Track } from "../lib/library";
import { useLibrary } from "../state/LibraryContext";
import { useUi, type TrackContext } from "../state/UiContext";
import { Artwork } from "./Artwork";
import { Icon } from "./Icon";
import { FavoriteButton } from "./TrackTable";

interface QueueRowProps {
  track: Track;
  current?: boolean;
  /** Position in Next up; absent for the playing track. */
  offset?: number;
  count: number;
  dropTarget: boolean;
  onDragStart: (offset: number) => void;
  onDragOver: (offset: number) => void;
  onDrop: (offset: number) => void;
  onDragEnd: () => void;
}

function QueueRow(props: QueueRowProps) {
  const { track, current, offset, count } = props;
  const { artworkUrl, playQueued, removeFromQueue, moveInQueue, toggle } = useLibrary();
  const { showDetails, openTrackMenu } = useUi();
  const queued = offset !== undefined;
  const context: TrackContext = {
    list: [track],
    index: 0,
    entryIndex: offset,
    entryCount: queued ? count : undefined,
    onMove: queued ? moveInQueue : undefined,
    onRemoveFromQueue: queued ? () => removeFromQueue(offset) : undefined,
  };
  const play = () => (queued ? playQueued(offset) : toggle());
  const onKeyDown = (event: KeyboardEvent<HTMLLIElement>) => {
    if (event.target !== event.currentTarget) return;
    const box = event.currentTarget.getBoundingClientRect();
    const sibling = (step: 1 | -1) => {
      const rows = [...document.querySelectorAll<HTMLElement>(".queue-row")];
      rows[rows.indexOf(event.currentTarget) + step]?.focus();
    };
    switch (event.key) {
      case "ArrowDown":
        sibling(1);
        break;
      case "ArrowUp":
        sibling(-1);
        break;
      case "Enter":
        showDetails(track, context);
        break;
      case " ":
        play();
        break;
      case "ContextMenu":
        openTrackMenu(track, context, box.left + 40, box.bottom - 8);
        break;
      case "F10":
        if (!event.shiftKey) return;
        openTrackMenu(track, context, box.left + 40, box.bottom - 8);
        break;
      case "Delete":
        if (!queued) return;
        removeFromQueue(offset);
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <li
      className="queue-row"
      tabIndex={0}
      aria-current={current ? "true" : undefined}
      data-unavailable={!track.available}
      data-drop={props.dropTarget}
      draggable={queued}
      onKeyDown={onKeyDown}
      onClick={() => showDetails(track, context)}
      onDoubleClick={play}
      onContextMenu={(event) => {
        event.preventDefault();
        openTrackMenu(track, context, event.clientX, event.clientY);
      }}
      onDragStart={(event) => {
        if (!queued) return;
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", String(offset));
        props.onDragStart(offset);
      }}
      onDragOver={(event) => {
        if (!queued) return;
        event.preventDefault();
        props.onDragOver(offset);
      }}
      onDrop={(event) => {
        if (!queued) return;
        event.preventDefault();
        props.onDrop(offset);
      }}
      onDragEnd={props.onDragEnd}
    >
      <Artwork src={artworkUrl(track.artwork, true)} size={2.25} />
      <span className="track-text">
        <span className="track-title truncate">{track.title}</span>
        <span className="track-artist truncate">{track.artist}</span>
      </span>
      <FavoriteButton track={track} />
      {queued && (
        <button
          className="fav"
          tabIndex={-1}
          aria-label={t("queue.remove", { title: track.title })}
          onClick={(event) => {
            event.stopPropagation();
            removeFromQueue(offset);
          }}
          onDoubleClick={(event) => event.stopPropagation()}
        >
          <Icon name="x" size={1} />
        </button>
      )}
      <span className="queue-duration num">{formatDuration(track.durationMs)}</span>
    </li>
  );
}

interface QueuePanelProps {
  onClose: () => void;
  /** True when the panel floats over the content (narrow windows). */
  overlay: boolean;
}

export function QueuePanel({ onClose, overlay }: QueuePanelProps) {
  const { playback, clearQueue, moveInQueue } = useLibrary();
  const { saveQueueAsPlaylist } = useUi();
  const { track, nextUp, nextUpTotal } = playback;
  const empty = !track && nextUp.length === 0;
  const [drag, setDrag] = useState<{ from: number; over: number | null } | null>(null);
  const panel = useRef<HTMLElement>(null);

  // As an overlay it behaves like a dismissible panel: focus moves into it and
  // Escape closes it. Docked beside the content, it is part of the page.
  useEffect(() => {
    if (!overlay) return;
    panel.current?.querySelector<HTMLElement>("button")?.focus();
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape" && !document.querySelector("dialog[open], .menu")) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [overlay, onClose]);

  const rowProps = (offset: number | undefined) => ({
    count: nextUp.length,
    dropTarget: drag !== null && offset !== undefined && drag.over === offset && drag.from !== offset,
    onDragStart: (from: number) => setDrag({ from, over: null }),
    onDragOver: (over: number) => setDrag((current) => (current ? { ...current, over } : current)),
    onDrop: (to: number) => {
      if (drag && drag.from !== to) moveInQueue(drag.from, to);
      setDrag(null);
    },
    onDragEnd: () => setDrag(null),
  });

  return (
    <>
      {overlay && <div className="queue-scrim" onClick={onClose} aria-hidden="true" />}
      <aside className="queue" aria-label={t("queue.title")} ref={panel} data-overlay={overlay}>
        <div className="queue-header">
          <h2>{t("queue.title")}</h2>
          <div className="queue-header-actions">
            {(track || nextUp.length > 0) && (
              <button
                className="icon-btn"
                aria-label={t("queue.save")}
                title={t("queue.save")}
                onClick={saveQueueAsPlaylist}
              >
                <Icon name="list-plus" size={1.25} />
              </button>
            )}
            {nextUp.length > 0 && (
              <button className="btn btn-ghost" onClick={clearQueue}>
                {t("queue.clear")}
              </button>
            )}
            <button className="icon-btn" aria-label={t("queue.close")} onClick={onClose}>
              <Icon name="x" size={1.25} />
            </button>
          </div>
        </div>

        {empty ? (
          <div className="queue-empty">
            <h3>{t("queue.emptyTitle")}</h3>
            <p className="muted">{t("queue.emptyText")}</p>
          </div>
        ) : (
          <div className="queue-body scroll">
            {track && (
              <section aria-labelledby="queue-now">
                <h3 id="queue-now" className="queue-section">
                  {t("queue.nowPlaying")}
                </h3>
                <ul className="queue-list">
                  <QueueRow track={track} current {...rowProps(undefined)} />
                </ul>
              </section>
            )}
            {nextUp.length > 0 && (
              <section aria-labelledby="queue-next">
                <h3 id="queue-next" className="queue-section">
                  {t("queue.nextUp")}
                </h3>
                <ul className="queue-list">
                  {nextUp.map((item, offset) => (
                    <QueueRow key={`${item.id}:${offset}`} track={item} offset={offset} {...rowProps(offset)} />
                  ))}
                </ul>
                {nextUpTotal > nextUp.length && (
                  <p className="queue-more">{t("queue.more", { count: nextUpTotal - nextUp.length })}</p>
                )}
              </section>
            )}
          </div>
        )}
      </aside>
    </>
  );
}
