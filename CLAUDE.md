# BitNot --- Project Specification & Claude Code Instructions

> **Product name:** BitNot\
> **Tagline:** Your Music. Your Way.\
> **Document status:** Approved planning baseline\
> **Initial platform:** Windows desktop\
> **License:** Apache License 2.0\
> **Repository:** Public GitHub repository\
> **Product direction:** Local-first music ecosystem with legally
> sourced online music

This document is the source of truth for the first Windows
implementation of BitNot. It consolidates the decisions made during
product planning. If an implementation detail is technically impossible,
legally restricted, unavailable through a provider, or unsupported by
the target Windows version, do not fake the feature. Explain the
limitation, implement a safe fallback, and document the deviation.

------------------------------------------------------------------------

# PART I --- PRODUCT VISION

## 1. Product overview

BitNot is a premium desktop music application that combines:

-   A local music library and local audio playback.
-   Discovery and playback from legally accessible online music
    catalogs.
-   Playlists, favorites, listening history, queue, artist and album
    pages.
-   Synchronized lyrics from permitted timestamped sources.
-   Advanced audio controls and Windows media integration.
-   Optional local-network synchronization between trusted devices.
-   A polished, responsive desktop interface.

BitNot is **local-first**. The application must remain useful without
an account, private server, paid cloud database, or mandatory
subscription. Local music playback and local library management must
continue to work offline.

The first release targets Windows. Android and iOS are future platforms
and must not delay or compromise the Windows release. Do not implement
mobile-specific architecture prematurely, but avoid unnecessary platform
lock-in where reasonable.

## 2. Non-negotiable product principles

1.  **Local music always belongs to the user.** Never move, modify,
    overwrite, or delete original music files without explicit user
    action.
2.  **No mandatory private backend.** Core functionality must not depend
    on a BitNot-operated VPS, cloud database, or paid hosting.
3.  **No unauthorized music access.** Do not scrape, bypass DRM, defeat
    access controls, or use private/undocumented APIs to access
    commercial catalogs.
4.  **Provider terms matter.** Publicly reachable audio is not
    automatically free to stream, cache, download, redistribute, or use
    commercially.
5.  **Privacy by default.** Do not upload local audio, personal library
    contents, or diagnostic logs automatically.
6.  **Graceful degradation.** If an online provider, codec, Windows API,
    or device is unavailable, explain the limitation and preserve local
    functionality.
7.  **No fake functionality.** Do not ship buttons or settings that look
    functional but do nothing.
8.  **Performance matters.** Animated artwork, scanning, indexing,
    synchronization, and visual effects must not make playback stutter.
9.  **Windows-first delivery.** Finish, test, and stabilize the Windows
    application before planning Android/iOS implementation.
10. **Keep the UI premium and coherent.** Native-looking dialogs or
    default controls must not visually clash with BitNot's design.

------------------------------------------------------------------------

# PART II --- BRAND AND USER EXPERIENCE

## 3. Branding

-   Product name: **BitNot**
-   Tagline: **Your Music. Your Way.**
-   Product name is written as one word, **BitNot**, everywhere it is
    shown (decided 2026-11-18). Technical identifiers keep their existing
    form for compatibility: package `bit-not`, crate `bit_not_lib`,
    app identifier and data folder `app.bitnot.desktop`.
-   Logo: the approved mark, a capital B integrated with a single musical
    note, with curved sound-wave accents, in an electric cyan/blue gradient
    on deep navy. Approved original:
    `docs/design/references/bitnot-logo-approved.jpg` (never edit it).
    Decided 2026-11-18; it replaces the earlier music-note logo.
-   Derived assets in `docs/design/brand/`: `bitnot-mark.png`
    (transparent), `bitnot-app-icon.png` (mark on its navy tile),
    `bitnot-mark-on-dark.png`, `bitnot-mark-on-light.png`. Windows icons in
    `src-tauri/icons/`; in-app mark in `src/assets/logo.png`.
-   Full wordmark: the mark plus BitNot.
-   App icon: the mark on its navy tile.
-   Accent color: Cyan Blue `#36C5F0`.
-   Main background: Deep Navy `#0B111B`.
-   Secondary surface: `#172333`.
-   Primary text: `#F3F7FA`.
-   Typography: Inter (bundled), chosen 2026-11-15 to match the visual
    reference.
-   Provide suitable dark/light brand assets and Windows application
    icon assets.

Do not invent a final logo by using plain text initials. Create a
coherent brand system and keep icon, splash, sidebar, installer, and
application branding consistent.

## 4. Official visual reference

Updated 2026-11-15: one image is the primary and authoritative visual reference for the interface. It is a screenshot of a third-party music app, so it is kept locally (`docs/design/references/bit-not-main-ui-reference.jpg`) and **not published** in the public repository. Its measured dimensions are recorded in `docs/implementation-log.md`, and the implemented interface (`docs/design/screenshots/`) is the working reference for contributors.

It shows the playlist page: left sidebar, centre playlist and track table, right Queue panel, and the persistent bottom playback bar. Reproduce its layout, proportions, spacing, typography hierarchy, and component placement as closely as technically possible; the playlist page gets the closest reproduction and every other page reuses the same sidebar, top bar, typography, spacing, controls, and player bar. It replaces the earlier three-image set.

Rules for applying it:

- BitNot brand colours replace the reference's neutrals and green: background `#0B111B`, secondary surface `#172333`, accent `#36C5F0`, text `#F3F7FA`. The accent goes exactly where the reference uses green.
- No unnecessary gradients, glass effects, oversized rounded cards, heavy shadows, or decorative elements. Pages must not look like collections of unrelated cards.
- Do not copy third-party logos or proprietary assets. Keep the BitNot logo as supplied.
- Measured dimensions, the typeface decision (Inter), and the remaining differences are recorded in `docs/implementation-log.md`.
- The sidebar collapses to an icon rail on narrow windows; the queue becomes an overlay; track lists drop Date Added, then Album, as the column narrows.
- Now Playing, Fullscreen Lyrics, and Mini Player have no dedicated reference image any more; design them in this same visual language when their phase arrives.

If the reference file is needed and not present locally, ask the user for it; do not pretend to have inspected an image that is unavailable.

## 5. Main desktop layout

The supplied desktop music-player reference image is the primary layout
reference. Reproduce its overall structure and proportions as closely as
practical while using BitNot branding. Do not replace it with a generic
admin dashboard.

Main layout:

-   Left sidebar: navigation, playlists, collections.
-   Top area: search and relevant page controls.
-   Center: Home, library, album, artist, playlist, search results, or
    Now Playing content.
-   Right side: Queue panel with Now Playing / Next Up.
-   Persistent bottom playback bar.
-   Queue panel can be collapsed; on narrower windows it may adapt into
    an overlay or separate panel.
-   Support Grid and List library views.
-   Preserve usability at smaller window sizes.

Suggested navigation:

-   Home
-   Explore
-   Search
-   Your Library
-   Playlists
-   Favorites
-   Recently Played
-   Local Music
-   Online Music
-   Settings

Required content pages and details:

-   Home
-   Explore
-   Search Results
-   Your Library
-   Playlist Detail
-   Album Detail
-   Artist Detail
-   Song Detail
-   Favorites
-   Recently Played
-   Now Playing
-   Settings

The Home page uses a hybrid composition: personal library content,
online recommendations, and listening activity. Recommendations are
general/popular provider content, not behaviorally personalized in the
initial release.

## 6. Responsive behavior and display modes

-   Main desktop layout is the priority.
-   Queue can appear as a right sidebar or separate panel/window; both
    must share the same live queue state.
-   Mini Player can be a compact main-window mode or a separate movable
    window.
-   Separate Mini Player supports optional Always on Top.
-   Now Playing supports two user-selectable layouts: Centered Artwork
    and Split Layout.
-   Fullscreen Lyrics is a distinct immersive mode, not the normal Now
    Playing screen.

## 7. Personalization and animation

Personalization level: Advanced.

-   Dark and light themes.
-   User-selectable accent color.
-   Display scaling.
-   Persist settings across restarts.
-   Live preview where appropriate.

Animation level: Advanced.

-   Page transitions, hover feedback, artwork transitions, play/pause
    animation, track changes, micro-interactions, and skeleton loading.
-   Include Reduce Motion.
-   Respect Windows accessibility preferences when possible.
-   Avoid animations that block interaction or degrade playback.

------------------------------------------------------------------------

# PART III --- LIBRARY AND MUSIC ORGANIZATION

## 7. Local music import and scanning

Support both Manual Import and Automatic Scan.

-   User can select one or more music folders.
-   Scan on startup and on request, with a manual Rescan action.
-   Monitor selected folders for changes while the application is
    running, where supported.
-   Use incremental scanning for new or changed files.
-   Exclude subfolders in the initial scan behavior, as selected during
    planning.
-   Detect moved, deleted, or inaccessible files carefully.
-   Do not indiscriminately delete library metadata or playlist
    references when a file disappears.
-   Read metadata, duration, technical audio information, and embedded
    artwork.
-   Never move or delete original files automatically.
-   Metadata and artwork edits must not modify original audio files by
    default.

If a file is unsupported or corrupt, record a useful status and continue
scanning other files.

## 8. Supported audio formats

Target broad support for:

-   Lossy: MP3, AAC, M4A, OGG, Opus, WMA.
-   Lossless: FLAC, ALAC, WAV, AIFF.
-   Advanced formats where a reliable, legally usable decoder is
    available: APE, DSD, and other suitable formats.

Do not promise that every format is supported until the selected decoder
and Windows implementation have been validated. Report unsupported
formats clearly.

## 9. Library organization and artwork

Library views:

-   Grid and List.
-   Filters: All, Local, Online.
-   Categories: All Music, Songs, Albums, Artists, Local Music, Online
    Music, Recently Added.
-   Grid is artwork-first with compact metadata.
-   List shows title, artist, album, duration, and quality when
    available.

Artwork priority:

1.  Embedded audio metadata artwork.
2.  Album-folder artwork.
3.  Permitted online artwork provider.
4.  User-uploaded artwork.

Manual artwork upload must remain available because online lookup may
fail or return the wrong result.

Artwork management level: Complete.

-   Adaptive artwork quality.
-   Cache artwork and manage cache automatically.
-   Preserve frequently used artwork and clean unused cache.
-   Optimize animated artwork to reduce CPU/RAM use.
-   Provide static fallback when animation is unsupported or performance
    requires it.
-   Animated covers should play continuously on library cards, not only
    on hover.
-   Provide performance safeguards for large grids.

Metadata editing supports title, artist, album, genre, year, and track
number. Do not rewrite the original audio file by default.

## 10. Duplicate handling and sources

Use hybrid duplicate handling:

-   Group entries as one displayed track only when identity matching is
    sufficiently confident.
-   Keep each source separately selectable.
-   Preserve source-specific metadata.
-   Do not merge uncertain matches.
-   Matching can consider provider ID, metadata, album, duration, and
    optional fingerprinting where reliable.

Source selection behavior:

-   Prefer an available local source when offline.
-   Select an appropriate playable source automatically when possible.
-   Allow manual source selection in details.
-   If a source fails, try another confidently matching source.
-   If all sources fail, show a clear error and allow retry/manual
    selection.
-   Continue from the previous position if duration/seek synchronization
    is possible; otherwise restart from the beginning and explain if
    necessary.

## 11. Playlists, favorites, queue, and history

### Playlists

Manual Playlist Only. Do not implement Smart Playlists in the initial
scope.

Support:

-   Create, rename, and describe playlists.
-   Playlist cover artwork.
-   Add local and online tracks.
-   Remove a track without deleting the original file.
-   Drag-and-drop reorder.
-   Delete with confirmation.
-   Persistent playlist ordering.

Playlist detail should follow the supplied visual reference as closely
as practical: artwork, title, description, track count/duration,
Play/Shuffle actions, and track list.

### Queue

Queue is temporary playback order; a playlist is persistent.

Advanced queue behavior:

-   Play Next.
-   Add to Queue.
-   Remove item.
-   Drag-and-drop reorder.
-   Clear Queue.
-   Save Queue as Playlist.
-   Now Playing / Next Up display.

Restore Queue behavior:

-   Save the last queue locally.
-   Restore it on app relaunch, preserving order and playback position
    where source supports it.
-   Mark or safely skip unavailable tracks.

### Favorites

-   Add/remove favorites.
-   View all favorites.
-   Play all.
-   Sort by title, artist, album, or date added.

### Recently Played and Play Count

Recently Played: Basic History.

-   Newest first.
-   Show title, artist, and artwork.
-   Replay, remove one item, or clear all.
-   Retain forever unless manually removed.
-   Store locally per profile.

Play Count: Basic.

-   Count total plays per song.
-   Count local and online tracks played through BitNot.
-   Increment immediately when playback starts.
-   Store locally per profile.
-   Do not provide individual or global reset in the initial scope.
-   Use local Play Count for Artist Page Popular Tracks.
-   If there is no play history for an artist, hide the Popular Tracks
    section.

No standalone Statistics page in the initial scope.

## 12. Track, album, and artist details

### Track list

Compact list: title, artist, duration. Richer metadata appears in Song
Details.

Single-click on a track list item opens Song Details; it does not
automatically play.

Right-click context menu:

-   Play
-   Play Next
-   Add to Queue
-   Add to Playlist
-   Add to Favorites
-   View Details
-   Remove from Library

Remove from Library removes only the BitNot library reference; it never
deletes the original file.

### Sorting

Hybrid sorting:

-   Albums default to original album order.
-   Playlists default to manual order.
-   User may sort by title, artist, duration, or date added.
-   User can revert to default ordering.
-   Alternate sorting must not destroy stored manual playlist order.

### Album behavior

Album details include artwork, title, artist, year if available, track
list, Play All, Shuffle, and per-track duration.

-   Selecting a track and pressing Play starts at that track and
    continues through subsequent tracks in album order.
-   Album Shuffle shuffles all tracks.
-   Track list stays compact.

### Artist behavior

Complete Artist Page:

-   Artist artwork/photo if available.
-   Artist name.
-   Popular Tracks.
-   Album discography.
-   All available tracks.
-   Play All and Shuffle.
-   Biography only if provider supplies it.

Popular Tracks:

-   Based only on BitNot local per-profile play counts, including local
    and online tracks played through BitNot.
-   Hide section if no local play history exists.
-   Do not use external provider popularity as a fallback.

Artist Play All plays all displayed artist tracks in displayed order.
Shuffle is separate. Album discography sorts newest first.

Group songs under the primary artist only. Show collaborators in track
credits.

## 13. Genre and ratings

-   One main genre per track.
-   Read from metadata where available.
-   User may edit genre manually.
-   Unknown genre remains blank/Unknown.
-   Use a fixed advanced genre list with common genres and subgenres
    (for example Indie Pop, Alternative Rock, Deep House, Synthwave).
-   Do not allow users to create new genres in the initial scope.
-   No mood system.
-   No song rating system; Favorites is sufficient.
-   No song notes/comments system.

------------------------------------------------------------------------

# PART IV --- SEARCH AND ONLINE MUSIC

## 14. Search behavior

Search Suggestions initial state:

-   Search bar.
-   Recent searches.
-   General popular music recommendations.
-   Recent searches can be cleared.

Categorized Results:

-   Songs
-   Albums
-   Artists
-   Local
-   Online

Ranking: relevance first.

-   Title match has highest priority.
-   Then artist, then album.
-   No fixed preference for local or online source.

Offline search: local files only. Do not display stale online cached
results as current online search results. Online results return when
connected.

Basic filters:

-   Title, artist, album only.
-   No advanced genre/year/duration/format/quality filters initially.

Instant Search:

-   Update results as the user types.
-   Local results should respond immediately.
-   Online requests may use debounce/throttling required by provider API
    terms while preserving an instant-search user experience.

Exact Matching:

-   No fuzzy typo correction or automatic guessing.
-   Matching remains case-insensitive.
-   Do not silently alter metadata.

Artist search remains categorized across Songs, Albums, and Artists; do
not prioritize one category.

Selecting a song opens Song Details first. Show artwork, title, artist,
album, duration, audio information, Play, Add to Queue, and Add to
Favorites. Do not auto-play merely because a result was selected.

Selecting an album opens Album Detail. Selecting an artist opens the
Complete Artist Page.

Search result presentation: medium artwork with title, artist, album,
duration if available, and clear Local/Online distinction.

Multilingual search: Advanced where feasible.

-   Support transliteration for Japanese, Korean, and Chinese when
    reliable libraries/providers make it practical.
-   Support spelling variants and diacritics.
-   Never mutate source metadata because of transliteration.

No voice search.

Search history: keep the latest 5 queries, with a clear action.

No results: simple "No Results Found" state, show the search term and
invite another query. Do not add automatic correction or speculative
suggestions.

## 15. Online music providers and legal requirements

Use a hybrid provider architecture. Potential providers discussed
include Audius, Jamendo, Internet Archive, and Wikimedia Commons, but
these are candidates only. Before integrating any provider, verify
current:

-   API availability and documentation.
-   Authentication requirements.
-   Rate limits and attribution requirements.
-   Streaming permissions.
-   Download permissions.
-   Caching and offline-use restrictions.
-   Commercial/non-commercial use restrictions.
-   Territory restrictions.
-   Artwork and metadata usage terms.
-   Branding and attribution requirements.

Do not assume that every item on a public archive is free of copyright
restrictions.

Provider behavior:

-   Search all enabled supported providers by default.
-   Users can enable/disable individual providers in Settings.
-   If one provider fails, continue returning results from others.
-   Respect each provider's API and licensing terms.
-   Do not scrape commercial streaming platforms or bypass DRM.
-   Do not use undocumented/private APIs.

Online recommendations are general popular tracks and broad discovery,
not personalized behavioral recommendations.

Explore page: Advanced.

-   Popular Music.
-   General Recommended Music.
-   Browse by Genre.
-   Albums.
-   Artists.
-   Quick Playback.
-   Only show categories supported by actual providers.

## 16. Streaming

Streaming quality: Adaptive.

-   Automatic adaptation to network conditions.
-   Manual Low, Normal, High, Maximum.
-   Maximum is capped by the provider's available quality and rights.

Buffering: Basic standard audio engine buffering. No manual buffer
controls.

Internet interruption behavior: Hybrid Recovery.

-   Attempt automatic reconnection.
-   Resume at last position if possible.
-   If a matching local version exists, switch to local.
-   Otherwise show connection error and allow manual retry.
-   Never interrupt local playback because an online provider is
    unavailable.

## 17. Download management

Advanced download management, only for provider-authorized downloads.

-   User chooses download location.
-   Dedicated default BitNot download folder.
-   User can change folder in Settings.
-   Changing folder affects future downloads only; do not automatically
    move existing downloads.
-   Check access permissions and available disk space.
-   Show Downloading, Completed, Failed, and Unavailable states.
-   Show storage information.
-   Allow removing a downloaded copy without removing its
    library/playlist reference.
-   No advanced download queue, pause/resume, or automatic retry in the
    initial scope.

Download quality: Match Streaming. If exact quality is unavailable, use
the nearest supported quality.

Download playback treatment:

-   Freely usable files explicitly supplied for download may enter the
    local library.
-   Licensed or DRM-protected content remains subject to provider terms
    and must not be treated as a free local file.
-   Never remove or modify original files without permission.

------------------------------------------------------------------------

# PART V --- LYRICS AND PLAYBACK EXPERIENCE

## 18. Lyrics

Automatic Lyrics Search only.

-   Search using title and artist.
-   Use only legal sources and APIs that permit the intended display.
-   Require timestamped lyrics for synchronization.
-   No manual LRC import/editor in the initial scope.
-   Do not upload user audio to identify lyrics.
-   Line-level synchronization is the default.
-   Per-word highlighting only when the source actually provides
    word-level timestamps.
-   If lyrics are unavailable, show a graceful unavailable state.

Fullscreen Lyrics:

-   Immersive fullscreen mode inspired by the visual quality of Apple
    Music, without copying protected assets.
-   Enlarged blurred artwork background with gradient.
-   Large centered lyric text.
-   Highlight active line.
-   Smooth automatic scrolling.
-   Clicking/tapping a timestamped lyric line seeks to that position.
-   Easy return to Now Playing.
-   Normal Now Playing remains a separate mode.

## 19. Now Playing, Mini Player, and visualizer

Now Playing: Basic controls and information.

-   Artwork.
-   Title and artist.
-   Play/Pause, Next, Previous.
-   Progress and duration.
-   Shuffle and Repeat.
-   Volume.
-   Technical information in Song Details.

Artwork: Hybrid static and animated; continuous animation while playing,
with CPU/RAM optimization and static fallback.

Mini Player: Advanced.

-   Small artwork.
-   Title and artist.
-   Progress.
-   Play/Pause.
-   Previous/Next.
-   Volume.
-   Shuffle.
-   Compact main-window mode or separate movable window.
-   Optional Always on Top.

Audio Visualizer: Basic.

-   Audio Bars and Waveform.
-   Follows audio rhythm where technically reliable.
-   Simple visual style.
-   Can be disabled for performance.
-   Available in Now Playing and Mini Player with separate on/off
    settings.

Queue panel: Hybrid.

-   Right sidebar or separate panel/window.
-   Both views share the same live queue.
-   Save preferred presentation in Settings.

## 20. Playback controls and audio features

### Audio quality and output

Audio quality: Advanced. Format support target: Maximum, subject to
decoder validation.

Expose accurate technical information where available:

-   Codec.
-   Bitrate.
-   Sample rate.
-   Bit depth.
-   Source and output information.

Do not claim Hi-Res, bit-perfect, or lossless output merely because
source metadata says so.

Advanced audio settings may include:

-   Output device.
-   Supported sample-rate choices.
-   Exclusive mode only where supported and safe.
-   ReplayGain/normalization.
-   Equalizer.
-   Crossfade.
-   Gapless.
-   Engine buffer behavior (no manual buffer UI in initial scope).

### Equalizer

Complete:

-   Graphic EQ/manual bands.
-   Presets.
-   Create, save, rename, and delete custom profiles.
-   Per-device profiles.
-   Reset.
-   Live adjustment.

### Audio effects

Complete, subject to engine/device capability:

-   Bass Boost.
-   Virtual Surround.
-   Loudness.
-   Stereo Enhancer.
-   Reverb.
-   Spatial Audio where supported.
-   Individual intensity.
-   Combine effects where technically safe.
-   Save profiles.
-   Per-device profiles.
-   Reset and toggle.
-   Disable unsupported effects safely and explain why.

### Crossfade and gapless

Crossfade: Basic, fixed 3--5 seconds, toggle on/off, gradual overlap.
Gapless playback is a separate capability.

### Sleep timer

Basic:

-   15, 30, 45, and 60 minutes.
-   Stop playback at countdown end.
-   Show remaining time.
-   Allow cancellation.

### Keyboard shortcuts

Basic fixed mapping:

-   Space: Play/Pause.
-   Ctrl+Right: Next.
-   Ctrl+Left: Previous.
-   Ctrl+Up: Volume Up.
-   Ctrl+Down: Volume Down.
-   Ctrl+F: Search.

Avoid hijacking shortcuts while the user is typing in text fields.

------------------------------------------------------------------------

# PART VI --- WINDOWS INTEGRATION

## 21. Windows media integration

Advanced Windows Media Integration:

-   Multimedia keyboard controls.
-   Windows Media Session integration.
-   Track title and artist in Windows media overlay.
-   Play/Pause, Next, Previous through Windows media controls.
-   Artwork where Windows supports it.
-   Playback status synchronized between BitNot and Windows.

## 22. Lock Screen integration

Advanced, within Windows capability:

-   Show title, artist, and artwork where supported.
-   Play/Pause.
-   Next/Previous where supported.
-   Keep state synchronized with the app.
-   Respect Windows version and system policy restrictions.

## 23. Audio output device management

Complete:

-   Choose output device inside BitNot.
-   Support Windows-recognized speakers, headphones, Bluetooth devices,
    USB DACs, and other output devices.
-   Save preferred output device per profile.
-   Detect newly available devices.
-   Switch output without opening Windows Settings.
-   Display current output device.

Device disconnect behavior: Smart Hybrid.

1.  Try the user's prioritized alternative device.
2.  Then try the Windows default output device.
3.  If no output is available, pause playback.
4.  Notify the user and preserve playback position.
5.  Allow playback to resume when a device becomes available.

Do not silently claim successful switching if the audio engine reports
failure.

## 24. Audio focus and other applications

Smart Audio Focus:

-   Calls may pause music or reduce its volume.
-   Important game/application audio may trigger volume adjustment where
    Windows exposes enough information.
-   Ordinary notifications should not automatically stop music.
-   Restore volume gradually after interruption.
-   Expose user-adjustable Audio Focus behavior in Settings.
-   If the OS cannot identify audio category reliably, use a
    conservative fallback.
-   Do not promise perfect classification of every other application's
    audio.

## 25. Startup, close, tray, and notifications

Startup System: Complete.

-   Launch on Windows startup toggle.
-   Open Normally or Start Minimized.
-   Background playback.
-   Optimize startup time.

Close Behavior: Smart Close.

-   User can choose Exit or Minimize to Tray.
-   Optional confirmation.
-   Tray mode keeps playback running.
-   Tray menu: Play/Pause, Next/Previous, Open, Exit.
-   Clicking tray icon restores the application.

System Tray: Complete.

-   Open.
-   Play/Pause.
-   Next/Previous.
-   Track title/artist.
-   Volume.
-   Status.
-   Quick settings.
-   Exit.

Notifications: Basic.

-   Windows toast on track change.
-   Artwork, title, artist.
-   Respect Windows notification settings.
-   User can toggle in Settings.

------------------------------------------------------------------------

# PART VII --- LOCAL-FIRST DEVICE ECOSYSTEM

## 26. Accounts and identity

No mandatory account.

Google Sign-In is optional and is used only as an identity signal for
pairing/recognition. It is not a cloud account system, cloud database,
cloud storage, or automatic internet sync service.

Do not require Firebase or a private backend for Google identity. Use a
standards-compliant OAuth flow and verify current Google requirements.
Never embed a client secret in a desktop app. If the selected OAuth flow
cannot be implemented securely without a backend, explain the constraint
and propose a safe alternative rather than pretending it is secure.

## 27. Device pairing and trust

-   Pair devices using QR and PIN.
-   Require explicit user approval for new devices.
-   Same Google identity can help identify the primary user/device
    relationship.
-   Different Google accounts or no Google login are allowed if pairing
    is manually approved with QR + PIN.
-   Same LAN does not automatically make a device trusted.
-   Only trusted devices can sync.
-   Use authenticated, encrypted device-to-device communication.
-   Provide trusted-device management.

## 28. Local network synchronization

No cloud sync. Sync only between trusted reachable devices over a local
network.

Hybrid sync behavior:

-   Automatically sync selected small metadata categories when trusted
    devices are on the same reachable LAN.
-   Large music files and original audio transfers require manual
    approval.
-   Sync categories: playlists, favorites, listening history, music
    metadata, app settings, artwork information.
-   Original audio files, music folders, and large files are manual
    transfer only.

Smart Hybrid conflict resolution:

-   Merge non-conflicting changes automatically.
-   Ask the user to choose when changes conflict.
-   Merge where safe and meaningful.
-   Preserve the previous version while resolving a conflict.

Smart Delete:

-   Playlist deletion syncs across devices.
-   Deleted items enter Recycle Bin.
-   Keep for 90 days.
-   User can restore.
-   User may permanently delete earlier.
-   Automatically permanently delete after 90 days.
-   Use deletion tombstones so offline devices do not resurrect deleted
    data.
-   Original audio files are never deleted automatically.

Recycle Bin Complete:

-   Playlists.
-   Favorites.
-   Music collection metadata.
-   Associated artwork where available.
-   Never automatically delete original audio files.

## 29. Profiles

Hybrid profiles:

-   One main profile by default.
-   Optional additional profiles.
-   Each profile has its own library, playlists, favorites, history, and
    settings.
-   Each profile may use a different Google identity or no login.
-   User can switch profiles.

Profile security: No Lock.

-   No profile PIN/password or auto-lock.
-   Switch instantly.

Main profile management: Smart Management.

-   Main profile cannot be deleted until another profile is designated
    as replacement.
-   After replacement, former main profile may be deleted.
-   Confirm deletion.
-   Profile data enters Recycle Bin for 90 days.
-   Original music files remain untouched.

Google account change: Secure Hybrid.

-   Confirm release of old account.
-   Verify new account.
-   Connect only after successful verification.
-   If verification fails or is cancelled, keep the previous account
    connected.

Google logout: Smart Logout.

-   Logout Only: release Google connection but keep local identity and
    local data.
-   Logout & Remove Google Identity: release connection and remove
    Google identity while retaining local music data.
-   Playlists, favorites, history, library, and audio files remain safe.

Trusted devices after logout: Smart Device Management.

-   Keep Trusted Devices: local trust/sync remains.
-   Revoke All Devices: other devices lose trust and must pair again.
-   Google logout does not automatically disconnect local pairing.

## 30. Background sync and network management

Background Sync: Smart Background.

-   App continues in System Tray after main window closes.
-   Local sync runs while a trusted device is reachable.
-   Schedule is configurable.
-   Network usage limit is configurable.
-   User can pause or disable sync at any time.

Sync prioritization: Smart Sync.

-   Important data (playlists, favorites, settings) syncs quickly.
-   Other changes may be batched.
-   Manual sync available.
-   Resume after network interruption.
-   Sync must not interrupt audio playback.

Network management: Complete.

-   Speed limit.
-   Network priority.
-   Bandwidth monitor.
-   Rules for Wi-Fi, Ethernet, and Hotspot.
-   Manual override.

Public Wi-Fi: Allow Automatically, but security remains mandatory.

-   Local sync may be allowed on public Wi-Fi.
-   Encryption and trusted-device authentication are always required.
-   Pairing new devices still requires QR + PIN and explicit approval.
-   Never trust arbitrary devices just because they share a network.

Personal Hotspot: Smart Hotspot.

-   Small metadata sync automatic.
-   Large music/file transfers require approval.
-   Estimate data usage.
-   User can stop manually.
-   Resume when connection returns.

Hotspot data cap: Basic.

-   No daily/weekly/monthly cap or automatic stop in the initial scope.
-   Bandwidth monitor remains available.
-   Large transfers still require approval.
-   User can stop manually.

Battery behavior: Smart Battery.

-   Small data sync continues.
-   Large transfers may be delayed or require approval on battery.
-   Adapt to battery condition.
-   Resume when charger is connected.

Idle behavior: Smart Idle.

-   Important data sync promptly.
-   Large transfers prioritized when computer is idle.
-   Lower priority or pause when user is active.
-   Manual transfers remain available anytime.
-   Default idle threshold: 5 minutes without keyboard/mouse activity.
-   User can adjust threshold.

CPU/RAM: Adaptive Resource Management.

-   Adapt resource use to system load.
-   Reduce sync priority when games/heavy apps are active.
-   Transfer more when system is free.
-   Prioritize uninterrupted music playback.
-   Adapt to battery state.

RAM limit: Smart Limit.

-   Automatic based on available memory and system load.
-   Optional manual cap.
-   Adaptive artwork/metadata cache.
-   Avoid excessive memory use.

Artwork cache: Automatic Cache Management.

-   Automatically preserve frequently used artwork.
-   Clean unused cache.
-   Optimize animated artwork to limit CPU/RAM.
-   No manual cache controls required in initial scope.

No separate performance-monitor page is required. Adaptive management
should work quietly in the background.

------------------------------------------------------------------------

# PART VIII --- DATA, RECOVERY, PRIVACY, AND ACCESSIBILITY

## 31. Local storage

Use the default Windows AppData location for:

-   Database.
-   Playlists.
-   Favorites.
-   History.
-   Metadata.
-   Artwork/cache.
-   Profiles.
-   Settings.
-   Trusted-device and sync information.

Do not ask the user to choose a database folder during initial setup.

## 32. First run and empty states

Quick Setup:

-   Language.
-   Theme.
-   Music folder.
-   Basic audio preferences.
-   Finish.
-   Optional steps may be skipped and changed later.

Guided Home:

-   Short, non-intrusive guide for Add Your Music, Create Playlist,
    Explore Music, and Customize BitNot.
-   Can be dismissed.
-   Must not clutter the premium Home layout.

Empty Library:

-   Simple illustration.
-   Short message.
-   Add Music button.
-   No lengthy tutorial.

## 33. Backup and restore

Basic manual backup:

-   Portable backup file.
-   Restore confirmation.
-   Create a safety copy before restore.
-   Include metadata, playlists, favorites, history, settings, library
    information.
-   Exclude original music files.

Do not add scheduled backup/version management in the initial scope.

## 34. Database recovery and crash recovery

Database recovery: Basic.

-   Detect database corruption.
-   Attempt automatic repair.
-   If repair fails, notify the user.
-   Do not immediately overwrite the original database.
-   Original audio files are unaffected.
-   No scheduled backup/recovery versions in initial scope.

Crash recovery: Complete.

-   Detect previous crash.
-   Save diagnostic logs locally.
-   Restore queue, playback position, last page, and app state where
    possible.
-   Check database integrity.
-   Attempt safe automatic recovery.
-   Never overwrite the only safe database copy if recovery fails.
-   Explain when manual action is needed.

Error reporting: Smart Hybrid.

-   Record errors locally.
-   Let user review diagnostics.
-   Send to GitHub or another reporting service only after explicit
    approval.
-   Exclude sensitive information and private music data by default.
-   Allow user to delete diagnostic logs.
-   No automatic diagnostic upload.

## 35. Permissions

Basic permission management:

-   Ask for folder access when user selects a folder.
-   Ask for device access when a feature needs it.
-   Do not request unnecessary permissions.
-   If access is denied or revoked, show a clear message.
-   No intrusive periodic permission checks.

## 36. Privacy and security

Basic security/privacy baseline:

-   No mandatory login.
-   Library and playlist data stay local by default.
-   Use HTTPS for online services.
-   Never auto-upload local audio.
-   No app PIN or database encryption in the initial scope unless later
    justified by a specific security requirement.
-   Local device sync must still use encryption and trusted-device
    authentication.
-   Store credentials/tokens using secure OS facilities where
    applicable.
-   Do not log secrets, OAuth tokens, personal file contents, or
    sensitive paths unnecessarily.

## 37. Accessibility

Complete accessibility:

-   Follow Windows accessibility settings where possible.
-   Adjustable text size and contrast.
-   Reduce Motion.
-   Full keyboard navigation.
-   Screen reader support.
-   Visible keyboard focus on controls.
-   Accessible labels and descriptions.
-   Animated artwork and visualizer can be disabled.

Accessibility must be considered during component design, not added as a
superficial final patch.

------------------------------------------------------------------------

# PART IX --- WINDOWS RELEASE AND PROJECT DELIVERY

## 38. Windows compatibility

-   Support Windows 10 and Windows 11 where the chosen framework and
    APIs permit.
-   Windows 11 is the primary optimization target.
-   Use fallbacks for features unavailable on Windows 10.
-   Keep core UI and playback behavior consistent.
-   Clearly document any OS-specific limitations.

## 39. Installation and uninstall

Installation: Per-User only.

-   No Administrator permission required.
-   Install under the user's profile.
-   Create Start Menu shortcut.
-   Store user data in AppData.
-   Provide an uninstaller.

Uninstall: Smart Uninstall.

Offer: 1. Keep User Data --- remove application but preserve playlists,
library metadata, favorites, history, profiles, and settings. 2. Remove
Everything --- remove BitNot application and internal data after
explicit confirmation.

Never delete original music files as part of uninstall.

No portable Windows build in the initial scope.

## 40. GitHub workflow and release

Public GitHub repository under Apache License 2.0.

Complete GitHub workflow:

-   Issue templates.
-   Pull request templates.
-   Code review guidance.
-   GitHub Actions CI.
-   Automated build, tests, and quality checks.
-   Release automation where practical.
-   Release notes.
-   Contribution guidelines.

Distribution:

-   Branded Windows installer.
-   GitHub Releases.
-   Automatic update check.
-   Release notes visible to user.
-   User chooses whether to download/install.
-   No forced updates.
-   Verify package integrity.
-   Previous releases remain accessible.
-   No automatic installation without user consent.

Code signing may be optional because of cost; document SmartScreen
implications if unsigned. Do not claim a package is signed unless it is.

## 41. Future platforms

After Windows is stable:

-   Android distribution target: APK through GitHub Releases.
-   iOS distribution target: IPA sideloading, with signing and platform
    restrictions handled later.
-   Do not spend implementation time on mobile clients in the first
    Windows milestone.
-   Keep domain logic modular enough to evaluate later reuse, without
    forcing premature cross-platform abstractions.

------------------------------------------------------------------------

# PART X --- IMPLEMENTATION GUIDANCE FOR CLAUDE CODE

## 42. First action: inspect before changing

When starting work in the repository:

1.  Inspect all existing files, package manifests, architecture,
    scripts, and current implementation.
2.  Identify existing code that can be reused.
3.  Identify the current framework, language, build system, and audio
    stack.
4.  Check installed dependencies and versions.
5.  Check the actual target Windows environment and available APIs.
6.  Report the current state and a proposed architecture before
    destructive changes.
7.  Do not replace the project wholesale just because a different stack
    is familiar.
8.  Preserve working functionality unless there is a clear reason to
    change it.
9.  Do not delete user data or project assets.
10. If the repository is empty, propose the stack and explain trade-offs
    before initializing it.

Do not assume a framework or audio library has already been selected.
Recommend a stack based on the existing project and the actual Windows
requirements.

## 43. Architecture expectations

Design clear boundaries between:

-   UI and presentation.
-   Playback/audio engine.
-   Local library scanner and metadata.
-   Local database and migrations.
-   Artwork cache and animated media handling.
-   Provider adapters and online catalog.
-   Lyrics providers.
-   Download manager.
-   Queue and playlist domain logic.
-   Profile and settings management.
-   Device pairing and local synchronization.
-   Windows integration.
-   Diagnostics and recovery.
-   Installer/update/release tooling.

Use typed models and explicit interfaces. Keep provider-specific
behavior behind adapters. Keep local playback independent from online
providers. Use database migrations and safe file operations. Avoid
unnecessary abstraction and avoid building an entire microservice system
for a local desktop application.

## 44. Security and legal engineering rules

-   Never embed private API secrets in a distributed desktop binary.
-   Use OAuth patterns appropriate for installed applications; do not
    ship a client secret.
-   Never bypass DRM, authentication, provider limits, or content
    restrictions.
-   Do not scrape services against their terms.
-   Respect rate limits and attribution.
-   Validate URLs and downloaded file paths.
-   Prevent path traversal and unsafe archive extraction.
-   Use authenticated encrypted pairing and sync.
-   Require explicit approval for device pairing and large file
    transfers.
-   Do not trust devices solely because they are on the same LAN.
-   Do not upload diagnostics without explicit consent.
-   Keep original music files read-only unless the user explicitly
    requests a supported file operation.

## 45. Quality requirements

For each implemented feature:

-   Add tests for domain logic.
-   Add integration tests for database, scanner, provider adapters, and
    playback where practical.
-   Test offline operation.
-   Test missing folders, moved files, inaccessible files, corrupt
    metadata, and unsupported formats.
-   Test provider failure and partial provider availability.
-   Test audio-device disconnect and reconnection.
-   Test crash recovery without risking the only database copy.
-   Test Windows 10 and Windows 11 behaviors where possible.
-   Test keyboard-only and screen-reader navigation.
-   Test high-DPI scaling and window resizing.
-   Test animated artwork performance with many cards.
-   Verify no audio stutter during scans, artwork processing, or sync.

Do not mark a feature complete merely because its UI exists. It must
work, fail safely, and have meaningful tests.

## 46. Phased implementation plan

Implement in gated phases. Do not attempt to build the entire product in
one unreviewed change.

### Phase 0 --- Repository audit and technical design

-   Inspect repository and environment.
-   Recommend architecture and dependencies.
-   Identify risky/uncertain features.
-   Create implementation roadmap and test strategy.
-   Do not perform broad destructive refactoring.

### Phase 1 --- Foundation

-   Application shell and navigation.
-   Branding and theme.
-   Local database and migrations.
-   Settings and profiles.
-   First-run Quick Setup.
-   AppData storage.
-   Basic diagnostics.

### Phase 2 --- Local library and core playback

-   Manual import and folder scan.
-   Metadata and artwork.
-   Library Grid/List.
-   Song/Album/Artist details.
-   Playback engine.
-   Queue, playlists, favorites, history, and play count.
-   Local-only search.
-   Offline behavior.

### Phase 3 --- Premium player experience

-   Now Playing.
-   Mini Player.
-   Animated artwork and cache optimization.
-   Equalizer and supported audio effects.
-   Crossfade, gapless, sleep timer.
-   Lyrics provider interface and legal timestamped source integration.
-   Fullscreen lyrics.
-   Visualizer.
-   Keyboard shortcuts and accessibility.

### Phase 4 --- Online catalog

-   Provider compliance review.
-   Provider adapter framework.
-   Search, Explore, online track details.
-   Adaptive streaming.
-   Authorized downloads.
-   Provider failure recovery.
-   Source matching and switching.

### Phase 5 --- Windows integration

-   Media Session.
-   Lock Screen where supported.
-   Multimedia keyboard.
-   Output device selection and recovery.
-   Audio Focus behavior.
-   System Tray.
-   Startup, close behavior, and notifications.

### Phase 6 --- Local device ecosystem

-   QR/PIN pairing.
-   Trusted devices.
-   Encrypted local-network communication.
-   Metadata sync.
-   Conflict handling.
-   Recycle Bin and deletion tombstones.
-   Manual large-file transfer.
-   Background/network/battery/idle-aware sync.

### Phase 7 --- Stability and release

-   Crash recovery.
-   Backup/restore.
-   Error review/reporting.
-   Performance and accessibility audit.
-   Windows 10/11 compatibility tests.
-   Installer/uninstaller.
-   GitHub Actions, release notes, integrity verification.
-   Final release checklist.

A phase is not complete until its acceptance criteria and tests pass. If
a feature is blocked by provider terms, OS limitations, missing
hardware, or unavailable APIs, mark it as blocked/limited and implement
the safe fallback. Do not silently skip it.

## 47. Definition of done

A Windows release candidate is ready only when:

-   Local music import, scanning, library, and playback work reliably.
-   Playlist, queue, favorites, history, and search persist correctly.
-   Offline local playback works without any account or network.
-   Online features use only verified permitted sources.
-   UI follows the approved BitNot visual direction.
-   Animated artwork does not cause unacceptable resource usage.
-   Audio device loss does not corrupt playback state.
-   Data survives restart and safe recovery.
-   Original music files are not modified/deleted unexpectedly.
-   Installer and uninstaller work without Administrator privileges.
-   User data retention/removal choices behave correctly.
-   Accessibility and keyboard navigation are usable.
-   CI, tests, and release artifacts pass validation.
-   Known limitations are documented honestly.

------------------------------------------------------------------------

# PART XI --- FIRST CLAUDE CODE TASK

Do not start by implementing every feature in this document.

Your first task is **Phase 0 only**:

1.  Audit the current repository and development environment.
2.  Summarize the existing architecture and working functionality.
3.  Identify technical risks, especially audio playback, Windows Media
    Session, online provider licensing, lyrics licensing, local-network
    security, and Windows 10 compatibility.
4.  Recommend an implementation architecture and dependency choices.
5.  Propose a detailed phased task breakdown with acceptance criteria.
6.  Identify which requirements need a proof-of-concept before full
    implementation.
7.  Do not delete or overwrite existing user work.
8.  Do not start large-scale implementation until the audit and proposed
    plan are presented for review.

After Phase 0 is reviewed, proceed one phase at a time. Keep a concise
implementation log and update this document when a decision is
technically constrained. Ask the user only when a product decision is
genuinely necessary; do not repeatedly ask about decisions already
specified here.
