# Implementation log

Concise record of what was built, what was decided, and where the implementation deviates from `CLAUDE.md`.

## Decisions

| Date | Decision |
|---|---|
| 2026-11-14 | Stack: Tauri 2 (Rust core, WebView2 window), React 19 + TypeScript + Vite, SQLite through `rusqlite` (bundled). |
| 2026-11-14 | Sidebar is kept on every page and can collapse to an icon rail, which gives Home/Explore the width shown in reference 03. |
| 2026-11-14 | Sidebar contents: Home, Explore, Search; Your Library with chips (Playlists, Albums, Artists, Local, Online); Favorites; Recently Played. Settings is the gear in the top bar. |
| 2026-11-14 | Track lists show Title/Artist/Duration always; Album and Date Added only on wide windows. (Applies from Phase 2.) |
| 2026-11-14 | Reference 02 is treated as the Now Playing **Split Layout**; Centered Artwork will be a variant in the same style. (Phase 3.) |
| 2026-11-14 | The waveform seek bar falls back to a plain bar when no waveform data exists; no fake waveform is drawn. (Phase 3.) |
| 2026-11-14 | Custom title bar (BN wordmark + Windows caption buttons) instead of the native frame, so the window matches the design. |
| 2026-11-14 | Logo: the user supplied the app logo (formerly `docs/design/brand/logo-source.webp`; superseded on 2026-11-18, see Branding update), replacing the BN monogram described in CLAUDE.md §3. Corners outside the rounded square are made transparent for icons. |
| 2026-11-14 | Fonts are bundled (originally Space Grotesk and JetBrains Mono; replaced by Inter on 2026-11-15; SIL OFL). Nothing is fetched at runtime. |

## Phase 1 — Foundation (2026-11-14)

### Built

- **Shell:** title bar, sidebar (full and rail), top bar with back/forward history, collapsible Queue panel (floats over content on narrow windows), persistent player bar.
- **Branding and theme:** user-supplied logo (music note on a rounded square) and BitNot wordmark, app icon set, design tokens, dark and light themes, "Match Windows" theme, six accent presets plus custom colour, display scale 90–150%, Reduce Motion (on / off / match Windows).
- **Storage:** everything under `%APPDATA%\app.bitnot.desktop` — `app.db` (profiles, app state), `profiles/<id>/profile.db` (per-profile settings), `logs/`.
- **Database:** forward-only migrations tracked by `PRAGMA user_version`; integrity check on open; a damaged or newer-schema database is refused and left byte-for-byte unchanged.
- **Settings:** typed, validated, stored one row per field; a damaged value falls back to its default without affecting the others.
- **Profiles:** create, rename, switch, designate main, delete (main cannot be deleted until replaced), restore from a 90-day recycle bin, automatic purge after 90 days.
- **Quick Setup:** language, theme and accent, music folder, finish; every step skippable.
- **Guided Home** (dismissible) and empty states.
- **Diagnostics:** local daily log file, panic hook, 14-day retention, in-app viewer, delete-all; unclean-shutdown detection with a notice on next start.
- **CI:** GitHub Actions workflow (typecheck, frontend tests and build, `cargo fmt`, `clippy -D warnings`, `cargo test`) on Windows.

### Verified

- 32 Rust tests and 10 frontend tests pass; `clippy -D warnings` and `tsc` are clean.
- Run in the real window on Windows 11 (build 26200): Quick Setup, live theme/accent preview, profile create / duplicate rejection / switch / delete / restore, queue and sidebar toggles, log viewer, clean shutdown, persistence across restart, crash notice after a forced kill.
- Every focusable control on Home has an accessible name.

### Not verified

- Windows 10.
- A real screen-reader pass (Narrator) and a full keyboard-only walkthrough. These are scheduled with the Phase 3 accessibility work.
- The CI workflow has not run yet (the repository has not been pushed).
- The NSIS installer bundle has not been built.

### Deviations and limitations

| Spec item | Status | Reason and fallback |
|---|---|---|
| Quick Setup "Basic audio preferences" step | Deferred to Phase 2 | There is no audio engine yet, so any audio preference would be a control that does nothing. The step is added with playback. |
| Language choice | English only | Only English strings exist. The picker lists what the core reports as supported and says so. |
| Music folders | Stored, not scanned | Folder selection is saved per profile. Scanning arrives in Phase 2; the UI states this wherever folders are shown. |
| Search field, playback controls, volume | Present but disabled | They are part of the approved layout. They are disabled with an explanation until their features exist. |
| Recycle Bin for profiles | Minimal | Restore is available in Settings → Profiles. Permanent-delete-early and the full Recycle Bin UI come in Phase 6. |
| Crash recovery | Detection only | Phase 1 detects and reports an unclean shutdown. Restoring queue, position, and last page is Phase 7. |
| Start-up failure | Explained, not repaired | A damaged database stops start-up with an explanation and is left untouched. Automatic repair is Phase 7. |
| Animated page transitions | Basic | Fade-in on navigation and panel slide. Richer transitions come with real content. |

## UI redesign to the main reference (2026-11-15)

The user replaced the three references with one authoritative image, `docs/design/references/bit-not-main-ui-reference.jpg` (kept locally, not published: it is a third-party app screenshot), and asked for a close reproduction. The Phase 1 interface was restyled and restructured; no core (Rust) behaviour changed.

### Measured from the reference

The image is a 1440×900 layout rendered at 1.333×, so reference px × 0.75 = CSS px.

| Element | Reference | Implemented |
|---|---|---|
| Sidebar width | 335 px → 251 | 252 px |
| Queue width | 480 px → 360 | 360 px |
| Player bar height | 117 px → 88 | 88 px |
| Hero artwork | 283 px → 212 | 212 px |
| Hero title | ~72 px → 54 | 54 px, weight 700 |
| Play button | 75 px → 56 | 56 px |
| Track row / thumbnail | 75 / 53 px → 56 / 40 | 56 / 40 px |
| Queue row / thumbnail | 64 / 48 px → 48 / 36 | 48 / 36 px |
| Sidebar item / thumbnail | 80 / 58 px → 60 / 44 | 60 / 44 px |

### Decisions

- Typeface changed from Space Grotesk to **Inter** (bundled, SIL OFL), which matches the reference. JetBrains Mono was removed.
- Reference green is replaced by the accent (cyan by default) in the same places: play button, playing row and title, favorite hearts, active playlist. Progress and volume fills stay neutral, as in the reference.
- Cards, gradients, glass, dashed outlines, and large radii were removed. Rows use a 6 px radius, artwork 3 px.
- Favorites and Recently Played are pinned rows at the top of the sidebar list (the reference's "Liked Songs" position). Explore stays in the top navigation because the specification requires it.
- The sidebar collapse toggle moved into the title bar so the top bar matches the reference.
- Default window is 1440×932 so the area below the title bar is the reference's 1440×900.

### Sample content (development builds only)

There is still no scanner or playback engine, so the real library is empty. To check the layout against the reference, development builds can show an invented sample library: Settings → Development → Design preview. It is loaded only when `import.meta.env.DEV` is true; the release bundle was checked and does not contain it. The title bar shows a "Design preview · sample content" tag while it is on.

Real in this view: navigation, the track filter, column hiding at narrow widths. Sample-only: play/pause and favorite toggles change local state and are not saved; shuffle, previous, next, repeat do nothing.

### Remaining differences from the reference

- A 32 px title bar with the logo and Windows caption buttons sits above the layout; the reference has no window chrome.
- Three top navigation items (Home, Explore, Search) instead of two.
- No output-device pill, lyrics button, or device button: those features do not exist yet.
- Queue header has Close only (no refresh control).
- Avatar is the profile initial, not a photo.
- Neutral greys are replaced by BitNot navy surfaces.
- Screenshots: `docs/design/screenshots/`.

## Phase 2 — Local library and core playback (2026-11-16)

### Decisions

| Decision | Reason |
|---|---|
| Playback uses `rodio` 0.23 (Symphonia decoders, WASAPI output through cpal). | It handles decoding, resampling to the device format, seek, and position. The controller talks to it through an `Output` trait, so a custom pipeline can replace it in Phase 3 if EQ, crossfade, or gapless need one. |
| Tags and technical properties are read with `lofty`. | Read-only use; one library for all supported containers. |
| Subfolders are excluded by default, with an "Include subfolders" setting. | CLAUDE.md §7 says to exclude them; most real music folders are nested, so the option is offered in Settings. |
| Artwork is cached once per image, named by content hash, in one cache shared by all profiles (`%APPDATA%\app.bitnot.desktop\artwork`). | Many tracks share a cover. The window is allowed to read only this folder. |
| A scan uses its own database connection. | The interface and track changes keep working while a scan runs. |
| Opus, WMA, APE, DSD, WavPack, and MKA files are reported as skipped. | Symphonia cannot decode them; they are not listed as if playable. |
| The sample content used for the redesign review was removed. | Real data replaces it. |

### Built

- **Scanner:** tags, duration, codec, bitrate, sample rate, bit depth, channels; embedded artwork, then a cover image in the folder. Incremental by size and modified time. Missing files are marked, not deleted. An unreachable folder leaves its tracks alone. Removing a folder detaches its tracks; re-adding restores them. Scan on startup, on folder changes, and on request.
- **Library:** Songs, Albums, Artists, album/artist/playlist pages, Song Details (opens on single click), track context menu.
- **Playback:** play, pause, seek, next, previous (restarts the track after 3 s), volume, shuffle, repeat off/all/one, auto-advance. Unplayable tracks are skipped with a reason. Works without an audio device except for playback itself.
- **Queue:** Now playing / Next up, Play Next, Add to Queue, remove, clear, jump to a queued track. Saved on every change and restored paused at its position. Tracks that leave the library leave the queue.
- **Favorites, playlists** (create, rename, delete, add, remove), **history** (replay, remove one, clear), **play counts** (incremented when playback starts).
- **Search:** instant, local, exact case-insensitive matching; title, then artist, then album. Latest five searches kept.

### Verified

- 72 Rust tests and 19 frontend tests; `clippy -D warnings`, `cargo fmt --check`, and `tsc` clean.
- In the real window on Windows 11 with generated tagged WAV files (volume set to 0): scan with embedded artwork and skipped-file reporting; covers displayed; play, pause, seek, auto-advance at track end, previous; history and play counts; favorites; playlist create/add/rename/delete; queue remove and clear; shuffle and repeat; search including the no-results state; Song Details; queue restored after restart; library and queue emptied after the folder was removed.

### Not verified

- Formats other than WAV (MP3, FLAC, M4A/AAC/ALAC, Ogg Vorbis, AIFF) in real playback. Scanning and decoding use the same libraries, but only WAV was played.
- Audible output: the run used volume 0, so sound quality and the absence of stutter during a scan were not heard.
- Large libraries (thousands of tracks): no list virtualisation yet.
- Windows 10, and an audio device being unplugged during playback (Phase 5).

### Phase 2 items still open

| Item | Status |
|---|---|
| Grid/List toggle for the library | Not built. Albums, artists, and playlists are grids; songs are a list. |
| Sorting options (title, artist, duration, date added) and reverting to default | Not built. |
| Drag-and-drop reorder for playlists and queue | Not built. |
| Playlist description and custom cover; Save Queue as Playlist | Not built. |
| Remove from Library (context menu) | Not built, so it is not in the menu. |
| Metadata and artwork editing | Not built. |
| Watching folders for changes while running | Not built; scans run at startup, on folder changes, and on request. |
| Artist page sections (Popular Tracks, discography) | The artist page lists all tracks; sections are not built. |
| Artwork resizing and cache clean-up | Not built; covers are stored at original size and unused ones are not removed. |
| Quick Setup audio step | Still omitted. |

## Phase 2 completion (2026-11-16 to 2026-11-17)

Work from `docs/reports/post-phase-2-audit.md`. Full status per item is in `docs/reports/phase-2-completion-report.md`.

### Decisions

| Decision | Reason |
|---|---|
| Metadata edits are stored in a separate `track_edits` table and applied over the scanned values by the `library_tracks` view. | A rescan can never overwrite the user's edits, and the audio file is never written. |
| "Remove from Library" keeps the track row with status `removed`. | A later scan must not add the file back. Permanent deletion drops what referred to it but keeps the marker row. |
| Duplicates are recognised only when folded title, artist, and album are identical and lengths differ by at most 2 seconds. | CLAUDE.md §10 asks for merging only on confident matches. Anything else stays separate. |
| Search matches a stored folded key (lower case, accents removed, full-width and ligature forms unified, katakana mapped to hiragana). | Fast (the database narrows the candidates) and meets §14's diacritics requirement without typo correction. Romaji/pinyin transliteration is **not** implemented: no reliable offline library was found. |
| One Recycle Bin table (`trash`) for playlists, favorites, removed tracks, and earlier metadata. Profiles keep their own bin in the app database. | Uniform 90-day retention, restore, and early permanent deletion. |
| Artwork is stored as a display image (longest side at most 1200 px), a 320 px static thumbnail, and, for animated originals, the untouched file under `.anim.`. | Lists load thumbnails. Animated playback is Phase 3; only the static thumbnail is shown now. |
| New dependencies: `image` (decode/resize), `notify` (folder watching), `unicode-normalization` (folding); dev: `jsdom`, `@testing-library/react`. | Each is needed by a listed requirement. |
| List virtualisation, drag-and-drop, the menu, and the translation layer are hand-written. | Small, fixed-row-height cases; no further dependencies. |
| A development build's killed session is not reported as a crash unless it panicked. | The build tool kills the app on every rebuild. Release behaviour is unchanged. |
| The window is allowed to read the artwork folder under both its plain path and the path a file inside it resolves to. | Found while testing: AppData can be redirected (here, by the packaged parent process), which made new covers unreadable. |

### Migrations

Profile database v3 (additive): `playlists.cover`, `playlists.deleted_at`, `tracks.custom_artwork`, `tracks.search_key`, tables `track_edits`, `preferred_sources`, `trash`, `playback_position`, view `library_tracks`. Verified on the existing v2 data on this machine and by a test that upgrades a v2 database in place. The application database is unchanged.

### Verified in the running app (Windows 11, build 26200)

- v2 to v3 migration of existing data; last page restored after restart; 0 crash notices over 7 development restarts.
- Real playback through the default device at 50% volume of FLAC, AIFF, WAV, AAC (M4A), ALAC (M4A), and MP3: device peak meter 0.039 for each (expected 0.0375), no silent samples, seek correct.
- Playback during a scan: 18.13 s of wall-clock time, 18.15 s of position, no silent meter samples, while 300 files were copied into a watched folder and scanned automatically.
- Virtual list (22 of 310 rows in the page), sorting, Recently Added, grid/list toggle, artist page data, playlist description and reorder, removal of the second copy of a duplicate under a filter, metadata edit and genre validation, Remove from Library and restore, Save Queue as Playlist, Recycle Bin, queue overlay closing with Escape, thumbnails.

## Branding update (2026-11-18)

- **Name:** the product is now written **BitNot** in every place a user sees it: window title, title-bar wordmark, Quick Setup, Home, Settings and About, dialogs, notifications, error messages, README, and package descriptions. The tagline stays "Your Music. Your Way."
- **Not renamed, for compatibility:** npm package `bit-not`, Rust crate `bit-not` / `bit_not_lib`, app identifier `app.bitnot.desktop` (which also names the data folder, so existing data is found), log files `bitnot-*.log`, and the local folder name. The installer and executable take their name from `productName`, now `BitNot`.
- **Logo:** replaced by the approved mark, `docs/design/references/bitnot-logo-approved.jpg` (2048×2048 JPEG; originally supplied with a `.png` name and renamed to `.jpg` without changing its bytes). All derived assets were produced from it by separating the mark from its flat navy background and scaling it, without redrawing:
  - `docs/design/brand/bitnot-mark.png`: transparent, 1024 px.
  - `docs/design/brand/bitnot-app-icon.png`: the mark on a rounded tile in the logo's own navy (#0E1B47), 1024 px; source for the Tauri icon set.
  - `docs/design/brand/bitnot-mark-on-dark.png` and `bitnot-mark-on-light.png`: previews on the app's dark and light backgrounds.
  - `src/assets/logo.png`: transparent mark, 256 px, used in the title bar, splash, and Quick Setup.
  - `src-tauri/icons/icon.ico`: 16, 20, 24, 32, 40, 48, 64, and 256 px, each rendered separately. Sizes up to 48 px let the mark fill more of the tile so it stays readable; its shape is unchanged.
- **Transparency:** the note and inner wave lines are cut-outs in the approved design, so in the transparent mark they show whatever is behind. On light backgrounds they appear light.
- **Old logo:** `logo-1024.png` and `logo-source.webp` were moved to `docs/design/brand/archive/`, which is excluded from publication locally.
