# Smith design system v2

The shared look for MediaSmith and its standalone siblings: ClipSmith, GifSmith and CaptionSmith. Code lives in `packages/ui` (`tokens.css`, `components.css`, `brand/`, `apps.ts`, `components.tsx`). Mockups are in `docs/mockups/v2`.

**Goal now:** apps that are clean, smooth and polished enough to demo on the web. Every decision below favours a calm, finished feel over adding more features.

## 1. What we took from chess-solver and cube-solver

Those two repos show the house standard the Smith suite should match. Cube Solver 1.2.0 (Oct 14) is the most recent visual pass.

| Lesson | Where it comes from | How the Smiths use it |
|---|---|---|
| Components read only from tokens, so a theme is just a block of values. A test checks that every theme defines the same names. | cube `src/theme/tokens.css`, `skins.test.ts` | `tokens.css` v2 plus `design.test.ts`. The light theme may only override names the base defines, and components may only use defined tokens. |
| The app mark is an original drawing on a dark rounded tile (rx 112/512), with a soft accent glow and a hairline inner edge. | cube `brand/icon.svg` | All four Smith marks use the same tile, glow and edge. |
| The display font is bundled, and only the title uses it. Everything else uses the system or a UI font. | cube Bungee title, chess DM Sans | Syne is used only for wordmarks and big titles; all UI text is Inter. Small Syne was the main reason v1 looked "primitive". |
| Brand assets are listed with their source files. | cube `brand/INVENTORY.md` | `packages/ui/src/brand/*.svg` are the sources. `marks.ts` is generated from them, and a test checks each mark's hue. |
| Theme, updates and About go in one Settings dialog. The header holds only the mark, wordmark and main controls. | cube "Gather theme, updates and About into a Settings dialog" | Top bars contain only the wordmark, context, a few actions and one primary button. |
| Light/Dark/System, following the OS until the user picks. | both | Dark comes first. Light is set with `data-theme="light"` and uses the same token names. |
| The layout doesn't jump: no shift when a scrollbar appears, and controls keep a stable width. | cube "Stop horizontal shift…" | Panel widths are fixed by tokens, numbers use tabular figures, and hover states change color, never size. |
| A chunky segmented control for modes, with one solid primary action. | cube's 2x2…7x7 / Fast-Optimal bars | `.sm-seg` and a single `.sm-btn--primary` per view. |

Chess-solver itself is still on the older warm-parchment and indigo theme, with a flat purple tile logo. It is a cue for consistency (one bold tile, one glyph), not for palette.

## 2. Identity

- **Keep:** near-black canvas, the violet frame (#a974ff, gradient #c6acff→#7a3dff), the cyan spark (#45f2f2), and the Syne wordmark.
- **Refine:** each app gets a **hue** for its glyph, its wordmark prefix and its active states inside the work area. The shared chrome (primary button, focus ring) stays violet, so the suite reads as one family.

| App | Hue | Glyph inside the frame |
|---|---|---|
| MediaSmith | violet #a974ff | stacked frames with a play button |
| ClipSmith | cyan #45f2f2 | trim brackets |
| GifSmith | amber #ffb547 | loop arrow |
| CaptionSmith | pink #ff5fa2 | caption bubble with lines |

The hues are the existing MediaSmith track colours (video, audio, text, caption), so the editor timeline already speaks the family language. Set the hue with `applyIdentity("gifsmith")`, which puts `data-app` on `<html>`.

All four marks share the same tile, frame and trim notches, plus a cyan spark at the top right. Before this, all four logos were the same frame and MediaSmith's was a copy of ClipSmith's. See `docs/mockups/v2/07-logos.png`.

## 3. Foundations (tokens)

- **Surfaces:** `--surface-0..3`, a cool near-black with a slight violet cast. Hairlines are `--line-1`, strong borders `--line-2`.
- **Text:** `--text-1/2/3`. Contrast against `--surface-1` is about 16.6:1 for text-1, 7.6:1 for text-2 and 3.5:1 for text-3. Text-3 is only for labels and hints, never body copy.
- **Type scale:** 11 / 12 / 13 (base) / 15 / 18 / 24 / 32 px. Weights are 450, 550 and 650. Section labels are 11 px uppercase with +0.06em spacing. Numbers use the `.sm-num` tabular figures.
- **Spacing:** a 4 px grid (`--sp-1..10`). Panels pad 16 px, sections are separated by hairlines rather than boxes, and pages get 40 px gutters.
- **Radius:** 4, 6 (controls), 10 (cards), 14 (panels), 20 (dialogs), and full.
- **Elevation:** three levels. Most surfaces are flat, popovers use 2 and dialogs use 3.
- **Layout:** top bar 52 px, tool rail 72, side panel 300, inspector 300, timeline 240.

## 4. Components (`components.css`, `components.tsx`)

| Component | Notes |
|---|---|
| Button | Variants: primary (gradient plus glow, one per view), secondary, ghost, danger. Sizes: sm 26, md 32, lg 40. Icon-only buttons need a `title`, which also becomes their aria-label. |
| Segmented | For mutually exclusive modes (aspect, format, quality, animation). `aria-pressed` drives the styling. |
| Chip | Status labels and counts. The `--hue` variant marks the current selection. |
| Panel and section | Header label, then sections divided by hairlines. |
| Card | Templates and projects. Hover lifts 2 px over 160 ms. |
| Field, input, range | The label sits above. The focus border is brand-coloured, plus a 2 px focus ring. |
| Empty state | Hue-tinted glyph tile, Syne title, one sentence and one action. |
| Drop zone | A dashed border that tints with the hue while a file is dragged over it. |
| Progress | Gradient fill, plus a text line with percent and time left. |
| Toast | ok or error left bar. It rises in over 260 ms and stays out of the way. |
| Dialog | 20 px radius over a blurred backdrop. |
| Kbd | Shows shortcuts in tooltips and in the help overlay. |
| Wordmark | Mark, then the hue prefix, then "Smith". |

## 5. Motion

- **Timing:** 90 ms for press and hover, 160 ms for small state changes, 260 ms for panels, dialogs and toasts.
- **Easing:** `--ease-out` for entering. Nothing bounces in the chrome. Bouncy "pop" belongs to user content (text animations).
- **Playback:** the playhead and timeline scrubbing are never animated. They follow the cursor 1:1.
- **Reduced motion:** `prefers-reduced-motion` sets every duration to 0.

## 6. Iconography

- One stroke icon set at 1.75 px on a 20 px grid, with rounded caps. Lucide (ISC) is the planned set; the mockups use glyph stand-ins.
- Rail icons have their label underneath, always visible.
- Colour means state only: hue for selected, `--err` for destructive.

## 7. Empty states and onboarding

- **First run** opens on Home ("What are you making today?") with four quick actions: New video, Auto-caption, Make a GIF, Grab from a link. You can drop a file anywhere on the page.
- **Empty states** say what the panel is for and offer one action. Examples: an empty timeline shows "Drop clips here or press I to import"; an empty Captions panel shows "Generate captions" with a model-size note.
- **Standalone apps** show a three-step breadcrumb in the top bar (for example Paste link › Trim › Save), so the flow explains itself without a tour.
- **Coach marks** come later, at most three, can be dismissed, and are only shown on the first project.
- **Demo mode (web):** loads a bundled sample project, so the editor is never blank when someone lands from a portfolio link.

## 8. Accessibility

- **Contrast:** WCAG AA for all text. Hue colours are only used on dark surfaces at 12 px or larger, or as fills behind dark text.
- **Keyboard:**
  - Every control is reachable by keyboard and shows the visible `--ring`.
  - Shortcuts are listed in the help overlay (`?`).
  - Timeline items can be moved with the arrow keys (Shift for one second).
- **Labels:** icon buttons carry aria-labels, progress carries ARIA values, and segmented controls use `role="group"` with `aria-pressed`.
- **Colour and motion:** speaker and track colours always come with a label (S1, "Video"). Reduced motion is respected.
- **Hit targets:** at least 24 px, with a 32 px default.

## 9. Product direction

- **Now:** a demo-ready web build plus the desktop apps.
- **Possible future direction:** a free web version with ads, plus a cheap paid desktop download. This is noted only; no layout space is reserved for it.

## 10. Scoped feature list

Rich enough to demo well, scoped to what fits the current engine.

**MediaSmith (editor)**
- Home: templates by platform, recent projects, quick actions, and drop to start.
- Editor:
  - left tool rail with panels for Media, Text, Captions, Elements, Filters, Transitions and Audio
  - canvas with on-canvas handles
  - inspector for the selection
  - timeline with split, duplicate, delete, zoom and fit
- Text: style presets (Bold Pop, Clean, Neon, Lower third), colour swatches, animations (none, fade, pop, slide), fade in and out.
- Captions: auto-captions with karaoke highlighting, speaker labels, words per line and caption style presets.
- Filters: 8 colour presets (LUT-free ffmpeg `eq`/`colorbalance`) with one intensity slider.
- Transitions: crossfade and dip-to-black between adjacent clips. This is the next engine task, since today we have fades only.
- Audio: music track with volume and auto-duck under speech.
- Export dialog: format presets (9:16, 4:5, 16:9, GIF), quality (Draft, High, Max), estimated size and progress with time left.
- Out of scope: keyframe curves, multi-cam, cloud sync and stock libraries.

**ClipSmith:** paste a link and fetch. Preview with a trim window on a filmstrip, typed start and end, format (MP4, WebM, MP3, WAV) and resolution, optional 9:16 centre crop, clear errors (already done) and "Open in MediaSmith".

**GifSmith:** choose a clip, then set the loop window, ping-pong, a size-limit preset (Discord, Slack, custom) with shrink to fit (done), width, frame rate and colour sliders, a text overlay and a live size estimate.

**CaptionSmith:**
- Add a video; the transcript appears alongside it, with speaker chips and click-to-seek.
- Style presets: Karaoke, Subtitle, Boxed, Minimal. Plus words per line and language.
- Export by burning captions in, or as SRT, VTT or ASS.

## 11. Rollout

1. ✅ Tokens v2, components, brand marks, identity helper and tests (`packages/ui`, branch `design/v2`).
2. MediaSmith: switch the top bar, rail, inspector and timeline to `sm-` classes (the v1 `ms-` look stays in place until each screen moves). Add Home, then the Export dialog.
3. Port `packages/ui` to the siblings. Each sibling copies `tokens.css`, `components.css` and its own mark, and sets `data-app`. The token test ships with it.
4. Regenerate desktop icons from `brand/<app>.svg` (`tauri icon`), plus favicons and an OG card.
5. Web demo: a bundled sample project and a final polish pass against the mockups.
