# BitNot — Phase 2 Completion Report

Date: 2026-11-17. Follows `docs/reports/post-phase-2-audit.md`.

## Summary

- Every item listed as unfinished in the audit now has an implementation and automated tests, with four exceptions that are limited or blocked and are named below.
- The three audit defects are fixed, each with a regression test.
- All checks pass: 113 Rust tests, 55 frontend tests, type check, production build, lint, and format.
- Real playback of six formats was measured on the output device. **Ogg Vorbis was not tested**: no Vorbis file or encoder was available on this machine.
- "Verified in the app" below means it was exercised in the real window on Windows 11. Several dialogs were driven through their commands rather than clicked through by hand; those are listed under "Manual testing still required".
- Nothing was committed. No Phase 3 feature was built.

## Status of every previously unfinished item

Status key: **Done** = implemented, tested, and exercised in the app. **Done (tests only)** = implemented and covered by automated tests, not exercised by hand in the app. **Limited** = works within a stated limit. **Blocked** = not possible with what is available.

### A. Playlists and queue

| Item | Status | Evidence |
|---|---|---|
| Playlist descriptions | Done | Core test; shown on the playlist page in the app |
| Custom playlist covers | Done (tests only) | Core and artwork tests; the image picker was not operated by hand |
| Drag-and-drop playlist reordering | Done (tests only) | Core test, component test of the drag events; reorder verified in the app through the command, and "Move up / Move down" exist for the keyboard. A real mouse drag was not performed |
| Drag-and-drop queue reordering | Done (tests only) | Queue and controller tests; same note about a real mouse drag |
| Save Queue as Playlist | Done | Core test; verified in the app (order kept) |
| Order preserved after restart | Done | Playlist order is stored; queue restore tested and seen in the app |

### B. Library and track management

| Item | Status | Evidence |
|---|---|---|
| Grid/List view toggle | Done | Verified in the app; the choice is saved per profile |
| Recently Added view | Done | Core test; verified in the app |
| Quality column | Done (tests only) | Component test. It appears only when the centre column is at least 62rem wide (for example with the queue closed); not looked at in the app |
| Sorting by title, artist, duration, date added | Done | Unit and component tests; header sorting verified in the app |
| Restore default sorting | Done | Tests; verified in the app |
| Favorites sorting | Done (tests only) | Uses the same sortable list as Songs, with album and date added |
| Remove from Library | Done | Core and scanner tests; verified in the app with restore |
| Removal never touches the audio file | Done | Tests assert the file still exists and a scan does not re-add it |

### C. Artist pages

| Item | Status | Evidence |
|---|---|---|
| Popular Tracks from local play counts | Done | Core test; data verified in the app |
| Album discography, newest first | Done | Core test; data verified in the app |
| Popular Tracks hidden without history | Done (tests only) | Core test |

The artist page's data was checked through the core in the running app; the page itself was not looked at.

### D. Scanning and artwork

| Item | Status | Evidence |
|---|---|---|
| Monitor music folders while running | Done | Watcher tests; in the app, 300 copied files were added with no manual scan |
| Detect added, modified, moved, removed files | Done | Scanner tests, including a moved-and-renamed file keeping its favorites and play count, and a copy not being mistaken for a move |
| Artwork resizing and thumbnails | Done | Artwork tests; 320 px thumbnails seen loading in the app |
| Artwork cache clean-up | Done (tests only) | Tests. It runs after each scan and only removes files older than an hour that no profile refers to, so it was not observed in the app |
| Manual artwork upload | Done (tests only) | Core and artwork tests; the image picker was not operated by hand |
| Preparation for animated artwork | Done | Animated originals are kept untouched under an `.anim.` name with a static thumbnail. Nothing is animated yet |

### E. Metadata and organisation

| Item | Status | Evidence |
|---|---|---|
| Metadata editing (title, artist, album, genre, year, track number) | Done | Core and scanner tests; verified in the app through the command. The edit dialog was not typed into by hand |
| Predefined genre list | Done | 79 genres; a genre outside the list is refused (verified in the app) |
| Duplicate detection | Done | Core test; in the app, five files of one recording showed as one track with five versions |
| Source selection | Done (tests only) | Preferred-version choice and the versions list in Song Details; playback falls back to another copy when a file is missing (controller test) |
| Original files preserved | Done | Edits live in their own table; tests confirm a rescan keeps edits and files are byte-identical after scanning |
| Duplicates and playlist references | Done | Playlists keep exact entries; tests cover both |

### F. Search

| Item | Status | Evidence |
|---|---|---|
| Diacritics and character variants | Done | Tests: accents, case, ß, æ, ø, ł, full-width forms |
| Multilingual support | **Limited** | Katakana and hiragana find each other. **Transliteration (romaji, pinyin, romanised Korean) is not implemented**: it needs a reliable offline dictionary, and none was found |
| Exact-match and ranking kept | Done | Tests: title, then artist, then album; typos are not guessed |

### G. Quick Setup and recovery

| Item | Status | Evidence |
|---|---|---|
| Audio preferences step | **Limited** | The step sets the starting volume. That is the only audio preference that exists before the Phase 3 equalizer and effects. Not viewed in the app, because setup was already complete on this machine |
| Recycle Bin for playlists, favorites, metadata | Done | Core tests; verified in the app |
| Early permanent deletion | Done | Tests; a deleted profile was permanently deleted in the app |
| Last-page restoration | Done | Router tests; verified in the app after a restart |
| Automatic database repair | Done (tests only) | Tests damage a real database file and confirm the rebuild, the kept original, and refusal when repair is unsafe. Not triggered in the app |
| Never overwrite the only valid database | Done | The damaged file is renamed aside and kept byte for byte (tested) |

### H. Accessibility and localisation

| Item | Status | Evidence |
|---|---|---|
| Keyboard access to track rows and menus | Done (tests only) | Component tests: arrows, Home/End, Enter, Space, Menu key, Shift+F10; menu navigation and focus return |
| Screen-reader compatibility | **Limited** | Roles, labels, sort state, and row hints were added. **No screen reader was run** |
| Contrast and focus indicators | Done | Faint text failed WCAG AA in three places (4.39, 4.13, 3.81); the colours were changed and a test now checks the real stylesheet |
| Translation system | **Limited** | The system exists, with typed keys, plurals, and fallback to English. Navigation, library, track table, menus, player, queue, dialogs, and the Recycle Bin use it. **Settings, Home, Quick Setup, Explore, Search, and history text is still inline English** |
| English default, ready for more languages | Done | A language is one file plus one registration; tested with a second catalog |

### I. Responsive layout

| Item | Status | Evidence |
|---|---|---|
| Queue on narrower windows | Done | As an overlay it takes focus, closes with Escape or a click outside, and stays within the window. Verified in the app at 1000 px wide |
| Desktop layout and identity preserved | Done | No layout dimensions or colours changed except the two faint-text colours |

## Bugs fixed

| # | Defect | Fix | Regression test |
|---|---|---|---|
| 1 | Second copy of a track in a filtered playlist acted on the first copy | Every row carries its own stored position; nothing looks rows up by track any more | `buildRows` unit test, track-table component test, core test; confirmed in the app |
| 2 | "Add all to Queue" sent one request and one notification per track | One batch command and one notification | Data-layer test asserts one call and one message |
| 3 | Development rebuilds caused a false "did not close properly" notice | A killed development session is not a crash unless it panicked; release builds unchanged | Diagnostics test; 0 notices over 7 restarts in the app |
| New | Covers could not be displayed when AppData was redirected | The artwork folder is allowed under the path its files actually resolve to | Found and confirmed in the app |

## Database migrations

One: **profile database v3**, additive only. New columns `playlists.cover`, `playlists.deleted_at`, `tracks.custom_artwork`, `tracks.search_key`; new tables `track_edits`, `preferred_sources`, `trash`, `playback_position`; new view `library_tracks`. It ran on the existing data on this machine without loss, and a test upgrades a v2 database in place. The application database is unchanged.

## Performance improvements

| Concern from the audit | Change | Evidence |
|---|---|---|
| Lists render every row | Lists over 80 rows render only rows near the viewport | 22 of 310 rows in the page; component test with 2,000 rows |
| Every change reloads whole lists | Change events name the part that changed; favorites update locally | Tests show a favorite or history change does not reload the track list or the overview |
| Search reads the whole table per keystroke | The database filters on a stored folded key; only matches are loaded | Core tests. Note: the filter is still a scan of that one column, not an index lookup |
| Full-size artwork in lists | 320 px thumbnails; display images capped at 1200 px | Artwork tests; seen in the app |
| Folder covers re-read per track | Looked up and hashed once per folder per scan | Scanner test |
| Volume slider writes on every step | The engine changes at once; settings are written 400 ms after the slider rests | Data-layer test: 4 moves, 1 write |
| Queue rewritten on every change | The queue is written only when it changed; position is one small row, also saved every 5 seconds while playing | Controller test |

Not measured: behaviour with tens of thousands of tracks.

## Audio engine validation

Played through the default Windows output device at 50% volume. "Meter" is the device's peak meter; the test tone should read about 0.0375.

| Format | Result | Meter | Silent samples | Seek |
|---|---|---|---|---|
| FLAC (24-bit / 48 kHz) | Plays | 0.0392 | 0 of 64 | Correct |
| AIFF (16-bit / 44.1 kHz) | Plays | 0.0392 | 0 of 64 | Correct |
| WAV (16-bit / 44.1 kHz) | Plays | 0.0392 | 0 of 64 | Correct |
| AAC in M4A | Plays | 0.0395 | 0 of 64 | Correct |
| ALAC in M4A | Plays | 0.0391 | 0 of 65 | Correct |
| MP3 | Plays | 0.0393 | 0 of 64 | Correct |
| Ogg Vorbis | **Not tested** | n/a | n/a | n/a |

- **During a scan:** over 18.13 s of wall-clock time the position advanced 18.15 s, with 0 silent samples of 289, while 300 files were copied and scanned.
- **Paused:** meter 0.0000.
- **What this does and does not show:** the signal reached the device at the expected level without gaps. Nobody listened, so audible quality (distortion, clicks at seek) is not confirmed. One device, one sample-rate pair.
- **Unsupported, reported to the user as skipped:** Opus, WMA, APE, DSD, WavPack, MKA.
- **Decoder limits known so far:** the output device is opened once at start-up, so a device change needs a restart; there is no gapless or crossfade path yet (Phase 3).

The audio architecture was not changed.

## Test results

| Check | Result |
|---|---|
| `cargo test` | 113 passed, 0 failed (was 72) |
| Vitest | 55 passed, 0 failed, 7 files (was 19 tests, 3 files) |
| `tsc --noEmit` | Pass |
| Production build | Pass: 340 kB JS (102 kB gzip), 30 kB CSS |
| `cargo clippy --all-targets -- -D warnings` | Clean |
| `cargo fmt --check` | Clean |

Frontend tests now include component and interaction tests (track table, context menu and confirmations, data layer). No existing test was removed. One search test's expectation was changed on purpose: searching "under" now finds "Ünder", as §14 requires.

## Original music files

Preserved. The scanner, artwork import, metadata editing, Remove from Library, the Recycle Bin, and database repair never write to music files or the user's image files. Tests assert files are byte-identical after scanning and still present after removal. The validation used generated files in a temporary folder only.

## Remaining limitations and unverified behaviour

- Ogg Vorbis playback is untested.
- No screen reader was run; no keyboard-only walkthrough was done by hand.
- Search has no romaji or pinyin transliteration.
- About half of the interface text is not yet routed through the translation system.
- The Quick Setup sound step offers only volume.
- Windows 10 is untested. The installer has not been built. CI has never run.
- Folder monitoring was verified on a local NTFS folder only, not on network or removable drives.
- Light theme was not looked at after these changes (its contrast is covered by the new test).
- When this app is started from inside the packaged Claude desktop app, Windows redirects its AppData. Data created during these verification runs may therefore not be what a normal launch sees. The verification profile was deleted afterwards either way.

## Manual testing still required

1. Play an Ogg Vorbis file.
2. Listen to each format with sound on, including a seek.
3. Drag a playlist row and a queue row with the mouse.
4. Choose a cover image for a playlist and for an album through the file picker.
5. Type into the Edit Details dialog and save; reset to file tags.
6. Open a duplicated track's Song Details and switch the preferred version.
7. Run Quick Setup from a fresh profile and check the sound step.
8. Close the queue and check the Quality column at a wide window.
9. Open an artist page with and without listening history.
10. Narrator pass and a keyboard-only pass over the main views.
11. A real library of several thousand tracks: scan time, scrolling, search.

## Project status and recommendations for Phase 3

Phase 2 is functionally complete, with the limits above. Before starting Phase 3:

1. **Commit and push.** The repository still has no commits; that remains the largest risk.
2. **Do the manual checks**, at least items 1 to 3.
3. **Run the audio pipeline proof of concept** (one EQ band, a crossfade between two files, a sample tap) to decide between extending `rodio` and a custom pipeline. Six formats now have known-good playback to compare against.

Then, in order: keyboard shortcuts, Now Playing page, sleep timer; gapless, equalizer, crossfade, visualizer on the chosen pipeline; Mini Player (needs playback changes broadcast to all windows); lyrics after a source licensing review; the accessibility pass. Finish moving the remaining interface text into the translation system alongside that work.
