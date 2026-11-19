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

## Phase 3 — Premium player experience (started 2026-11-18)

Assessment, risks, sequence, and the audio-pipeline proof of concept plan: `docs/reports/phase-3-assessment.md`.

### Milestone 1, step 1: keyboard shortcuts

- **Mapping** (`src/lib/shortcuts.ts`): the fixed set from the specification (Space, Ctrl+→/←, Ctrl+↑/↓, Ctrl+F) plus three additions that need no new product decision: Ctrl+Shift+→/← seek ±10 s, Ctrl+M mute, F1 or ? show the shortcut list. Volume steps by 5. Only volume and seek repeat while a key is held.
- **Not hijacking:** while typing in a text field only Ctrl+F (and F1) act. Space on a focused button, link, row, or other control keeps its native meaning. Nothing acts while a dialog or menu is open, or when another handler already handled the key. Alt and Windows-key combinations are left to the system.
- **Mute:** sets the engine volume to 0 without changing the saved volume; any volume change unmutes. Mute is not persisted (a restart starts unmuted).
- **Discoverability:** player-bar tooltips show the shortcut (e.g. "Next (Ctrl+→)"); the volume icon is a mute button; the list is in a dialog (F1/?) and in Settings → Keyboard shortcuts.
- **Scope:** shortcuts work while the BitNot window has focus. System-wide media keys belong to Phase 5 (Windows Media Session).
- **Verified** in the running app with real key events: play/pause, next/previous (no action past the end of the queue), seek ±10 s, volume up/down with the saved setting following, mute and unmute through Ctrl+↑, Ctrl+F focus, Space and letters typed into search without toggling playback, F1 and ? open the dialog, Escape closes it.
- **Not yet verified:** a physical keyboard pass and a Narrator pass.

### Milestone 1, step 2: Now Playing

- **Page** (`src/pages/NowPlaying.tsx`, route `nowPlaying`): full-size artwork, title, links to the artist and album pages, the source's format line, a Favorites toggle, Song details (technical information stays there), seek bar, transport (shuffle, previous, play/pause, next, repeat), and volume. With nothing loaded it shows an empty state with a Browse your library button.
- **Shared controls** (`src/components/PlayerControls.tsx`): the player bar and Now Playing use the same seek bar, transport, and volume components, so they cannot drift apart.
- **Layouts:** Centered (artwork in the middle, everything below) and Split (artwork left; details, controls, and the next five tracks of the queue on the right, then "and N more"). Below 1100 px window width Split stacks into one centred column. The choice is a per-profile setting, `nowPlayingLayout` (`centered` by default), switchable on the page and in Settings → Appearance; the core rejects any other value.
- **Opening it:** the current track in the player bar is a button (Open Now Playing / Close Now Playing, with `aria-pressed`). Closing goes back to the previous page, or Home when Now Playing was restored with no history behind it. The page is restored after a restart like any other page.
- **Motion:** artwork fades in on track change using the shared duration tokens, so Reduce Motion turns it off.
- **Defect fixed (from Phase 1):** the last page was compared with the value loaded at start, so returning to that page later in the session (e.g. Now Playing → artist → Now Playing) was not saved and a restart opened the wrong page. It is now compared with the last value written, per profile. Covered by `src/state/AppContext.test.tsx`, which fails without the fix.
- **Verified in the running app:** both layouts with and without artwork; Next Up listing and "and 1 more" with a 7-track queue; "Nothing is queued after this track"; favourite toggle syncing with the player-bar heart; Song details; artist link; open/close from the player bar returning to the previous page; Space toggling playback on the page; stacked Split at 1000 px with the queue as an overlay; layout and page restored after reload.
- **Not yet verified:** a Narrator pass over the page and the player-bar button.

### Milestone 1, step 3: Sleep timer

- **Choices:** 15, 30, 45, or 60 minutes, and Stop after current track (an addition to CLAUDE.md §20, approved 2026-11-18). Picking a choice replaces the one set before. Turn off sleep timer cancels.
- **Where it lives:** `src-tauri/src/sleep.rs`, owned by the playback controller. A fixed length is an end time on `std::time::Instant`, which is monotonic, so changing the Windows clock or time zone cannot move it. The existing 250 ms playback ticker checks it with one comparison, so no thread, timer, or polling was added. The core rejects any other length.
- **When it runs out:**
  - Playback pauses; the track and position stay. Taking the end time and disarming the timer happen in one step, so it fires once; music started again afterwards keeps playing.
  - It counts real time, including while paused. If it runs out while paused, it is simply used up.
  - Seeking and skipping do not change it.
  - The window shows "Sleep timer ended. Playback is paused." (event `playback:sleep-ended`).
- **Stop after current track:**
  - It waits for the player's own "finished" event, not a time computed from the track length, so seeking, pausing, and skipping cannot make it fire early.
  - When the song ends, the next song is not started. The queue still moves to it, following repeat and shuffle (Repeat One leaves the same song ready, Repeat All wraps to the first, and at the end of the queue it stays on the last song). Play then continues where the queue would have gone. The next song is not counted as played until it actually starts.
  - It applies to whichever song is playing when one ends, so after a manual skip it stops after the new song.
  - It needs a song in the queue, and the menu item is disabled otherwise.
- **Not kept across a restart.** CLAUDE.md does not ask for it, and a timer that silently came back could pause music started on purpose in a new session. A profile switch also cancels it. Shutdown needs nothing extra, because the timer is only in memory.
- **Interface:**
  - A moon button in the player bar opens the choices as radio menu items, with the active one checked; it is keyboard-operable and returns focus to the button.
  - While a timer is set, the moon turns the accent colour and shows the time left (m:ss, rounded up) or "End of track". The button's name says the same, e.g. "Sleep timer: 14:41 left".
  - The countdown starts from the time left that the core reports with every playback state and runs on the browser's monotonic clock, updating once a second only while a fixed-length timer is set. A window reload picks it up again from the core.
  - Setting, cancelling, and expiry are confirmed with a message.
- **Menu component:** items may now be marked `checked`, which renders them as `menuitemradio` with `aria-checked` and a check mark.
- **Tests:**
  - Rust (17 new): each length expires at its end time and not 1 ms before; it fires once; wall-clock jumps have no effect; pause and resume; cancel; seek and skip; stop-after-track including Repeat One, Repeat All, repeat off at the end of the queue, and shuffle; manual skip; no track and no audio device; restart, profile switch, and reset.
  - Frontend (8 new): labels, rounding, the countdown and its resynchronisation, the menu's choices and checked state, the disabled stop-after-track item without a track, cancel, and radio menu items.
- **Verified in the running app:**
  - The menu opened and Stop after current track was chosen with the keyboard; focus went back to the button.
  - Stop after current track let Harbor Lights finish, did not start Night Bus, and left it ready at 0:00 with its play count unchanged.
  - A 15-minute timer counted down in the player bar.
  - A window reload resynchronised the countdown.
  - It kept counting real time through a 20-second pause.
  - It looked right in both themes.
  - A full 15-minute run (set at 17:29:23, with Repeat All on) had paused by the first check after the end time (17:44:28; checks every 5 s). The position was kept at 0:05 in the song, the message appeared, and playback stayed paused.
- **Limits:**
  - It pauses at once, without a fade (a fade needs the audio pipeline work in the proof of concept).
  - It fires within one ticker interval (at most 250 ms late).
  - If the computer sleeps, Windows may not advance the monotonic clock while suspended, so the timer can end later than the wall-clock time suggests. No music plays during suspend in any case.

### Milestone 1, step 4: Dated playback clock

- **Why:** the position reached the window only with each progress event (every 250 ms), so the seek bar moved in visible jumps. Synchronised lyrics and the visualizer will also need a position between events.
- **Core:**
  - Every `playback:progress` event and every playback state now carries `clockMs`: the core's monotonic time (`std::time::Instant` since the first reading in the process, `monotonic_ms` in `playback.rs`) when the position was read. Wall-clock time is not used.
  - Existing fields are unchanged; `clockMs` was only added. The core stays the authority, and nothing about queue, repeat, shuffle, the sleep timer, or audio output changed.
- **Window** (`src/lib/playbackClock.ts`, no React inside):
  - **Lining up the clocks:** it learns how the core's monotonic time lines up with `performance.now()`, taking the smallest difference seen (each reading arrives late by its delivery time, never early). That figure may grow by at most 1 ms per second, to allow for slow drift between the two clocks.
  - **Estimating:** while playing, the position is the reading plus the time since it was taken. A late or irregular reading therefore adds no error, and nothing accumulates.
  - **Limits:** the estimate stops at the track's duration, and at 1 s past the newest reading if readings stop arriving.
  - **Paused or stopped:** it shows the reading unchanged.
  - **Snapping:** a seek (disagreement over 300 ms), a track change, a pause, or a stop is shown at once. A smaller backwards correction is absorbed by holding the shown position until the estimate passes it, so the bar never twitches backwards.
  - **Out-of-order readings:** a reading older than one already applied is ignored, for example a progress event delivered after the result of a seek.
- **Rendering:**
  - The clock lives in its own React context and is never replaced. `usePosition(granularity)` re-renders a component only when the position enters a new step.
  - The seek bar uses about a thousand steps per track (16 ms to 1 s), so it moves by under a pixel at a time without re-rendering on every frame. A 3-minute track re-renders it about 5 times a second.
  - Animation frames run only while playing, while a seek bar is mounted, while the window is visible, and while the estimate can still move.
  - With Reduce Motion, the bar steps once a second.
- **Seek bar:** the slider now has 1 ms steps, so its thumb (shown on hover and focus) follows the smooth fill. Arrow keys still move 1 s and Page Up/Down 10 s, as before; Ctrl/Shift arrows remain app shortcuts. The player bar and Now Playing use the same component and the same clock, so they always show the same position.
- **Tests:** 1 Rust (dated readings for play, pause, track change, stop, and state) and 18 frontend:
  - `playbackClock.test.ts` covers moving while playing, holding while paused, resuming, seeks both ways, absorbing small corrections, track and duration changes, stopping, late and irregular readings without drift (50 rounds, error at most 5 ms), ignoring stale readings, the duration and look-ahead caps, step-based re-rendering, two subscribers in step, frames only when needed, and the sleep-timer pause.
  - `SeekBar.test.tsx` covers two bars sharing one clock, dragging while it runs, keyboard steps, and Reduce Motion.
  - `LibraryContext.test.tsx` covers events feeding the clock and relative seeking.
- **Verified in the running app** (8-second test tracks):
  - Sampling both bars on every frame for 1.5 s gave 95 distinct positions, none backwards, the two bars identical on every frame, and the largest step 28 ms.
  - Against fresh readings from the core, the shown position was within 20 ms, including the delivery delay.
  - Pausing froze both bars at the core's exact position; resuming moved on.
  - Seeking with the arrow keys on the bar and with Ctrl+Shift+Right snapped at once.
  - A track change reset both bars and the Now Playing title.
  - Stop after current track still stopped on time, with the bars frozen at 0.
- **Limits:**
  - The position is where the decoder is, which runs ahead of the speakers by the output buffer (tens of milliseconds). That is not corrected here; it matters for lyrics and the visualizer and is noted for the proof of concept.
  - Playback speed is assumed to be 1×, as BitNot has no speed control.

### Audio pipeline proof of concept (2026-11-19)

Full results, method, environment, and recommendation: `docs/reports/phase-3-assessment.md`, section 9.

- **What was built:** a standalone example, `src-tauri/examples/audio_poc/`, not wired into the app. The production engine, the `Output` trait, and existing tests are unchanged, and no dependency was added (cpal is used through rodio's re-export).
  - It generates its own deterministic test audio (WAV, AIFF, and FLAC through a small verbatim writer, since no encoder is installed).
  - It runs seven experiments: gapless, EQ, crossfade, capture, CPU, rodio on the device, and direct cpal.
- **Decision: keep rodio and extend it.**
  - Gapless joins are bit-identical when the next track is appended early; this held on the real device too.
  - Flat EQ and an idle limiter are bit-exact.
  - The equal-power crossfade holds level within the material's own variation, and the handover point is exact to the frame.
  - The visualizer ring never blocks the audio side.
  - With all features running: 8 ms of CPU per second of audio, and 0 missed callbacks in 3 × 20 s on the device.
- **Defects found in the current engine** (not fixed; awaiting approval):
  1. rodio's 44.1 → 48 kHz conversion is linear interpolation and restarts at every decoder packet: THD+N −34.5 dB at 1 kHz, −7.4 dB at 16 kHz, timing steps of a few µs, and 10 extra frames per 2 s. Feeding rodio audio already at the device rate avoids it bit-exactly; a windowed-sinc stage in the PoC reached the file's own noise floor (−92 dB) for about 2 ms of CPU per second.
  2. Because `DeviceOutput` connects its `Player` before anything is queued, the first track after start-up plays its first 10.7 ms as mono at half speed and runs 5.3 ms late. The fix is to connect after the first append.
- **EQ design facts:** RBJ biquads and Simper SVFs have identical static response. Under live gain changes the SVF is much cleaner (−54 dB click level at a 30 ms glide with per-frame updates, against −11.5 dB for the biquad), so the production EQ should use SVFs. Neighbouring bands interact; with all bands at −12 dB the middle measures −18.3 dB.
- **Not covered:**
  - MP3/AAC encoder-delay gapless (no encoder available).
  - Listening tests (device runs are muted).
  - Other devices, Windows 10, and exclusive mode.
  - Output latency through rodio (not exposed; cpal reports 31.8 ms on this device).
- **Reproduce:** `cargo run --release --example audio_poc -- all`, from `src-tauri/`.

### Engine defect fixes (2026-11-19)

Details, measurements, and corrections to the proof-of-concept record: `docs/reports/phase-3-assessment.md`, section 10.

- **Fix:** a new output stage, `src-tauri/src/resample.rs`, sits between the rodio Player's queue and the device mixer. `DeviceOutput::on_mixer` creates the Player unconnected and adds only the stage to the mixer.
  - The stage always presents stereo at the device rate, read from the stream that was opened (`sink.config()`, now logged). The mixer therefore never resamples and never reads the empty queue's mono format.
  - This fixes the first-track glitch (the first 10.7 ms as mono at half speed, then 5.3 ms late) and rodio's linear-interpolation resampling (THD+N −34.5 dB at 1 kHz, −7.4 dB at 16 kHz).
- **Resampler:**
  - In-house windowed sinc: 96 taps at the lower rate, Kaiser β 9, cut-off 0.97, a table of 512 entries per sample.
  - The read position is an exact fraction, so there is no drift.
  - It runs on the continuous queue output, and same-rate audio is passed through bit-exact.
  - `rubato` (MIT) was considered and not added: several extra crates, and the streaming and boundary handling would still be needed.
  - The PoC's 32-tap version was replaced after measuring its response (−5.7 dB at 20 kHz). The production filter is flat to 20 kHz, with 91.5 dB of alias suppression.
- **Track boundaries:** rodio's queue reports the wrong format for the first samples of a new track (decoders never report themselves finished). Each appended track is now wrapped in `Marked`, which records its format on its first sample, so the stage switches rate and channel count on the exact sample.
- **Results:**
  - 44.1 → 48 kHz: THD+N −97.5 to −116 dB from 1 to 20 kHz, gain within 0.001 dB.
  - 10 s gives exactly 480 000 frames, with no drift.
  - Track joins at the same rate are continuous; a 48 kHz track after a 44.1 kHz one is bit-exact.
  - Cost: 12.7 ms of CPU per second of audio (release).
- **Tests:** 13 new (10 for the stage, 3 driving the real `DeviceOutput` on an offline mixer, including one that reproduces the original rodio defect). 145 Rust tests pass over 5 runs.
- **On the device** (WASAPI loopback, with another application playing throughout):
  - The first 44.1 kHz track after start-up played at 996.9998 Hz with 0.02 dB coherence loss, against 996.9026 Hz and 7.81 dB for the old path on the same file. 48 kHz was exact.
  - Volume changes, seek, pause and resume, and 44.1 ↔ 48 kHz track changes worked in the app.
  - No listening test was performed.
- **Not done:** gapless, EQ, crossfade, visualizer (unchanged scope); hardware tests at other device rates or channel counts; lossy formats measured through the stage.

### Gapless playback (2026-11-19)

Details, measurements, and limitations: `docs/reports/phase-3-assessment.md`, section 11.

- **New `src-tauri/src/gapless.rs`:**
  - A `TrackChain` is the one source the Player gets per load. When the current decoder runs dry, it continues with the prepared next track on the audio thread, with no polling, decoder opening, or source replacement at the boundary.
  - Finished tracks are dropped there, so at most two tracks are held.
  - Preparations are tagged with the transition count, so a stale one is never played.
  - Position is counted per track.
- **Controller (`playback.rs`):**
  - `sync_preload` prepares the track automatic advance would play (a copy of the queue advanced with `advance(true)`: repeat, shuffle, and the end of the queue as before), after every command and tick. It skips the preparation when "stop after this track" is set, and does not retry a track that failed.
  - `catch_up` advances the queue, makes the prepared track current, and counts its play when the chain reports a transition, never at preparation.
  - `core.rs` runs `catch_up` before each command and `sync_preload` after.
  - The `Output` trait gained `prepare_next` and `transitions`.
- **Output stage (`resample.rs`) at a change of rate:**
  - Overlapping contributions of neighbouring segments with a common band limit, grids joined at their half-sample edges, and the exact sub-sample position carried across.
  - A track at the output rate glides onto whole frames over 0.1 s and is then bit-exact again.
  - Deviation of a continuous tone at the join fell from 0.33 to about 0.005 (of 0.5); length exact to the frame.
  - The kernel table is now built when the output is created (it cost about 2 ms on the audio thread at first use).
- **Tests:** 20 new (5 chain, 2 rate-change, 9 controller with the fake output, 4 on the real `DeviceOutput`) plus 1 ignored cost measurement. 163 Rust tests pass over 3 runs; frontend unchanged (109).
- **Device** (WASAPI loopback, other audio playing):
  - Six phase-continuous pieces at 44.1/48 kHz played through five joins as one tone: 996.9999 Hz, 0.01 dB coherence loss, largest phase step between seconds 7.7 µs.
  - Each piece became current in order and was counted once.
  - Seek-near-end, Next, Previous, reorder, and pause worked.
  - No listening test was performed.
- **Cost:** preparing a track 0.13–0.28 ms; about 86–89 KB per prepared track; the slowest audio-thread sample at a switch 316 µs (of a 10 ms callback).
- **Not measured:** MP3/AAC encoder-delay trimming (no encoder); the old engine's gap on the device; other devices and Windows 10.
