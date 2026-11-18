# BitNot — Post-Phase 2 Audit

Date: 2026-11-16. Scope: the repository as it stands after Phase 2. No application code, tests, or configuration were changed for this audit; this report is the only file added.

## Summary

- Phases 1 and 2 are in place and every automated check passes.
- Phase 2 is **not fully complete** against `CLAUDE.md`: nine Phase 2 items are still open (section 4).
- **Nothing has ever been committed.** The whole project is untracked files on `main` with no remote. This is the largest risk in the repository today.
- Three real defects were found by reading the code (section 6). None is data-destroying.
- Real playback has only been exercised with WAV files at volume 0. Other formats and audible quality are unverified.

## 1. Architecture

| Layer | Technology | Notes |
|---|---|---|
| Desktop shell | Tauri 2 (WebView2 window, custom title bar) | 41 commands in `src-tauri/src/lib.rs`, each a thin wrapper |
| Core | Rust, 13 files, 5,145 lines | `core.rs` owns the databases and the playback controller; no UI dependency |
| Database | SQLite via `rusqlite` (bundled), WAL | `app.db` (profiles, app state) and one `profile.db` per profile; forward-only migrations (app v1, profile v2) |
| Audio engine | `rodio` 0.23: Symphonia decoders, cpal/WASAPI output | Behind an `Output` trait; default device opened once at start |
| Tags | `lofty` 0.25, read-only | |
| Interface | React 19, TypeScript 6, Vite 8; 33 source files, about 4,150 lines, plus 1,960 lines of CSS | Hand-rolled router; three contexts (app, library, UI overlays); no UI or state library |
| Fonts | Inter, bundled | |
| Tests | `cargo test`, Vitest | |
| CI | One GitHub Actions workflow | Has never run |

Data lives in `%APPDATA%\app.bitnot.desktop`: `app.db`, `profiles/<id>/profile.db`, `artwork/` (shared, content-addressed), `logs/`.

Module boundaries match `CLAUDE.md` §43 for what exists: scanner, library queries, queue logic, playback, settings, profiles, diagnostics, and database are separate modules with their own tests.

## 2. Test and check results

All run on 2026-11-16 on Windows 11 (build 26200).

| Check | Result |
|---|---|
| `tsc --noEmit` | Pass |
| Vitest | 19 of 19 pass (3 files) |
| Frontend production build | Pass: 308 kB JS (92 kB gzip), 27 kB CSS |
| `cargo fmt --check` | Clean |
| `cargo clippy --all-targets -- -D warnings` | Clean |
| `cargo test` | 72 of 72 pass |

Rust tests by module: playback 11, queue 10, scanner 10, profiles 8, core 7, library 7, db 6, settings 6, diagnostics 5, paths 2.

Coverage gaps:

- **No interface tests.** The 19 frontend tests cover pure functions only (router, theme, formatting). No component, context, or interaction is tested automatically.
- **No test plays real audio.** The controller is tested against a fake output; `DeviceOutput` is untested by automation.
- **No test of the command layer** (`lib.rs`) or of the events it emits.
- **No frontend linter or formatter is configured.** There is no ESLint or Prettier config, yet two `eslint-disable` comments exist in the source.
- Not run for this audit: installer build (`tauri build`), dependency vulnerability scans, and anything on Windows 10.

## 3. Repository status

- Branch `main`, **zero commits**, no remote.
- 14 untracked top-level entries, which is the entire project: `.github/`, `.gitignore`, `CLAUDE.md`, `LICENSE`, `README.md`, `docs/`, `index.html`, `package-lock.json`, `package.json`, `src-tauri/`, `src/`, `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`.
- Correctly ignored: `node_modules/`, `dist/`, `src-tauri/target/`, `src-tauri/gen/`.
- Nothing was committed, reset, or discarded during this audit.

## 4. Feature status against `CLAUDE.md`

"Complete" means built, tested, and exercised in the running app. "Partial" lists what is missing.

### Complete

| Area | Spec | Evidence |
|---|---|---|
| App shell, navigation, back/forward | §5 | Run in app |
| Branding, dark/light themes, accent, display scale, Reduce Motion | §3, §7 | Run in app (light theme not re-checked since the redesign) |
| AppData storage, migrations, integrity check on open | §31, §34 | Tests |
| Settings, per profile, validated | §29 | Tests, run in app |
| Profiles: create, rename, switch, main rule, delete, restore, 90-day purge | §29 | Tests, run in app |
| Folder scan: incremental, missing/detached handling, skipped-file reporting | §7 | Tests, run in app |
| Tags, technical info, embedded and folder artwork | §7, §9 | Tests, run in app |
| Playback: play, pause, seek, next, previous, volume, shuffle, repeat, auto-advance | §19 | Tests, run in app (WAV only) |
| Favorites: add, remove, view, play all | §11 | Tests, run in app |
| Play count, incremented at start | §11 | Tests, run in app |
| Recently Played: newest first, replay, remove one, clear | §11 | Tests, run in app |
| Song Details on single click, with audio information | §12, §14 | Run in app |
| Local search: instant, exact, ranked, five recent searches, no-results state | §14 | Tests, run in app |
| Queue restore after restart | §11 | Tests, run in app |
| Local diagnostics: log, viewer, delete, crash detection | §34 | Tests, run in app |
| Offline operation | §1 | The app has no network code |

### Partial

| Area | Spec | Built | Missing |
|---|---|---|---|
| Playlists | §11 | Create, rename, delete with confirmation, add, remove, persistent order | Description, custom cover, drag-and-drop reorder |
| Queue | §11 | Play Next, Add, remove, clear, Now playing / Next up | Drag-and-drop reorder, Save Queue as Playlist |
| Track context menu | §12 | Play, Play Next, Add to Queue, Add to Playlist, Favorites, View Details | Remove from Library |
| Library views | §9 | Songs list; album, artist, playlist grids; filters | Grid/List toggle, Recently Added, quality column |
| Sorting | §12 | Default orders (album order, playlist order) | Sort by title/artist/duration/date added, revert to default; Favorites sorting |
| Artist page | §12 | Name, artwork, all tracks, Play, Shuffle | Popular Tracks, album discography |
| Album page | §12 | Artwork, title, artist, year, tracks, Play, Shuffle | Nothing notable |
| Scanning | §7 | Startup scan, rescan, on folder change | Monitoring folders while running |
| Artwork management | §9 | Cache, de-duplication | Resizing, cache clean-up, user upload, animated artwork |
| Supported formats | §8 | MP3, FLAC, WAV, M4A/AAC/ALAC, Ogg Vorbis, AIFF accepted | Only WAV played for real; Opus, WMA, APE, DSD unsupported and reported |
| Quick Setup | §32 | Language, theme, music folder, finish | Basic audio preferences step |
| Recycle Bin | §28 | Profiles only | Playlists, favorites, metadata; early permanent delete |
| Crash recovery | §34 | Detection, queue and position restore | Last page restore, automatic repair |
| Accessibility | §37 | Labels on controls, focus rings, Reduce Motion, scale | Keyboard access to row actions is partial; no screen-reader pass; no contrast setting |
| Languages | §32 | English | No translation layer; strings are inline |
| GitHub workflow | §40 | CI workflow file | Templates, contribution guide, release automation; CI has never run |
| Responsive layout | §5, §6 | Sidebar rail, queue overlay, column hiding | Queue as separate window |

### Not implemented

- **Phase 2 leftovers:** metadata editing (§9), duplicate handling and source selection (§10), genre list (§13), multilingual search and diacritics (§14).
- **Phase 3:** Now Playing page, Mini Player, Fullscreen Lyrics, lyrics, equalizer, audio effects, crossfade, gapless, sleep timer, visualizer, keyboard shortcuts.
- **Phase 4:** all online music, Explore content, streaming, downloads.
- **Phase 5:** Windows media session, lock screen, media keys, output device selection and disconnect recovery, audio focus, tray, startup and close behaviour, notifications.
- **Phase 6:** Google identity, pairing, sync, network management.
- **Phase 7:** backup and restore, error reporting, installer and uninstaller, update check.

## 5. Code quality

**Good:**

- No `TODO`, `FIXME`, or `unimplemented!` markers.
- Outside tests, the only `expect` is the Tauri builder at start-up.
- Errors cross the command boundary as typed `{ kind, message }` values with user-readable text.
- Start-up failure (damaged or newer database) is explained in the window and the file is left untouched.
- The scanner never writes to music folders; a test asserts files are byte-identical after scanning.

**Technical debt:**

1. **No translation layer.** All interface text is inline English. Every new page adds to the cost of introducing one later.
2. **No frontend lint or format tooling**, so style and hook-dependency mistakes are only caught by review.
3. **Stale CSS.** At least four rule groups have no users (`.bar-fill`, `.lib-item-playing`, `.titlebar-tag`, `.explicit`), left over from the removed sample content.
4. **Small duplications.** The "Play all" block (Songs and Favorites), the music-folder list (Settings and Quick Setup), and the avatar initial logic each exist twice.
5. **Hand-rolled router and menu.** Both are small and tested or simple, but they will need features (focus return, nested routes) as pages are added.
6. **`implementation-log.md` Phase 1 section is stale** in places (it still says music folders are "stored, not scanned").
7. **Playback changes made through the core do not broadcast an event**; the window updates from the command's return value. This is correct for one window and wrong the moment a second window (Mini Player, separate queue) exists.
8. **The audio device is opened once at start-up.** If the default device changes or is unplugged, playback fails until restart. This is Phase 5 scope but affects daily use now.

## 6. Known defects

| # | Defect | Impact | Where |
|---|---|---|---|
| 1 | In a playlist that contains the same track twice, with the filter field in use, Play and "Remove from this playlist" on the second copy act on the first copy. | Wrong entry removed or wrong start position; only with duplicates and an active filter | `TrackTable.tsx` (index found with `indexOf`) |
| 2 | "Add all to Queue" sends one request and shows one notification per track. | Slow and noisy on large albums or playlists | `Collection.tsx` |
| 3 | Restarting during development (auto-rebuild) kills the app without a clean shutdown, so the next launch shows the "did not close properly" notice. | Development only; misleading notice | Dev workflow |

Unverified rather than known-broken:

- Real playback of MP3, FLAC, M4A, Ogg Vorbis, and AIFF.
- Audible output quality, and whether playback stutters during a scan.
- Light theme after the redesign.
- Behaviour on Windows 10.

Non-code risk raised earlier and still open: the supplied logo closely resembles Apple Music's icon, which matters for a public release.

## 7. Performance concerns

Ordered by expected user impact.

1. **Track lists are not virtualised.** Every row is rendered. Fine for hundreds of tracks; a Songs view of several thousand will be slow to open and scroll.
2. **Every library change refetches whole lists.** A favorite toggle, or each new track starting, reloads and re-renders the Songs, Favorites, or collection view in full.
3. **Search reads the entire track table on every keystroke** and filters it in Rust, then runs two grouping queries. Acceptable for small libraries; it also holds the core lock, which delays track changes while it runs.
4. **Artwork is shown at original size.** A 3000-pixel cover is decoded for a 40-pixel thumbnail, once per visible row. Memory grows with library size, and nothing cleans the cache.
5. **Folder covers are re-read and re-hashed for every track** in the folder during a first scan.
6. **The volume slider writes to the database on every step** while dragging.
7. **The queue is serialised in full on every change**, including each track change.

None of these is visible with the seven-track test library used so far.

## 8. Recommended priorities before and during Phase 3

### Do first (before Phase 3 work)

| Priority | Item | Why |
|---|---|---|
| 1 | **Make the first commit and push to a remote.** | There is no history or backup of any kind. Also lets CI run for the first time. |
| 2 | **Play real files of each supported format, with sound on.** | Phase 3 builds EQ, crossfade, and gapless on this engine; its real behaviour must be known first. |
| 3 | **Fix defects 1 and 2.** | Small, contained, and one of them removes the wrong playlist entry. |
| 4 | **Decide on the audio pipeline.** | See below; it gates most of Phase 3. |

### The gating decision: audio pipeline

Equalizer, effects, crossfade, gapless, and the visualizer all need access to decoded samples between the decoder and the device. `rodio` allows custom sources in that position, but crossfade and gapless need two tracks decoded and mixed at once, which the current single-player design does not do. A short proof of concept (one EQ band, one crossfade between two files, sample taps for a visualizer) should decide whether to extend `rodio` or replace the output with a custom Symphonia + WASAPI pipeline. The `Output` trait means the controller and its tests survive either choice.

### Phase 3 order

| Order | Feature | Depends on | Risk | User value |
|---|---|---|---|---|
| 1 | Keyboard shortcuts (§20) | Nothing | Low | High, immediate |
| 2 | Now Playing page (§19) | Nothing | Low | High |
| 3 | Sleep timer (§20) | Nothing | Low | Medium |
| 4 | Audio pipeline proof of concept | Item 2 above | **High** | Enables 5–8 |
| 5 | Gapless playback | Pipeline | High | High for albums |
| 6 | Equalizer with presets and per-device profiles | Pipeline | Medium | High |
| 7 | Crossfade | Pipeline, gapless work | Medium | Medium |
| 8 | Visualizer | Pipeline sample tap | Medium | Low to medium |
| 9 | Mini Player | Playback events broadcast to all windows (debt item 7) | Medium | Medium |
| 10 | Lyrics provider and Fullscreen Lyrics | Licensing review of a source | **High (legal)** | High |
| 11 | Audio effects (bass boost, surround, reverb, spatial) | Pipeline, EQ | High; several may be unsupported | Low to medium |
| 12 | Animated artwork | Artwork resizing and cache work | Medium | Low |
| 13 | Accessibility pass (keyboard-only, Narrator) | Pages existing | Low | Required by §37 |

### Carry-over to schedule alongside Phase 3

- **Before libraries get large:** list virtualisation, artwork thumbnails, narrower refetching (performance items 1, 2, 4).
- **Cheap Phase 2 completions:** sorting, Grid/List toggle, Save Queue as Playlist, Remove from Library, artist page sections.
- **Larger Phase 2 completions:** drag-and-drop reorder, metadata editing, folder monitoring.
- **Tooling:** ESLint and Prettier, a translation layer, and at least a few component tests for the player bar, track table, and queue.

Bringing output-device change handling forward from Phase 5 is worth considering, because a device change currently requires an app restart.
