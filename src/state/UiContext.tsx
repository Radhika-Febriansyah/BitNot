import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { Artwork } from "../components/Artwork";
import { ConfirmDialog, Dialog, PromptDialog } from "../components/Dialog";
import { Icon } from "../components/Icon";
import { t, tn } from "../i18n";
import { api } from "../lib/api";
import { addMusicFolder } from "../lib/folders";
import {
  audioFacts,
  formatDate,
  formatDuration,
  qualityText,
  type Collection,
  type Track,
  type TrackEdit,
} from "../lib/library";
import { useApp } from "./AppContext";
import { useLibrary } from "./LibraryContext";

export type MenuItem =
  | { separator: true }
  | {
      label: string;
      onSelect?: () => void;
      /** Opens a second level in place of the menu. */
      children?: MenuItem[];
      danger?: boolean;
      disabled?: boolean;
      /** Set (true or false) for one of a group of choices; shown with a check. */
      checked?: boolean;
    };

interface MenuState {
  items: MenuItem[];
  x: number;
  y: number;
  /** Element to give focus back to when the menu closes. */
  returnFocus: HTMLElement | null;
}

/** Where a track is shown, so "Play" continues through the same list. */
export interface TrackContext {
  list: Track[];
  index: number;
  /** Set when the list is a playlist, to offer "Remove from this playlist". */
  playlistId?: string;
  /** The entry's position in the stored list (differs from `index` when sorted). */
  entryIndex?: number;
  entryCount?: number;
  /** Present when entries can be reordered. */
  onMove?: (from: number, to: number) => void;
  /** Present for queue rows: removes this entry from the queue. */
  onRemoveFromQueue?: () => void;
}

interface UiContextValue {
  openMenu: (items: MenuItem[], x: number, y: number) => void;
  openTrackMenu: (track: Track, context: TrackContext, x: number, y: number) => void;
  showDetails: (track: Track, context: TrackContext) => void;
  editDetails: (track: Track) => void;
  newPlaylist: (tracks?: Track[]) => void;
  editPlaylist: (playlist: Collection) => void;
  confirmDeletePlaylist: (id: string, name: string) => void;
  confirmRemoveFromLibrary: (tracks: Track[]) => void;
  saveQueueAsPlaylist: () => void;
  addFolder: () => void;
}

const UiContext = createContext<UiContextValue | null>(null);

function Menu({ state, onClose }: { state: MenuState; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [stack, setStack] = useState<MenuItem[][]>([state.items]);
  const [position, setPosition] = useState({ left: state.x, top: state.y });
  const items = stack[stack.length - 1];

  // Keep the menu inside the window and put focus on its first item.
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const box = node.getBoundingClientRect();
    setPosition({
      left: Math.max(8, Math.min(state.x, window.innerWidth - box.width - 8)),
      top: Math.max(8, Math.min(state.y, window.innerHeight - box.height - 8)),
    });
    node.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [state.x, state.y, items]);

  useEffect(() => {
    const close = (event: Event) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    const key = (event: KeyboardEvent) => {
      const buttons = [
        ...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []),
      ];
      const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const focus = (index: number) => buttons[(index + buttons.length) % buttons.length]?.focus();
      switch (event.key) {
        case "Escape":
          onClose();
          break;
        case "Tab":
          onClose();
          return; // let focus move on naturally
        case "ArrowDown":
          focus(at + 1);
          break;
        case "ArrowUp":
          focus(at - 1);
          break;
        case "Home":
          focus(0);
          break;
        case "End":
          focus(buttons.length - 1);
          break;
        default:
          return;
      }
      event.preventDefault();
    };
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("keydown", key, true);
    window.addEventListener("resize", onClose);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  return (
    <div ref={ref} className="menu" role="menu" style={position}>
      {stack.length > 1 && (
        <button className="menu-item" role="menuitem" onClick={() => setStack(stack.slice(0, -1))}>
          <Icon name="chevron-left" size={1} />
          <span className="menu-label">{t("menu.back")}</span>
        </button>
      )}
      {items.map((item, index) =>
        "separator" in item ? (
          <div key={index} className="menu-separator" role="separator" />
        ) : (
          <button
            key={index}
            className={item.danger ? "menu-item danger" : "menu-item"}
            role={item.checked === undefined ? "menuitem" : "menuitemradio"}
            aria-checked={item.checked}
            aria-haspopup={item.children ? "menu" : undefined}
            disabled={item.disabled}
            onClick={() => {
              if (item.children) {
                setStack([...stack, item.children]);
              } else {
                onClose();
                item.onSelect?.();
              }
            }}
          >
            <span className="menu-label truncate">{item.label}</span>
            {item.checked && (
              <span className="menu-check">
                <Icon name="check" size={1} />
              </span>
            )}
            {item.children && <Icon name="chevron-right" size={1} />}
          </button>
        ),
      )}
    </div>
  );
}

function SongDetails({
  track,
  context,
  onClose,
  onEdit,
}: {
  track: Track;
  context: TrackContext;
  onClose: () => void;
  onEdit: () => void;
}) {
  const { artworkUrl, playTracks, addToQueue, setFavorite, isFavorite, preferSource, versions } =
    useLibrary();
  const favorite = isFavorite(track.id);
  const [sources, setSources] = useState<Track[]>([]);
  const [preferred, setPreferred] = useState(track.id);

  useEffect(() => {
    let cancelled = false;
    if (track.sources > 1) {
      api
        .trackSources(track.id)
        .then((found) => !cancelled && setSources(found))
        .catch(() => undefined);
    }
    return () => {
      cancelled = true;
    };
  }, [track.id, track.sources, versions.tracks]);

  const facts = [
    ...audioFacts(track),
    ...(track.year ? [{ label: t("details.year"), value: String(track.year) }] : []),
    ...(track.genre ? [{ label: t("details.genre"), value: track.genre }] : []),
    ...(track.trackNo ? [{ label: t("details.track"), value: String(track.trackNo) }] : []),
    { label: t("details.plays"), value: String(track.playCount) },
    { label: t("details.added"), value: formatDate(track.addedAt) },
  ];

  return (
    <Dialog open title={t("details.title")} wide onClose={onClose}>
      <div className="details">
        <Artwork src={artworkUrl(track.artwork)} size={11} />
        <div className="details-main">
          <h3 className="details-title">{track.title}</h3>
          <p className="muted">{track.artist}</p>
          <p className="muted">
            {track.album} · {formatDuration(track.durationMs)}
          </p>
          {track.edited && <p className="details-note">{t("details.edited")}</p>}
          {!track.available && (
            <p className="details-warning" role="status">
              {t("details.missing")}
            </p>
          )}
          <div className="details-actions">
            <button
              className="btn btn-primary btn-lg"
              disabled={!track.available}
              onClick={() => {
                playTracks(context.list, context.index);
                onClose();
              }}
            >
              <Icon name="play" size={1.125} />
              {t("menu.play")}
            </button>
            <button
              className="btn btn-lg"
              disabled={!track.available}
              onClick={() => addToQueue([track])}
            >
              {t("menu.addToQueue")}
            </button>
            <button
              className="btn btn-lg"
              aria-pressed={favorite}
              onClick={() => setFavorite(track, !favorite)}
            >
              <Icon name={favorite ? "heart-filled" : "heart"} size={1.125} />
              {favorite ? t("details.inFavorites") : t("menu.addFavorite")}
            </button>
            <button className="btn btn-lg" onClick={onEdit}>
              {t("details.edit")}
            </button>
          </div>
        </div>
      </div>
      <dl className="facts">
        {facts.map((fact) => (
          <div key={fact.label}>
            <dt>{fact.label}</dt>
            <dd>{fact.value}</dd>
          </div>
        ))}
        <div className="facts-wide">
          <dt>{t("details.file")}</dt>
          <dd className="selectable">{track.path}</dd>
        </div>
      </dl>
      {sources.length > 1 && (
        <section className="sources" aria-labelledby="sources-title">
          <h3 id="sources-title">{t("details.versionsTitle")}</h3>
          <p className="muted">{t("details.versionsText")}</p>
          <ul className="rows">
            {sources.map((source) => (
              <li key={source.id} className="row">
                <span className="row-main">
                  <span className="num">{qualityText(source) || source.codec || "—"}</span>
                  {source.id === preferred && <span className="tag accent">{t("details.preferred")}</span>}
                  <span className="muted selectable source-path">{source.path}</span>
                </span>
                <button className="btn" onClick={() => playTracks([source], 0)}>
                  {t("details.playThis")}
                </button>
                {source.id !== preferred && (
                  <button
                    className="btn btn-ghost"
                    onClick={() => {
                      preferSource(source);
                      setPreferred(source.id);
                    }}
                  >
                    {t("details.prefer")}
                  </button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
      <div className="dialog-actions">
        <button className="btn btn-lg" onClick={onClose}>
          {t("details.close")}
        </button>
      </div>
    </Dialog>
  );
}

/** Edits a track's details in BitNot. The audio file is never written. */
function DetailsEditor({ track, onClose }: { track: Track; onClose: () => void }) {
  const { editTrack, chooseArtwork, clearArtwork } = useLibrary();
  const { notify } = useApp();
  const [genres, setGenres] = useState<string[]>([]);
  const [form, setForm] = useState({
    title: track.title,
    artist: track.artist,
    album: track.album,
    genre: track.genre ?? "",
    year: track.year ? String(track.year) : "",
    trackNo: track.trackNo ? String(track.trackNo) : "",
  });
  const id = useId();

  useEffect(() => {
    api.genres().then(setGenres).catch(() => undefined);
  }, []);

  const set = (field: keyof typeof form) => (value: string) =>
    setForm((current) => ({ ...current, [field]: value }));
  const number = (text: string) => (text.trim() ? Number(text) : null);

  const save = async (edit: TrackEdit) => {
    const saved = await editTrack(track.id, edit);
    if (saved) {
      notify(t("edit.saved"));
      onClose();
    }
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void save({
      title: form.title,
      artist: form.artist,
      album: form.album,
      genre: form.genre,
      year: number(form.year),
      trackNo: number(form.trackNo),
    });
  };
  // A genre from the file that is not in the fixed list stays selectable as-is.
  const genreOptions = form.genre && !genres.includes(form.genre) ? [form.genre, ...genres] : genres;
  const text = (field: "title" | "artist" | "album", label: string) => (
    <label className="form-field" htmlFor={`${id}-${field}`}>
      <span>{label}</span>
      <input
        id={`${id}-${field}`}
        className="input selectable"
        value={form[field]}
        maxLength={200}
        required
        onChange={(event) => set(field)(event.target.value)}
      />
    </label>
  );

  return (
    <Dialog open title={t("edit.title")} wide onClose={onClose}>
      <p className="muted">{t("edit.note")}</p>
      <form className="form" onSubmit={submit}>
        {text("title", t("edit.fieldTitle"))}
        {text("artist", t("edit.fieldArtist"))}
        {text("album", t("edit.fieldAlbum"))}
        <div className="form-row">
          <label className="form-field" htmlFor={`${id}-genre`}>
            <span>{t("edit.fieldGenre")}</span>
            <select
              id={`${id}-genre`}
              className="select"
              value={form.genre}
              onChange={(event) => set("genre")(event.target.value)}
            >
              <option value="">{t("edit.genreNone")}</option>
              {genreOptions.map((genre) => (
                <option key={genre} value={genre}>
                  {genre}
                </option>
              ))}
            </select>
          </label>
          <label className="form-field" htmlFor={`${id}-year`}>
            <span>{t("edit.fieldYear")}</span>
            <input
              id={`${id}-year`}
              className="input"
              type="number"
              min={1000}
              max={9999}
              value={form.year}
              onChange={(event) => set("year")(event.target.value)}
            />
          </label>
          <label className="form-field" htmlFor={`${id}-track`}>
            <span>{t("edit.fieldTrack")}</span>
            <input
              id={`${id}-track`}
              className="input"
              type="number"
              min={1}
              max={9999}
              value={form.trackNo}
              onChange={(event) => set("trackNo")(event.target.value)}
            />
          </label>
        </div>
        <div className="form-field">
          <span>{t("edit.artwork")}</span>
          <div className="section-actions">
            <button type="button" className="btn" onClick={() => chooseArtwork([track])}>
              {t("edit.chooseArtwork")}
            </button>
            <button type="button" className="btn btn-ghost" onClick={() => clearArtwork([track])}>
              {t("edit.resetArtwork")}
            </button>
          </div>
        </div>
        <div className="dialog-actions">
          {track.edited && (
            <button
              type="button"
              className="btn btn-ghost btn-lg"
              onClick={() =>
                void save({ title: null, artist: null, album: null, genre: null, year: null, trackNo: null })
              }
            >
              {t("edit.reset")}
            </button>
          )}
          <span className="spacer" />
          <button type="button" className="btn btn-ghost btn-lg" onClick={onClose}>
            {t("edit.cancel")}
          </button>
          <button type="submit" className="btn btn-primary btn-lg">
            {t("edit.save")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}

function PlaylistEditor({ playlist, onClose }: { playlist: Collection; onClose: () => void }) {
  const { updatePlaylist } = useLibrary();
  const [name, setName] = useState(playlist.title);
  const [description, setDescription] = useState(playlist.description);
  const id = useId();
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void updatePlaylist(playlist.id, name, description).then((ok) => ok && onClose());
  };
  return (
    <Dialog open title={t("playlist.editTitle")} onClose={onClose}>
      <form className="form" onSubmit={submit}>
        <label className="form-field" htmlFor={`${id}-name`}>
          <span>{t("playlist.name")}</span>
          <input
            id={`${id}-name`}
            className="input selectable"
            value={name}
            maxLength={100}
            required
            autoFocus
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <label className="form-field" htmlFor={`${id}-description`}>
          <span>{t("playlist.description")}</span>
          <textarea
            id={`${id}-description`}
            className="input textarea selectable"
            value={description}
            maxLength={300}
            rows={3}
            onChange={(event) => setDescription(event.target.value)}
          />
        </label>
        <div className="dialog-actions">
          <button type="button" className="btn btn-ghost btn-lg" onClick={onClose}>
            {t("playlist.cancel")}
          </button>
          <button type="submit" className="btn btn-primary btn-lg" disabled={!name.trim()}>
            {t("playlist.save")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}

type Prompt =
  | { kind: "new"; tracks: Track[] }
  | { kind: "queue" }
  | { kind: "delete"; id: string; name: string }
  | { kind: "remove"; tracks: Track[] }
  | null;

/** Menus and dialogs shared by every page. */
export function UiProvider({ children }: { children: ReactNode }) {
  const app = useApp();
  const library = useLibrary();
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [details, setDetails] = useState<{ track: Track; context: TrackContext } | null>(null);
  const [editing, setEditing] = useState<Track | null>(null);
  const [playlistEdit, setPlaylistEdit] = useState<Collection | null>(null);
  const [prompt, setPrompt] = useState<Prompt>(null);
  const latest = useRef({ app, library });
  latest.current = { app, library };

  const closeMenu = useCallback(() => {
    setMenu((current) => {
      // Give focus back to the row or button the menu came from.
      const target = current?.returnFocus;
      if (target?.isConnected) window.setTimeout(() => target.focus(), 0);
      return null;
    });
  }, []);

  const value = useMemo<UiContextValue>(() => {
    const openMenu: UiContextValue["openMenu"] = (items, x, y) =>
      setMenu({ items, x, y, returnFocus: document.activeElement as HTMLElement | null });
    const showDetails: UiContextValue["showDetails"] = (track, context) =>
      setDetails({ track, context });
    return {
      openMenu,
      showDetails,
      editDetails: (track) => setEditing(track),
      newPlaylist: (tracks = []) => setPrompt({ kind: "new", tracks }),
      editPlaylist: (playlist) => setPlaylistEdit(playlist),
      confirmDeletePlaylist: (id, name) => setPrompt({ kind: "delete", id, name }),
      confirmRemoveFromLibrary: (tracks) => setPrompt({ kind: "remove", tracks }),
      saveQueueAsPlaylist: () => setPrompt({ kind: "queue" }),
      addFolder: () => {
        const { app: a, library: l } = latest.current;
        void addMusicFolder(a.settings, a.updateSettings, a.notify).then((added) => {
          if (added) void l.startScan();
        });
      },
      openTrackMenu: (track, context, x, y) => {
        const l = latest.current.library;
        const playlists = l.overview.playlists;
        const favorite = l.isFavorite(track.id);
        const items: MenuItem[] = [
          {
            label: t("menu.play"),
            disabled: !track.available,
            onSelect: () => l.playTracks(context.list, context.index),
          },
          { label: t("menu.playNext"), disabled: !track.available, onSelect: () => l.playNext(track) },
          {
            label: t("menu.addToQueue"),
            disabled: !track.available,
            onSelect: () => l.addToQueue([track]),
          },
          { separator: true },
          {
            label: t("menu.addToPlaylist"),
            children: [
              { label: t("menu.newPlaylist"), onSelect: () => setPrompt({ kind: "new", tracks: [track] }) },
              ...(playlists.length > 0 ? [{ separator: true } as MenuItem] : []),
              ...playlists.map(
                (playlist): MenuItem => ({
                  label: playlist.title,
                  onSelect: () => l.addToPlaylist(playlist.id, [track], playlist.title),
                }),
              ),
            ],
          },
          {
            label: favorite ? t("menu.removeFavorite") : t("menu.addFavorite"),
            onSelect: () => l.setFavorite(track, !favorite),
          },
          { label: t("menu.viewDetails"), onSelect: () => showDetails(track, context) },
          { label: t("menu.editDetails"), onSelect: () => setEditing(track) },
        ];
        const { entryIndex, entryCount, onMove } = context;
        if (onMove && entryIndex !== undefined && entryCount !== undefined) {
          items.push(
            { separator: true },
            {
              label: t("menu.moveUp"),
              disabled: entryIndex === 0,
              onSelect: () => onMove(entryIndex, entryIndex - 1),
            },
            {
              label: t("menu.moveDown"),
              disabled: entryIndex >= entryCount - 1,
              onSelect: () => onMove(entryIndex, entryIndex + 1),
            },
          );
        }
        items.push({ separator: true });
        if (context.onRemoveFromQueue) {
          items.push({ label: t("menu.removeFromQueue"), onSelect: context.onRemoveFromQueue });
        }
        if (context.playlistId && entryIndex !== undefined) {
          const playlistId = context.playlistId;
          items.push({
            label: t("menu.removeFromPlaylist"),
            onSelect: () => l.removeFromPlaylist(playlistId, entryIndex),
          });
        }
        items.push({
          label: t("menu.removeFromLibrary"),
          danger: true,
          onSelect: () => setPrompt({ kind: "remove", tracks: [track] }),
        });
        openMenu(items, x, y);
      },
    };
  }, []);

  const closePrompt = () => setPrompt(null);
  const openCreated = (id: string | null) => {
    if (id) app.go({ page: "collection", id });
  };

  return (
    <UiContext.Provider value={value}>
      {children}
      {menu && <Menu key={`${menu.x}:${menu.y}`} state={menu} onClose={closeMenu} />}
      {details && !editing && (
        <SongDetails
          key={details.track.id}
          track={details.track}
          context={details.context}
          onClose={() => setDetails(null)}
          onEdit={() => {
            setEditing(details.track);
            setDetails(null);
          }}
        />
      )}
      {editing && <DetailsEditor key={editing.id} track={editing} onClose={() => setEditing(null)} />}
      {playlistEdit && (
        <PlaylistEditor key={playlistEdit.id} playlist={playlistEdit} onClose={() => setPlaylistEdit(null)} />
      )}
      <PromptDialog
        open={prompt?.kind === "new"}
        title={t("playlist.newTitle")}
        label={t("playlist.name")}
        submitLabel={t("playlist.create")}
        maxLength={100}
        onSubmit={(name) => {
          const tracks = prompt?.kind === "new" ? prompt.tracks : [];
          void library.createPlaylist(name, tracks).then(openCreated);
        }}
        onClose={closePrompt}
      />
      <PromptDialog
        open={prompt?.kind === "queue"}
        title={t("queue.saveTitle")}
        label={t("playlist.name")}
        submitLabel={t("playlist.create")}
        maxLength={100}
        onSubmit={(name) => void library.saveQueueAsPlaylist(name).then(openCreated)}
        onClose={closePrompt}
      />
      <ConfirmDialog
        open={prompt?.kind === "delete"}
        title={t("playlist.deleteTitle", { name: prompt?.kind === "delete" ? prompt.name : "" })}
        message={t("playlist.deleteText")}
        confirmLabel={t("playlist.deleteConfirm")}
        danger
        onConfirm={() => {
          if (prompt?.kind === "delete") {
            const id = prompt.id;
            void library.deletePlaylist(id).then((ok) => {
              const route = latest.current.app.route;
              if (ok && route.page === "collection" && route.id === id) {
                latest.current.app.go({ page: "library", filter: "playlists" });
              }
            });
          }
        }}
        onClose={closePrompt}
      />
      <ConfirmDialog
        open={prompt?.kind === "remove"}
        title={tn("remove.title", prompt?.kind === "remove" ? prompt.tracks.length : 1)}
        message={t("remove.text")}
        confirmLabel={t("remove.confirm")}
        danger
        onConfirm={() => {
          if (prompt?.kind === "remove") library.removeFromLibrary(prompt.tracks);
        }}
        onClose={closePrompt}
      />
    </UiContext.Provider>
  );
}

export function useUi(): UiContextValue {
  const value = useContext(UiContext);
  if (!value) throw new Error("useUi must be used inside UiProvider");
  return value;
}
