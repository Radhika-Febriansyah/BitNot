# BitNot — Phase 3 Technical Assessment

Date: 2026-11-18. Starting point: commit `eac3a35` on `main`, published to GitHub. The first GitHub Actions run on that commit passed (type check, frontend tests and build, Rust format, lint, and tests on a clean Windows runner).

## 1. Current architecture findings

### Playback

- **`src-tauri/src/playback.rs`** holds two parts:
  - `DeviceOutput` wraps one `rodio::Player` on the default output device, opened once at start-up.
  - `Controller` owns the queue, chooses what plays, records plays, saves and restores the queue, and is tested against a fake `Output`.
- **Loading model:** one track at a time. `load` stops the player, decodes the file with Symphonia, appends it, and seeks if needed. The next track is only loaded after the ticker (every 250 ms) notices the current one has finished. That leaves a gap of up to about 250 ms plus decoder start-up between tracks.
- **Volume:** set on the player as a squared linear value. Settings are written only when the slider rests.
- **Interface link:** commands return the new state to the window. A `playback:progress` event carries the position and `playback:changed` announces track changes. Playback changes made by one window are not broadcast to other windows.

### Interface

- **Player controls:** shuffle, previous, play/pause, next, repeat, seek, volume, and the queue toggle, all in `PlayerBar.tsx`.
- **No Now Playing page, no app-wide keyboard shortcuts, no sleep timer, no mini player, no lyrics.**
- **Keyboard handling today:** track rows, the queue, and menus each handle their own keys. There is no app-wide handler.
- **Translations and router:** a typed translation system exists (`src/i18n`), about half the interface uses it, and a small in-memory router (`src/lib/router.ts`) handles pages.

### What the audio library offers for Phase 3 (rodio 0.23)

| Need | rodio 0.23 | Fit |
|---|---|---|
| Equalizer, effects | `Source` is an iterator of `f32` samples; any filter can wrap a source. Built-in low-pass and high-pass filters, gain, fades, a simple delay-based reverb. | Good for EQ bands and simple effects, written as source wrappers |
| Gapless | Decoder builder trims encoder padding (`with_gapless`, on by default). The player plays appended sources back to back. | Possible if the next track is appended before the current one ends, which the current one-at-a-time model does not do |
| Crossfade | `Mixer` mixes several sources; fade helpers exist | Possible, but needs two decoders active together and changes to how position and "finished" are measured |
| Visualizer | `periodic_access` and source wrappers can copy samples out | Possible: a wrapper writing into a small ring buffer the window polls |
| Output device choice (Phase 5) | Device can be chosen when opening the sink | Possible; needs re-opening the sink on change |

## 2. Feature status at the start of Phase 3

| Feature | Status |
|---|---|
| Keyboard shortcuts (§20) | Done 2026-11-18 (Milestone 1, step 1). Physical-keyboard and Narrator checks pending. |
| Now Playing (§19) | Done 2026-11-18 (Milestone 1, step 2), Centered and Split layouts. Narrator check pending. |
| Audio pipeline proof of concept (§6) | Done 2026-11-19. Recommendation: keep rodio and extend it (section 9). Two defects found in the current engine (9.4). |
| Engine defect fixes (9.4) | Done 2026-11-19: constant-format output stage with windowed-sinc resampling (section 10). |
| Dated playback clock (8.4) | Done 2026-11-18. Readings carry the core's monotonic time; both seek bars advance smoothly between them. |
| Sleep timer (§20) | Done 2026-11-18 (Milestone 1, step 3), including "Stop after current track" (approved 2026-11-18). Narrator check pending. |
| Equalizer, effects, crossfade, gapless, visualizer (§19–20) | Not started |
| Mini Player, fullscreen lyrics, animated artwork (§18–19) | Not started. Animated originals are already kept in the artwork cache. |

## 3. Dependencies and technical risks

| Risk | Level | Notes |
|---|---|---|
| Gapless and crossfade need a different loading model | High | The controller must preload the next track and track position across a source change. The `Output` trait will need extending, with the controller tests updated alongside. |
| DSP cost and audible quality | Medium | EQ and effects run inside the audio callback. They must be cheap, glitch-free on parameter changes, and correct at any sample rate. |
| Lyrics licensing | High (legal) | Synchronised lyrics are copyrighted. Any online source needs a licence review first (CLAUDE.md §18). No provider is chosen. |
| Multi-window state (Mini Player) | Medium | Playback changes must be broadcast to all windows before a second window exists. |
| Global shortcuts | Medium | System-wide hotkeys and media keys belong to Windows integration (Phase 5). Phase 3 shortcuts work while BitNot has focus. |
| Accessibility of new views | Medium | Now Playing and lyrics must stay operable by keyboard and screen reader. |

## 4. Recommended sequence

Revised 2026-11-18 after the BitChord study (section 8). Steps 1 and 2 are done.

1. **Keyboard shortcuts** (done).
2. **Now Playing page** (done).
3. **Sleep timer** (done). Built on a monotonic end time in the core rather than a ticking countdown (8.4).
4. **Dated playback clock** (done 2026-11-18; see the implementation log): each progress event carries the moment it was measured, and the interface advances the position smoothly between events. It jumps back only on a real seek or track change. Small; needed before synchronised lyrics and the visualizer, and it makes the seek bar smoother (8.4).
5. **Audio pipeline proof of concept** (done 2026-11-19; results and recommendation in section 9). Waiting for approval before any production change.
6. **Engine groundwork** (from the proof of concept, 9.9): fix the two defects it found in the current engine (done 2026-11-19, section 10), then **gapless,** then **equalizer and presets,** then **crossfade,** then **visualizer,** all on rodio.
7. **Playback event broadcast,** then **Mini Player.**
8. **Lyrics:** licence review first; then the provider interface and fullscreen lyrics.
9. **Audio effects** beyond EQ, where safe.
10. **Animated artwork, accessibility pass, translation coverage.**

## 5. Milestones and acceptance criteria

### Milestone 1 — Core player experience

**Keyboard shortcuts**
- Space plays or pauses; Ctrl+Right and Ctrl+Left go to the next and previous track; Ctrl+Up and Ctrl+Down change volume by 5 (CLAUDE.md §20); Ctrl+F focuses search.
- Additional shortcuts: Ctrl+M mutes and unmutes; Ctrl+Shift+Right and Ctrl+Shift+Left seek 10 s forward and back; F1 or `?` opens the shortcut reference.
- Shortcuts never fire while typing in a text field, while a dialog or menu is open, or when Space would activate a focused button or row.
- The shortcut reference is reachable from Settings and from the keyboard, and player tooltips name their shortcut.
- The decision logic is unit-tested; the handler has component tests.

**Now Playing**
- A dedicated view showing large artwork, title, artist, album, quality, progress, all transport controls, and volume, consistent with the design language.
- Opened from the player bar and closed with Escape or Back. Works at all supported window sizes and in both themes. Shows an empty state when nothing is loaded.

**Sleep timer**
- 15, 30, 45, and 60 minutes, and Stop after current track (added with approval on 2026-11-18). The remaining time is shown, and the timer can be cancelled.
- When time runs out, playback pauses and the position is kept. Stop after current track lets the song finish and does not start the next one.
- The timer lives in the core, so it works with the window minimised. It is not persisted across restarts.
- Tests cover expiry, cancellation, and a pause or track change mid-timer.

### Milestone 2 — Audio experience (after the proof of concept)

- **Gapless:** consecutive tracks of the same album play with no audible gap, on more than one format.
- **Equalizer:** at least 10 bands with presets and custom profiles, applied live without clicks. Bypassed, it is bit-transparent.
- **Crossfade:** a fixed 3–5 s, toggleable; position and "finished" stay correct.
- **Visualizer:** bars and waveform from real samples, and it can be turned off.
- **CPU:** the audio thread stays well below real time on this machine, measured.

### Milestone 3 — Additional experiences

- **Mini Player:** a separate window with optional always-on-top, sharing live state.
- **Fullscreen lyrics:** only from a source whose terms permit it, with a graceful unavailable state.
- **Animated artwork:** plays only when visible and can be turned off.
- **Accessibility and translations:** a Narrator pass, and translation coverage completed.

## 6. Audio pipeline proof-of-concept proposal

**Question:** can the existing rodio/Symphonia pipeline support gapless playback, a live equalizer, crossfade, and a sample tap for the visualizer, with acceptable quality and CPU? Or is a custom pipeline (Symphonia decoding with direct WASAPI output through `cpal`) needed?

**Method:** a standalone example program under `src-tauri/examples/`, not wired into the app, that:

1. Plays two real files back to back by appending the second decoder before the first ends, and measures the gap. Silence detection on a sample tap is the criterion.
2. Wraps the source in a 10-band biquad equalizer and changes gains while playing. It checks for discontinuities and confirms that a flat equalizer leaves samples unchanged.
3. Crossfades two sources through a `Mixer` with fade-out and fade-in, and checks the reported position.
4. Copies samples into a ring buffer for a visualizer and measures how far behind real time it is.
5. Measures CPU time spent in the processing chain per second of audio.
6. Added after the BitChord study (8.4):
   - Changes EQ gains as a glide rather than a step, and listens for clicks.
   - Compares a biquad with a state-variable filter under live parameter changes.
   - Adds a peak limiter at the end of the chain and checks that boosts do not clip.
   - Crossfades with the incoming track on a second decoder and an equal-power curve, moving "current track" at the midpoint.
   - Reports the exact sample where a gapless boundary falls.

**Alternatives compared:**
- *Extend rodio:* least change, keeps tested behaviour. Its limits are the position and "finished" accounting across mixed sources.
- *Custom Symphonia + cpal pipeline:* full control, but more code to own, including resampling and device handling.
- *Windows Media Foundation:* native, but Windows-only, with a large FFI surface. Not recommended.

**Decision rule:** if rodio passes 1–5 with tolerable complexity, extend it behind the existing `Output` trait. Otherwise write the custom pipeline behind the same trait, so the controller and its tests carry over. The result and the numbers go into `docs/implementation-log.md` before any engine change.

## 7. Outstanding items carried into Phase 3

- **Manual checks from Phase 2:** Ogg Vorbis playback, listening tests, real mouse drags, file pickers, and a Narrator pass.
- **Manual checks from Phase 3 so far:**
  - Narrator pass over Now Playing (layout switch, artist and album links, Favorites toggle, Song details) and the player-bar Open/Close Now Playing button. **Pending.**
  - Narrator pass over the shortcut dialog and the mute button. **Pending.**
  - Shortcuts on a physical keyboard (verified so far with real key events sent through DevTools). **Pending.**
  - Narrator pass over the sleep timer button and its menu (radio items, disabled items, the countdown in the button name). **Pending.**
- **Output device:** fixed at start-up until Phase 5.

## 8. Reference study: BitChord (2026-11-18)

Repository: `github.com/kushagrasinghx/BitChord`, studied as an external reference only. I read its README, its design notes (`web/docs/`, `docs/`, `backend/README.md`), release list, open-issue counts, and the design comments of a few player files. I did not install, build, or run it. Nothing from it was copied into BitNot, and nothing below is BitChord text or code; it is my summary of observable concepts.

### 8.1 What it is

- **A YouTube Music client**, mainly for Android (Kotlin, Media3/ExoPlayer, Jetpack Compose), with Android TV and Wear OS apps, a browser app with a companion extension (React, TypeScript), a C++ audio-analysis library, and a Go server for "listen together" sessions.
- **Licence:** GPL-3.0. Created 2026-08-11 and very active (last push 2026-11-18); about 6.9k stars and 433 open issues.
- **Windows:** one desktop beta (v1.8, 2026-10-05, Compose Desktop: installer, MSI, portable). Every release since has been Android and TV only, so its Windows experience is a beta and not a mature reference.
- **Its central function is out of bounds for BitNot.** Its design notes document streaming from YouTube's private player API by impersonating clients, generating bot-protection tokens, and rewriting request origins through a browser extension. That is the use of undocumented APIs and the working around of access controls that CLAUDE.md §2, §15 and §44 forbid.

### 8.2 How much is verified

I did not run BitChord, so nothing is verified by me. The evidence for each feature is:

| Evidence | Features |
|---|---|
| **Design notes with recorded measurements** | Browser crossfade (two decks, tested in Chromium and Firefox), browser lyrics rendering and seeking, YouTube playback success rates (17 of 20 test tracks) |
| **Unit tests in the repository** | Sleep timer, equalizer processor, loudness processor, queue rules, USB audio logic, lyric parsing |
| **Advertised, beta, or with open defects** | Automix (labelled Beta; open issues on loops and repetition), animated "canvas" artwork (5 open issues), lyrics (34 open issues), Windows desktop (one beta) |
| **Not found** | A separate mini-player window. Its "mini player" is the collapsed player bar on the phone, which BitNot already has. |

### 8.3 Comparison with BitNot

Complexity: S is days, M is about a week, L is several weeks.

| Area | BitChord | BitNot now | What BitNot could take from it | Fits? | Complexity | Risks |
|---|---|---|---|---|---|---|
| Keyboard shortcuts | Space, Ctrl+arrows, Ctrl+F or `/`, Esc (browser app) | Same core set plus seek, mute, help; ignored while typing or in dialogs | Nothing important. `/` for search would conflict with `?` on some keyboard layouts. | — | — | — |
| Now Playing | Phone full-screen player; landscape layout; codec and bit depth shown | Centered and Split layouts, format line, Song details, Next Up | Background colour taken from the artwork, for fullscreen lyrics only (8.4) | Yes, later | S–M | Contrast and accessibility of text over artwork colour |
| Sleep timer | Presets 15–60 min, a custom length, or stop after the current track; stored as a deadline on a monotonic clock | Not started | The deadline design. "Stop after this track" needs your decision (8.7). | Yes | S | None significant |
| Previous | Restarts the track if more than 10 s in | Already restarts past a threshold | — | — | — | — |
| Equalizer | 10 bands (shelves at the ends), bass/treble, pre-amp, balance; changes glide; limiter at the end of the chain | Not started | Gliding changes, a pre-amp with a limiter, and a state-variable filter as a candidate to biquads. All are standard published techniques. | Yes, through the PoC | M | Clicks on change, clipping, CPU. A bypass made of a "flat curve" is not bit-transparent, so BitNot should glide to flat and then remove the stage. |
| Loudness / ReplayGain | Gain per player, applied at the exact sample where a track starts | Not started | Apply ReplayGain per track and per deck, so a crossfade does not re-level the outgoing track | Yes | M | Needs the gapless boundary event |
| Crossfade | Two full players; the incoming track loads on the idle one; equal-power curve; "current track" moves at the midpoint; 0–12 s | Not started (spec: fixed 3–5 s) | The two-deck model with the incoming track on the second decoder. It matches rodio's `Mixer`. The midpoint handoff gives a clear rule for history, Now Playing and position. | Yes, through the PoC | L | Position and "finished" accounting, play-count timing, device changes mid-fade |
| Gapless | Media3's built-in gapless | Not started | The boundary event idea: switch per-track state at the exact sample | Yes | M–L | Same loading-model change as crossfade |
| Lyrics timing | A clock that dates each position reading, advances on its own frames, and moves backwards only after a real seek | Position updates every 250 ms from the core, without smoothing between updates | The dated-clock design for lyrics and the visualizer (sequence step 4) | Yes | S–M | Drift tests; must stop while paused |
| Lyrics sources | Twelve sources, several reached through cookies or an extension that rewrites origins; on-demand translation through Google | No source chosen; licence review required | Only the parsing concepts (timed lines, word timing). Not the sources. | Sources: no | — | Legal: most sources have no display licence |
| Fullscreen lyrics backdrop | Artwork averaged to a small colour grid, painted as drifting blobs on a low-resolution canvas, stopped when paused or with Reduce Motion | Not started | A cheap animated backdrop meeting "enlarged blurred artwork background" (§18) without full-resolution blur | Yes, Milestone 3 | M | GPU use on weak machines; needs an off switch |
| Animated artwork | Fetched from Apple Music, Spotify and Tidal motion-artwork endpoints | Animated originals from local files are already kept in the cache | Nothing. The source is unsuitable (8.5). | No | — | Legal |
| Automix (beat-matched transitions) | Beta; ported from an AGPL-3.0 project; uses essentia.js; analyses each track for 12–18 s | Not in scope | Nothing now | No | L+ | Licence (AGPL), CPU, battery, complexity |
| Bit-perfect output | Android USB direct driver | Not started; WASAPI exclusive mode is Phase 5 "where supported" | Only the honest rule: say "bit-perfect" only when it truly is (already CLAUDE.md §20) | Phase 5 | L | Device compatibility |
| Queue tiers | Separate "context", "added by you" and "autoplay" sections; Clear removes only tracks you added | One queue (Phase 2) | Possibly later: Clear keeping the album or playlist you started | Maybe | M | Changes behaviour approved in Phase 2; needs your decision |
| Speed, skip silence, per-network quality | Present | Not in scope | Postpone | — | — | Not in CLAUDE.md |

### 8.4 Ideas worth investigating independently

These are general techniques, documented in public audio literature, that BitChord's notes describe using. BitNot would implement them from its own design.

1. **Sleep timer as a deadline.** Store an end time on a monotonic clock (Rust `Instant`), let the existing 250 ms ticker check it, and report time left from it. Changing the system clock cannot cut it short, and nothing has to count down. Fits `AppCore` directly.
2. **Dated playback clock.** Stamp each `playback:progress` event with when it was measured. The interface advances the position between events with `requestAnimationFrame`, eases towards readings that differ slightly, and jumps back only on a real seek or track change. This prepares lyrics and the visualizer, and costs little.
3. **Smooth parameter changes in DSP.** Move EQ and effect parameters towards their target over a few milliseconds instead of stepping. Measure clicks in the proof of concept, and compare biquad and state-variable filters for stability while a slider moves.
4. **Headroom and limiter.** EQ boosts need a pre-amp cut and a peak limiter at the end of the chain. The limiter must be out of the path (bit-transparent) when nothing can clip.
5. **Two-deck crossfade.** Load the incoming track on a second decoder, use an equal-power (sine/cosine) curve, and treat the midpoint as the moment the "current track" changes for the queue, history, Now Playing, and Windows media controls. The play count still increments when the incoming track starts, per §11.
6. **Per-track gain at the exact boundary.** ReplayGain belongs to each deck and switches at the sample where the next track begins, not when the interface hears about it.

### 8.5 Unsuitable for BitNot

| Item | Reason |
|---|---|
| YouTube Music playback, sign-in, or any part of its InnerTube, BotGuard or token code | Undocumented APIs and working around access controls (CLAUDE.md §2, §15, §44) |
| Spotify, Apple Music and Tidal motion artwork | Unofficial use of proprietary services and assets |
| Lyrics from Musixmatch, Genius, KuGou, Apple and similar, through cookies or origin rewriting | No display licence; working around access controls (§18) |
| Lyrics translation through Google's translate endpoint | Unofficial endpoint; sends listening data online; not in scope |
| Automix | AGPL-3.0 origin and an AGPL-3.0 analysis library; heavy CPU; not in scope |
| Listen-together server | Needs a server BitNot would operate (§2: no mandatory private backend) |
| Discord presence, Last.fm, ListenBrainz, Spotify accounts | Not in scope; account and privacy implications |

### 8.6 Licensing notes

- **BitChord is GPL-3.0.** BitNot is Apache-2.0. Apache-2.0 code may go into a GPL-3.0 project, but not the other way round: any copied BitChord code would require BitNot to be distributed under the GPL-3.0. Nothing was copied. Its logo, banner, screenshots and wording are not used.
- **Its dependencies include copyleft libraries:** essentia.js and its Automix port are AGPL-3.0 (per its notes), and its lyrics renderer is MPL-2.0. None of them is proposed for BitNot.
- **Ideas and techniques are not covered by copyright,** but expression is. Any feature above is designed and written from BitNot's own architecture, tests, and words.

### 8.7 Effect on the Phase 3 plan

- **Prioritise:**
  - The sleep timer, using the deadline design.
  - A small dated-clock step before the proof of concept.
  - The six investigations in 8.4, added to the proof of concept (section 6, item 6).
- **Postpone:**
  - Artwork-coloured backdrop (with fullscreen lyrics, Milestone 3).
  - Queue tiers (needs your decision).
  - Playback speed and skip silence (outside CLAUDE.md).
- **Unsuitable:** everything in 8.5.
- **No architecture change.** Everything adopted fits the current `Output` trait, the controller, and the React contexts. BitChord's main argument for crossfade (that a second decoder is unavoidable) is already the model in section 6.
- **Decision needed:** whether the sleep timer also offers "stop after the current track". CLAUDE.md §20 lists only 15/30/45/60 minutes. It is cheap to add (it ends on the player's own "track finished" event, not a computed deadline), but it is a product addition.

## 9. Audio pipeline proof of concept: results (2026-11-19)

Code: `src-tauri/examples/audio_poc/` (standalone; not wired into the application, and the production engine was not changed). Full output of the reference run: `src-tauri/target/audio-poc/results.md` (generated, not committed).

### 9.1 Executive summary

- **Recommendation: keep rodio and extend it (Option A).** Every planned feature was demonstrated on rodio with sample-exact timing, clean audio, and very low CPU use. No measured limitation needs a custom pipeline.
- **Gapless works in rodio** when the next track is appended before the current one ends: output bit-identical to the two files joined, in every format pair tested, offline and on the real device.
- **Crossfade, EQ, limiter, and the visualizer tap** all fit as rodio `Source` wrappers. With all of them running at once, the cost was 8 ms of CPU per second of audio, and there were no missed device callbacks in 3 runs of 20 s.
- **Two defects found in the current engine** (present since Phase 2, not caused by this work):
  - rodio's own 44.1 → 48 kHz conversion is low quality (THD+N about −35 dB at 1 kHz, −7 dB at 16 kHz). It affects every 44.1 kHz file on this 48 kHz device today.
  - The first 10.7 ms of the first track after the player is created play wrongly, because the player is connected to the mixer before anything is queued.
  - Both have measured fixes that stay inside rodio (9.4).

### 9.2 Test environment

- Windows 11 Home Single Language 10.0.26200; Intel Core 5 210H (8 cores, 12 threads); 15.6 GB RAM.
- Output: Speakers (Realtek(R) Audio), WASAPI shared mode, 48 000 Hz, 2 channels, f32. The default period is 480 frames (10 ms). cpal lists 8 kHz to 384 kHz as accepted in shared mode, which Windows converts.
- rodio 0.23.0 (`symphonia-all`, `playback`), cpal 0.18.2 (through rodio's re-export), symphonia 0.5.5. Rust 1.99.1, release profile. No new dependency was added.
- Test audio is generated by the example, so every result can be reproduced:
  - **Gapless:** a continuous 997 Hz stereo sine (left sin, right cos, −6 dBFS) split into two files at an odd frame count.
  - **Noise programs:** music-like spectral tilt, normalised where stated.
  - **Formats:** WAV 16-bit, AIFF 16-bit, FLAC 16-bit and 24-bit (verbatim frames from a small writer in the example, decoded by Symphonia), at 44.1 and 48 kHz.
- **Device runs** measure the samples the device thread pulls, then multiply them by zero before the speakers, so nothing is audible. Set `BITNOT_POC_AUDIBLE=1` to hear them.
- **Repetitions:** CPU figures are the median of 5 runs. The device runs were repeated 3 times; their figures agreed within the ranges shown. The other experiments are deterministic.

### 9.3 Method

- **Bit-exact comparisons:** output against the same files decoded separately.
- **Timing steps:** the phase of a known tone per 10 ms block. A gap or jump shows as a phase step.
- **Distortion:** THD+N per 100 ms block (residual after fitting the tone).
- **Clicks:** peak of a 4th-order high-pass (at 4× the tone, at least 2 kHz) during a change, in dB relative to the tone.
- **Loudness through a crossfade:** stereo RMS per 50 ms.
- **CPU:** single-thread wall time pulling a 48 kHz mixer as fast as possible.
- **Device timing:** a tap times each burst of pulls by the device thread (one callback) against the audio it held. The cpal comparison uses cpal's own callback timestamps.

### 9.4 Results

**A. Gapless**

| Check | Result |
|---|---|
| Next track appended before the current one ends (rodio `Player` queue), 6 pairs: WAV, FLAC 16/24-bit, AIFF, mixed formats, 44.1 and 48 kHz | Output bit-identical to A followed by B in all 6 pairs; no zero samples at the join |
| Next track appended only after the current one ended | 256 frames (5.8 ms) of silence inserted: the queue's keep-alive filler |
| Through the 48 kHz mixer, 48 kHz files | Bit-identical; output length exact |
| Through the 48 kHz mixer, 44.1 kHz files (resampled) | No zeros at the join. Phase step at the join 11–13 µs, against 2–9 µs at every decoder packet elsewhere: the join is no worse than any packet boundary |
| 44.1 kHz track followed by a 48 kHz track | No zeros, but a 320 µs phase step at the join: the timing drift that rodio's resampling had built up in the first track (see A4) |
| On the real device (FLAC 44.1 kHz pair), 3 runs | The samples the device thread pulled were identical to the same chain run offline; no callback missed |
| Current engine (detects the end on its 250 ms ticker, then opens the next file) | Gap up to one tick (≤ 250 ms) plus file opening (0.04 ms WAV, 0.1–0.12 ms FLAC measured) |

**A4. Defect 1: rodio's sample-rate conversion.** (Fixed: section 10, which also corrects the PoC wrapper's frequency response.) rodio resamples by linear interpolation, and its `UniformSourceIterator` starts a new converter at every span (decoder packet), which adds a small timing step each time. Measured on a 2 s, 44.1 kHz file played into the 48 kHz mixer:

| Tone | File itself | rodio, continuous buffer | rodio, from the decoder (as in BitNot) | Timing steps and drift | PoC windowed-sinc wrapper |
|---|---|---|---|---|---|
| 997 Hz | −92.1 dB | −46.2 dB | −34.5 dB | 47 steps, −101 µs | −92.0 dB, no steps |
| 4 999 Hz | −92.1 dB | −30.0 dB | −20.3 dB | 132 steps, −189 µs | −92.1 dB, no steps |
| 9 997 Hz | −92.1 dB | −19.5 dB | −13.6 dB | 142 steps, −192 µs | −91.9 dB, no steps |
| 15 997 Hz | −92.1 dB | −9.7 dB | −7.4 dB | 145 steps, −192 µs | −91.0 dB, no steps |

- The output is also 10 frames longer than it should be over 2 s.
- A 48 kHz file into the 48 kHz mixer is passed through bit-exact.
- **Fix inside rodio:** resample before rodio's mixer with a proper resampler, so rodio never converts. The example's 32-tap Kaiser windowed-sinc `Source` reaches the file's own noise floor at a cost of about 2 ms per second of audio. For gapless, the resampler must sit after the queue (on the continuous stream), not on each track.

**A5. Defect 2: start-up glitch.**
- `DeviceOutput` connects its `Player` to the mixer while the queue is empty. The mixer's format converter then takes the empty queue's mono format for its first span.
- Measured: 511 of the first 600 output frames carry the stereo file as mono at half speed, and afterwards the output is 256 frames (5.3 ms) late.
- **Fix:** append before connecting, or add the queue to the mixer after the first append (measured clean in A3).
- From rodio's code this applies only to the first track after the player is created. That conclusion is inferred and was not measured inside the application.

**B. Equalizer.** 10 bands at 31.25 Hz–16 kHz (octave centres): low shelf, 8 peaking filters (Q √2), high shelf.

| Check | Result |
|---|---|
| Response, RBJ biquad vs Simper SVF | Identical to 0.01 dB in every configuration; the set gain is reached exactly at an isolated band's centre (+12.00 dB) |
| Band interaction | Neighbouring bands add up: all bands at −12 dB measure −18.3 dB in the middle. The displayed curve should be the computed response, not the slider values |
| Flat (0 dB) | Bit-exact: 0 of 192 000 samples changed, whether the bands are bypassed or all 10 filters run at 0 dB. After gliding up and back to 0 dB, bit-exact again |
| Click on an instant +12 dB change (100 Hz tone, 125 Hz band) | 0.0 dB re tone (as loud as the tone itself); steady-state reference −96 dB |
| Biquad with a glide | 30 ms: −11.6 dB; 100 ms: −24.6 dB. Updating coefficients every frame does not help (−11.5 / −24.0 dB) |
| SVF with a glide | 30 ms: −29.0 dB (updates every 16 frames) and −53.7 dB (every frame); 100 ms with every-frame updates: −64.2 dB |
| Clipping (program peaking at −3 dBFS, bass and treble +12 dB) | Unprotected: +1.5 dBFS, 44 samples over full scale |
| −12 dB pre-amp | −10.5 dBFS peak, none over |
| Peak limiter (−1 dBFS, instant attack, 50 ms release, no look-ahead) | −1.00 dBFS peak, none over. Gain reduced by more than 1 dB on 16% of frames (heavy on this deliberately hot program) |
| Limiter on audio below the threshold | 0 samples changed. rodio's built-in `limit()` also changed none |

**C. Crossfade** (4 s, rodio Mixer, deck B started with `delay`):

| Material | Linear | Equal-power |
|---|---|---|
| Uncorrelated (two noise programs: typical of two songs) | dips to −3.33 dB | −0.48 to +0.56 dB (the noise's own 50 ms variation) |
| Correlated (identical signal on both decks) | ±0.00 dB | rises to +3.01 dB |

- **Clicks:** none at the start, midpoint, or end with either curve. The second-difference ratio stayed at 0.74–1.41, against 1.0 for the material itself.
- **Handover:** at the fade midpoint the incoming deck had produced exactly 96 000 frames (2.000000 s), counted in its own wrapper, so the "current track" can switch on an exact frame. rodio's `track_position()`, read every 5 ms as the Player does, gave 1.995 s.
- **rodio's built-in `take_crossfade_with`** is linear (−3.31 dB dip) and ends both sources at the fade end, so BitNot needs its own equal-power wrapper.

**D. Visualizer capture**
- **Ring buffer:** lock-free, single producer and single consumer, 2^16 samples.
- **Correctness:** the capture is identical to the samples played.
- **Usefulness:** a 2048-point FFT of it finds the 1 kHz and 5 kHz test tones, enough for frequency bars and for waveform minimum/maximum.
- **No waiting:** with no reader at all, the audio side produced 1 s of audio in 0.3 ms and dropped what didn't fit, without waiting.
- **On the device** (3 runs): the newest captured sample was 3.8–4.2 ms old when read (median; 95th percentile 8.5–8.8 ms; maximum 10.1–13.4 ms), and nothing was dropped.
- **Remaining offset:** audio reaches the speakers after the device buffer: rodio requests 2048 frames (42.7 ms); cpal directly reports 31.8–31.9 ms. The visualizer and lyrics must allow for this.

### 9.5 Performance

CPU per second of audio, one thread, 44.1 kHz sources into a 48 kHz mixer (median of 5; 1000 ms would be real time):

| Configuration | ms per s | × real time |
|---|---|---|
| Playback (decode, Player, rodio resampling) | 2.07 | 484× |
| + windowed-sinc resampler instead of rodio's | 4.09 | 245× |
| + EQ, 10 biquads | 3.68 | 272× |
| + EQ, 10 SVFs | 5.27 | 190× |
| + EQ, 10 SVFs gliding continuously, coefficients every frame (worst case) | 10.58 | 95× |
| Crossfade, two decoders | 4.19 | 239× |
| + visualizer capture | 2.25 | 445× |
| All: crossfade + EQ on both decks + limiter + capture | 8.16 | 123× |

On the device, with everything running for 20 s (3 runs):

| | Result |
|---|---|
| Callbacks per run | 2 015–2 019, about 9.9 ms of audio each |
| Time to produce one callback | mean 0.27–0.29 ms, longest 1.28–1.76 ms |
| Missed callbacks | 0 |

The cpal-direct comparison produced a callback in 0.07 ms on average, with the longest at 0.73–1.28 ms and none missed.

### 9.6 Known limitations of this proof of concept

- **Encoder delay not tested:** no MP3 or AAC encoder is installed, so gapless with encoder delay and padding (LAME header, iTunSMPB, which rodio's `with_gapless` relies on) was not measured. It needs real files before gapless is called done for lossy formats.
- **Device-level limits:**
  - All runs were muted after measurement, so nothing was confirmed by listening.
  - Glitches inside Windows' audio engine are invisible to the application; only the application's own deadlines were checked.
  - One machine and one device (Realtek, 48 kHz). Windows 10, Bluetooth, USB DACs, exclusive mode, and device switching were not tested.
- **Latency through rodio** cannot be read (rodio does not expose cpal's timestamps). Only the requested buffer size is known.
- **The PoC limiter** has no look-ahead (instant attack), and the sinc resampler is a feasibility check, not a tuned production component.
- **The A5 conclusion** that only the first track is affected comes from rodio's code, not from a measurement inside BitNot.

### 9.7 Option A (extend rodio) vs Option B (Symphonia + cpal)

| Criterion | Option A: rodio, measured | Option B: direct cpal, measured in the minimal experiment |
|---|---|---|
| Sample-exact track transitions | Yes: bit-identical joins (A1), exact frame at the crossfade handover (C2) | Yes, by construction (switch at frame 288 000 = file length) |
| DSP control | Full, via `Source` wrappers; flat EQ and limiter bit-exact | Full |
| Resampling | rodio's built-in is poor, but bypassed bit-exactly when sources arrive at the device rate; a wrapper fixes it | None built in; BitNot would write or add one either way |
| Real-time behaviour | 0 missed callbacks; longest 1.8 ms against 10 ms | 0 missed; longest 1.3 ms |
| Output latency visible to the app | No (requested buffer 42.7 ms only) | Yes (31.8–31.9 ms from WASAPI timestamps) |
| Device handling, format negotiation, fallback | Provided by `DeviceSinkBuilder` | Would be written and maintained by BitNot |
| Effect on the existing `Output` trait, controller, and tests | Extends them | Replaces the output side |

- **Windows Media Foundation:** offers nothing measured here that is needed. Windows already converts sample rates in shared mode (cpal opened the 48 kHz device at 44.1 kHz). Using Media Foundation would add a large Windows-only API surface. Not recommended.

### 9.8 Recommendation

**Keep rodio as the foundation.**
- Every required experiment passed on rodio: sample-exact gapless and crossfade timing, transparent EQ and limiter when idle, a non-blocking visualizer tap, and large CPU headroom.
- The one serious weakness measured, resampling quality, sits in a stage rodio skips bit-exactly when the audio already matches the device rate, so it can be replaced inside rodio's model.
- Option B's only measured advantage is visible output latency. That doesn't justify rewriting device handling. If exact latency becomes necessary for lyrics or the visualizer, it can be evaluated then, without changing the processing chain.

### 9.9 Suggested next steps (each needs approval; none started)

1. **Fix the two defects in the current engine:**
   - Connect the queue to the mixer only after the first append (A5).
   - Resample with a proper converter before rodio's mixer, so rodio never resamples (A4). Choose between an in-house windowed-sinc stage like the PoC's, or an established crate such as `rubato` after a licence and dependency review.
   - Add a regression test for each.
2. **Gapless:**
   - Append the next track ahead of the end, with the resampler on the continuous stream.
   - Detect the track change at the exact frame (a frame-counting wrapper, or the queue's length) instead of the 250 ms ticker.
   - Test with real MP3 and AAC files for encoder delay.
3. **EQ:**
   - Use SVF filters with per-frame coefficient smoothing (about 30–100 ms) and exact bypass at 0 dB.
   - Add a pre-amp equal to the largest boost, with a limiter as a safety net.
   - Show the computed response curve.
4. **Crossfade:**
   - Two decks in the mixer, with equal-power gains.
   - Switch the "current track" on the incoming deck's exact frame count at the midpoint; play counts still increase when the next track starts.
5. **Visualizer:**
   - An SPSC ring after the final mix.
   - Compensate for the device buffer (about 43 ms with rodio's requested buffer, or a smaller buffer chosen deliberately).

**Reproduce** (from `src-tauri/`):
- `cargo run --release --example audio_poc -- all` runs everything, in about 46 s.
- `cargo run --release --example audio_poc -- gapless|eq|crossfade|capture|cpu|device|cpal` runs one part.
- Add `BITNOT_POC_AUDIBLE=1` to hear the device runs at low level.

## 10. Engine defect fixes (2026-11-19)

The two defects found by the proof of concept (9.4) are fixed in the production engine. rodio stays the foundation; nothing else about playback changed.

### 10.1 Defects and root causes

1. **First-track start-up glitch.**
   - `DeviceOutput` connected its rodio `Player` to the device mixer before anything was queued.
   - The mixer wraps each source in a `UniformSourceIterator`, which takes the source's format when the source is added. An empty queue reports 1 channel at 48 kHz, so the first span of the first track was converted as mono.
   - Measured: 511 of the first 600 frames had identical left and right, and the track then ran 256 frames (5.3 ms) late.
2. **Low-quality resampling.**
   - Whenever the file's rate differed from the device's (for example 44.1 kHz on this 48 kHz device), rodio converted it by linear interpolation, restarting the converter at every decoder packet.
   - Measured: THD+N −34.5 dB at 1 kHz and −7.4 dB at 16 kHz, timing steps at each packet, and 10 extra frames every 2 s.

### 10.2 The fix: one output stage with a constant format

`src-tauri/src/resample.rs`, used by `DeviceOutput::on_mixer` in `playback.rs`.

```
decoder → Marked → Player queue → OutputStage (stereo, device rate) → device mixer → WASAPI
```

- **Device format:** taken from the stream actually opened (`sink.config()`), and logged at start-up ("audio output: 48000 Hz, 2 channels, F32"). Nothing is assumed.
- **Constant format:** the Player is created unconnected (`Player::new`). Only the output stage is added to the mixer, and it always reports two channels at the device rate, with no spans. The mixer therefore never converts the rate and never sees a format change, which removes defect 1 structurally.
- **Channels:** mono plays on both sides; beyond two channels the first two are kept, as rodio's own converter does.
- **Rate conversion:**
  - Windowed sinc, 96 taps at the lower rate, Kaiser window (β 9), cut-off at 0.97 of the lower Nyquist frequency.
  - The kernel is a table with 512 entries per sample, read with linear interpolation.
  - The read position is an exact fraction, input rate over output rate, so it cannot drift.
  - It runs on the queue's continuous output, not per track, so decoder packets and boundaries between tracks at the same rate are invisible to it.
  - Audio already at the device rate is passed through bit-exact.
- **Where a track begins:** found by marking, not by asking the queue.
  - rodio's queue cannot say reliably which source a sample came from at a boundary. A decoder never reports itself finished, so the queue describes the old track until the new one has delivered a sample. With another track queued, an in-memory source is never reported finished either.
  - Each track BitNot appends is therefore wrapped in `Marked`. It records its format when it delivers its first sample, inside the stage's own read, so the stage knows the exact first sample of a new track.
  - This was found by a failing test; asking the queue gave the wrong frame both before and after the read.
- **Change of rate between tracks:** the stage finishes the old rate's audio exactly at its end and starts the new rate from the next output frame. No silence is inserted.

**Dependency decision: no new dependency.**
- `rubato` (MIT-licensed, so compatible with Apache-2.0, and actively maintained) was considered.
- It would add several crates (realfft, rustfft and others) and works in fixed chunks, so it would still need the same streaming, boundary, and rate-change handling around it.
- The in-house stage is about 300 lines plus tests, uses only what is already in the project, and is measured below.

### 10.3 Corrections to the proof-of-concept record

- **The PoC's 32-tap resampler (9.4) was flat only to about 16 kHz:** −2.1 dB at 19 kHz, −5.7 dB at 20 kHz, and 42 dB of alias suppression at 23 kHz. The PoC measured distortion but not frequency response.
  - Tap counts and cut-offs were compared before choosing. 96 taps at 0.97 is flat to 20 kHz (0.000 dB), −1.8 dB at 21 kHz, with 91.5 dB of alias suppression; 128 taps gained little.
- **The 320 µs step the PoC saw at a 44.1 → 48 kHz track change (9.4, A)** was attributed to drift. It is probably at least partly rodio tagging the first samples of the new track with the old track's format (the boundary problem in 10.2). That is inferred, not separately measured.
- **9.9 suggested connecting the queue after the first append.** The constant-format stage was chosen instead, because it also covers every later format change.
- **Cost:** about 6× the PoC's estimate (12.7 against 2 ms per second of audio), because of 3× the taps and f64 arithmetic. See 10.5.

### 10.4 Regression tests (automated)

All deterministic, using generated signals.

**`resample.rs`** (10 tests and 1 manual cost measurement):

| Test | Result |
|---|---|
| Audio at the device rate | Bit-exact |
| 44.1 → 48 kHz at 997, 4 999, 9 997, 15 997, and 19 997 Hz | Gain within ±0.001 dB; worst 100 ms THD+N −112.5, −116.0, −112.7, −109.4, −97.5 dB. Old path: −34.5 dB at 1 kHz, −7.4 dB at 16 kHz |
| 48 → 44.1 kHz | 997 Hz: gain +0.001 dB, THD+N −110.2 dB; a 23 kHz tone's alias −91.5 dB |
| Length and drift | 10 s at 44.1 kHz gives exactly 480 000 frames at 48 kHz; deviation from the ideal tone 3.8e-6 in both the first and the last second |
| Decoder packets | 16-bit WAV through the real decoder and queue: deviation 3.6e-5 (the file's quantisation), worst THD+N −92.2 dB (the 16-bit floor); no steps |
| Boundary between tracks at the same rate | Continuous: deviation 3.5e-5 within 10 ms of the join |
| Change of rate between tracks | The 48 kHz track that follows a 44.1 kHz one is bit-exact from its first frame, with no silence at the switch |
| Channels | Mono is duplicated; 4 channels give the first two; mono → stereo track changes land on the exact sample, for in-memory and decoded sources |
| Format | Constant from creation, including while the queue is empty |

**`playback.rs`, `device_output`** (3 tests, driving the real `DeviceOutput` on an offline mixer pulled by a thread):

| Test | Result |
|---|---|
| The original condition, with rodio alone | Still shows the defect (more than 500 of the first 600 frames mono). The test fails if a rodio update changes this, so the reason for the stage can be rechecked |
| First track | 48 kHz bit-exact from its first sample; 44.1 kHz stereo at the right speed from the first millisecond |
| Seek, pause, resume, stop, and track changes | Seek forward and back, with position within tolerance and a clean tone afterwards; pause gives silence with the position held; resume; a change to a 48 kHz track passes it through exactly; `finished()` at its end; loading paused at 1.5 s; stop gives silence |

**Totals:** 145 Rust tests pass (5 consecutive runs), with 1 ignored manual cost measurement. Frontend unchanged: 109 tests pass.

### 10.5 Performance

- **Cost:** release build, 44.1 → 48 kHz, 30 s: 12.6–13.0 ms of CPU per second of audio over 3 runs, about 1.3% of one core.
  - A contiguous-slice fast path brought it down from 20 ms.
  - Audio already at the device rate costs only the copy.
- **Latency:** the stage reads up to 48 frames ahead (1 ms at 44.1 kHz; 1.4 ms when converting 96 → 48 kHz, where it reads 96 frames). The reported position therefore leads the stage's output by at most that much; the device buffer (about 43 ms) still dominates.

### 10.6 On the real device (Windows 11, Realtek, 48 kHz shared mode)

- **Method:** what Windows received was recorded with WASAPI loopback capture, using a scratch tool outside the repository. Another application was playing audio throughout, so each measurement is one that this audio cannot fake: the exact pitch of a 997 Hz tone, and how much of its level survives a coherent fit over the whole capture. Timing steps destroy coherence; other audio does not create a coherent 997 Hz tone.

| Run | Pitch | Coherence loss over about 11.5 s | Level |
|---|---|---|---|
| Old engine's path (rodio mixer, same 44.1 kHz file, rendered offline) | 996.9026 Hz (−98 ppm) | 7.81 dB | — |
| New engine, 44.1 kHz file, first track after start-up | 996.9998 Hz | 0.02 dB | −21.8 dBFS (−6 dBFS at volume 40 is −21.9 expected) |
| New engine, 48 kHz file | 997.0009 Hz | 0.00 dB | −21.7 dBFS |

- **Volume:** live change from 40 to 20 gave a median of −33 dBFS (1 dB resolution, raised slightly by the other audio), against −33.8 expected.
- **In the app:** seek to 5 s (reported 5.605 s after 0.6 s); pause held 5.620 s; resume; seek to 10 s and an automatic change from the 44.1 kHz to the 48 kHz track; seek within it; Previous back to the 44.1 kHz track. No errors logged.
- **Listening:** not performed. No one listened; every device result above is a signal measurement.

### 10.7 Remaining limitations

- **Untested formats and devices:** only WAV, plus FLAC and AIFF through the existing paths (all 16/24-bit PCM). Lossy formats (MP3, AAC, Ogg) go through the same stage but were not measured. No other device rates (44.1 or 96 kHz devices) and no surround devices were tested on hardware; the stage keeps two channels and leaves upmixing to rodio as before.
- **Rate-change boundaries:** at a change of rate between tracks, the old rate's filter tail is cut at its end and the new rate starts with no history. That is a sub-millisecond edge, measured for no silence and bit-exactness after the switch, but not for audibility. Gapless across different rates is not a goal yet.
- **Seeking:** after a seek, the first millisecond of output blends the last pre-seek frames still in the filter. It is measured as clean 50 ms later, and it matches what rodio did before, minus the restart artefacts.
- **First track:** the start-up glitch fix is verified by tests on an offline mixer and by the device's measured pitch and coherence. The first 10 ms on the device itself could not be isolated under the other application's audio.
- **Output latency:** still not exposed by rodio (9.6).
