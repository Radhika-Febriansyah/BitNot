import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { t } from "../i18n";
import {
  formatDate,
  formatDuration,
  qualityText,
  visibleRange,
  type Sort,
  type SortField,
  type Track,
  type TrackRow,
} from "../lib/library";
import { useLibrary } from "../state/LibraryContext";
import { useUi, type TrackContext } from "../state/UiContext";
import { Artwork } from "./Artwork";
import { Icon } from "./Icon";

/** Row height in rem; must match `.track-row` in pages.css. */
const ROW_REM = 3.5;
/** Shorter lists are rendered in full; longer ones only around the viewport. */
export const VIRTUAL_THRESHOLD = 80;
const OVERSCAN = 12;

/** Animated bars marking the track that is playing. */
export function Equalizer({ active }: { active: boolean }) {
  return (
    <span className="eq" data-active={active} aria-hidden="true">
      <i />
      <i />
      <i />
      <i />
    </span>
  );
}

interface FavoriteButtonProps {
  track: Track;
  /** Keep the outline heart visible instead of only on hover. */
  alwaysVisible?: boolean;
}

export function FavoriteButton({ track, alwaysVisible }: FavoriteButtonProps) {
  const { setFavorite, isFavorite } = useLibrary();
  const on = isFavorite(track.id);
  return (
    <button
      className={on || alwaysVisible ? "fav on-row" : "fav"}
      data-on={on}
      aria-pressed={on}
      aria-label={t(on ? "table.removeFavorite" : "table.addFavorite", { title: track.title })}
      tabIndex={-1}
      onClick={(event) => {
        event.stopPropagation();
        setFavorite(track, !on);
      }}
      onDoubleClick={(event) => event.stopPropagation()}
    >
      <Icon name={on ? "heart-filled" : "heart"} size={1.125} />
    </button>
  );
}

function rowHeightPx(): number {
  const rootSize = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  return rootSize * ROW_REM;
}

interface TrackTableProps {
  rows: TrackRow[];
  /** The list playback continues through, in displayed order without the filter. */
  playList: Track[];
  /** Label for assistive technology, e.g. the playlist name. */
  label: string;
  playlistId?: string;
  /** How many entries the stored list has (for Move up/down limits). */
  entryCount?: number;
  /** Present only when entries may be reordered (stored order shown, no filter). */
  onMove?: (from: number, to: number) => void;
  sort?: Sort;
  onSort?: (field: SortField) => void;
}

/**
 * Track list from the reference: #, artwork + title/artist, album, date added,
 * quality, favorite, duration. Clicking a row opens Song Details
 * (CLAUDE.md §12); the number turns into a play button, and double-click
 * also plays. With the keyboard: arrows move between rows, Enter opens
 * details, Space plays, and the Menu key (or Shift+F10) opens the actions.
 */
export function TrackTable(props: TrackTableProps) {
  const { rows, playList, label, playlistId, entryCount, onMove, sort, onSort } = props;
  const { playback, artworkUrl, playTracks, toggle } = useLibrary();
  const { openTrackMenu, showDetails } = useUi();
  const listRef = useRef<HTMLDivElement>(null);
  const virtual = rows.length > VIRTUAL_THRESHOLD;
  const [range, setRange] = useState({ start: 0, end: virtual ? OVERSCAN * 2 : rows.length });
  const [rowHeight, setRowHeight] = useState(56);
  const [focusRow, setFocusRow] = useState(0);
  const pendingFocus = useRef(false);
  const [drag, setDrag] = useState<{ from: number; over: number | null } | null>(null);

  // Keep only the rows near the viewport in the page.
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list || !virtual) {
      setRange({ start: 0, end: rows.length });
      return;
    }
    const scroller = list.closest<HTMLElement>(".content") ?? document.documentElement;
    const update = () => {
      const height = rowHeightPx();
      setRowHeight(height);
      const listTop =
        list.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
      const next = visibleRange(
        scroller.scrollTop,
        scroller.clientHeight,
        listTop,
        height,
        rows.length,
        OVERSCAN,
      );
      setRange((current) =>
        current.start === next.start && current.end === next.end ? current : next,
      );
    };
    update();
    scroller.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    return () => {
      scroller.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, [rows.length, virtual]);

  // Move keyboard focus to a row, scrolling it into the rendered range first.
  useEffect(() => {
    if (!pendingFocus.current) return;
    const list = listRef.current;
    if (!list) return;
    const target = list.querySelector<HTMLElement>(`[data-row="${focusRow}"]`);
    if (target) {
      pendingFocus.current = false;
      target.focus();
      target.scrollIntoView?.({ block: "nearest" });
      return;
    }
    const scroller = list.closest<HTMLElement>(".content");
    if (scroller) {
      const listTop =
        list.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
      scroller.scrollTop = listTop + focusRow * rowHeight - scroller.clientHeight / 2;
    }
  }, [focusRow, range, rowHeight]);

  const focusOn = (index: number) => {
    const next = Math.min(rows.length - 1, Math.max(0, index));
    pendingFocus.current = true;
    setFocusRow(next);
  };

  const headerCell = (field: SortField, text: string, className?: string) => {
    const active = sort?.field === field;
    const ariaSort = active ? (sort?.descending ? "descending" : "ascending") : undefined;
    return (
      <span className={className} role="columnheader" aria-sort={ariaSort}>
        {onSort ? (
          <button className="sort-header" onClick={() => onSort(field)}>
            {text}
            {active && <Icon name={sort?.descending ? "chevron-down" : "chevron-up"} size={0.875} />}
          </button>
        ) : (
          text
        )}
      </span>
    );
  };

  const visible = rows.slice(range.start, range.end);
  const safeFocus = Math.min(focusRow, Math.max(0, rows.length - 1));

  return (
    <div
      className="tracks"
      role="table"
      aria-label={label}
      aria-rowcount={rows.length + 1}
      aria-description={t("table.rowHint")}
      ref={listRef}
    >
      <div className="track-row track-head" role="row" aria-rowindex={1}>
        <span className="col-index" role="columnheader">
          {onSort ? (
            <button
              className="sort-header"
              aria-label={t("sort.default")}
              title={t("sort.default")}
              onClick={() => onSort("default")}
            >
              #
            </button>
          ) : (
            "#"
          )}
        </span>
        {headerCell("title", t("table.title"))}
        {headerCell("album", t("table.album"), "col-album")}
        {headerCell("added", t("table.dateAdded"), "col-date")}
        <span className="col-quality" role="columnheader">
          {t("table.quality")}
        </span>
        <span role="columnheader" aria-label={t("table.favorite")} />
        <span className="col-duration" role="columnheader" aria-sort={sort?.field === "duration" ? (sort.descending ? "descending" : "ascending") : undefined}>
          {onSort ? (
            <button className="sort-header" aria-label={t("table.duration")} onClick={() => onSort("duration")}>
              <Icon name="clock" size={1.125} />
            </button>
          ) : (
            <Icon name="clock" size={1.125} />
          )}
        </span>
        <span role="columnheader" aria-label={t("table.more")} />
      </div>

      <div
        className="track-body"
        role="rowgroup"
        style={
          virtual
            ? {
                paddingTop: range.start * rowHeight,
                paddingBottom: Math.max(0, rows.length - range.end) * rowHeight,
              }
            : undefined
        }
      >
        {visible.map((row, offset) => {
          const position = range.start + offset;
          const { track } = row;
          const context: TrackContext = {
            list: playList,
            index: row.playIndex,
            playlistId,
            entryIndex: row.sourceIndex,
            entryCount,
            onMove,
          };
          const current = playback.track?.id === track.id;
          const play = () => (current ? toggle() : playTracks(playList, row.playIndex));
          const menuAt = (x: number, y: number) => openTrackMenu(track, context, x, y);
          const onContextMenu = (event: MouseEvent) => {
            event.preventDefault();
            event.stopPropagation();
            menuAt(event.clientX, event.clientY);
          };
          const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
            if (event.target !== event.currentTarget) return;
            const box = event.currentTarget.getBoundingClientRect();
            switch (event.key) {
              case "ArrowDown":
                focusOn(position + 1);
                break;
              case "ArrowUp":
                focusOn(position - 1);
                break;
              case "Home":
                focusOn(0);
                break;
              case "End":
                focusOn(rows.length - 1);
                break;
              case "Enter":
                showDetails(track, context);
                break;
              case " ":
                if (track.available) play();
                break;
              case "ContextMenu":
                menuAt(box.left + 48, box.bottom - 8);
                break;
              case "F10":
                if (!event.shiftKey) return;
                menuAt(box.left + 48, box.bottom - 8);
                break;
              default:
                return;
            }
            event.preventDefault();
          };
          return (
            <div
              key={`${row.sourceIndex}:${track.id}`}
              className="track-row"
              role="row"
              aria-rowindex={position + 2}
              aria-current={current ? "true" : undefined}
              data-row={position}
              data-unavailable={!track.available}
              data-drop={drag?.over === row.sourceIndex && drag.from !== row.sourceIndex}
              tabIndex={position === safeFocus ? 0 : -1}
              draggable={Boolean(onMove)}
              onFocus={(event) => {
                if (event.target === event.currentTarget) setFocusRow(position);
              }}
              onKeyDown={onKeyDown}
              onClick={() => showDetails(track, context)}
              onDoubleClick={() => track.available && play()}
              onContextMenu={onContextMenu}
              onDragStart={(event) => {
                event.dataTransfer.effectAllowed = "move";
                event.dataTransfer.setData("text/plain", String(row.sourceIndex));
                setDrag({ from: row.sourceIndex, over: null });
              }}
              onDragOver={(event) => {
                if (!drag || !onMove) return;
                event.preventDefault();
                event.dataTransfer.dropEffect = "move";
                if (drag.over !== row.sourceIndex) setDrag({ ...drag, over: row.sourceIndex });
              }}
              onDrop={(event) => {
                event.preventDefault();
                if (drag && onMove && drag.from !== row.sourceIndex) onMove(drag.from, row.sourceIndex);
                setDrag(null);
              }}
              onDragEnd={() => setDrag(null)}
            >
              <span className="col-index num" role="cell">
                <button
                  className="row-play"
                  tabIndex={-1}
                  aria-label={t(current && playback.playing ? "table.pause" : "table.play", {
                    title: track.title,
                  })}
                  disabled={!track.available}
                  onClick={(event) => {
                    event.stopPropagation();
                    play();
                  }}
                  onDoubleClick={(event) => event.stopPropagation()}
                >
                  <span className="row-play-rest">
                    {current ? <Equalizer active={playback.playing} /> : position + 1}
                  </span>
                  <span className="row-play-hover">
                    <Icon name={current && playback.playing ? "pause" : "play"} size={1.125} />
                  </span>
                </button>
              </span>
              <span className="track-main" role="cell">
                <Artwork src={artworkUrl(track.artwork, true)} size={2.5} />
                <span className="track-text">
                  <span className="track-title truncate">{track.title}</span>
                  <span className="track-artist truncate">
                    {track.available ? track.artist : `${track.artist} · ${t("table.fileMissing")}`}
                  </span>
                </span>
              </span>
              <span className="col-album truncate" role="cell">
                {track.album}
              </span>
              <span className="col-date truncate num" role="cell">
                {formatDate(track.addedAt)}
              </span>
              <span className="col-quality truncate num" role="cell">
                {qualityText(track)}
              </span>
              <span role="cell">
                <FavoriteButton track={track} />
              </span>
              <span className="col-duration num" role="cell">
                {formatDuration(track.durationMs)}
              </span>
              <span role="cell">
                <button
                  className="fav"
                  tabIndex={-1}
                  aria-label={t("table.moreFor", { title: track.title })}
                  aria-haspopup="menu"
                  onClick={(event) => {
                    event.stopPropagation();
                    const box = event.currentTarget.getBoundingClientRect();
                    menuAt(box.left, box.bottom + 4);
                  }}
                  onDoubleClick={(event) => event.stopPropagation()}
                >
                  <Icon name="dots" size={1.125} />
                </button>
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
