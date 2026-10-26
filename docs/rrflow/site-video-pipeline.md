# rrflow.ai — generated video for a gripping, layman-friendly experience

Date: 2026-10-24. Extends `claude/rrflow-public-site-build-order.md` (Decisions 1–3 stand: `rrflow/site-development`, Rust gateway + Astro islands, VPS demo estate). This record: how Higgsfield and Runway video fits the site without breaking its fidelity rule, the metaphor world, the shot list, the consistency recipe, the web delivery techniques, and a pilot. Nothing has been generated; no credits spent.

## The rule that makes video possible here
AI video cannot show real data, and the site may never fake data. So the experience has **two layers that never trade places**:
- **Metaphor layer** — generated video: the world a layman understands (a city, strata, a trail). Labeled "illustration" once per scene. Never renders words, numbers, UI, logos or charts (generators get those wrong anyway).
- **Truth layer** — HTML/WebGL rendered from the gateway: the Flight Ledger, read stamps, digests, counters, badges. Always overlaid on or beside the video, always from `/api/*`.
The video makes people *feel* what the engine does; the overlay *proves* it. Interaction binds the two: the same scroll, hover or scrub drives both.

## Candidate worlds (one decision)
| World | Records / relations | A flight | VALID vs KNOWN | One commit | rrflowKV | RROFLOW | Fit |
|---|---|---|---|---|---|---|---|
| **Night city on strata** (recommended) | lit buildings / streets; clusters = districts | one violet→cyan light trail gathering five lights, then sealing | the same corner at two times: sun (valid) and how much is built (known) | a whole block appears in one breath, or reverts together | geological layers under the streets: snow (WAL), soil (memtable), rock (segments), pressure (compaction), light veins (cache) | footprints → worn path → paved road → highway | matches the dark-instrument look; everyone reads a city |
| The Archive | books/cards in an infinite library-observatory | a probe sweeping the stacks, returning five sealed items | the same shelf photographed twice; date on the back vs date received | shelves rotate together | strata under the floor | a reading path worn into the carpet | warm, literate; slower to read at a glance |
| Instrument (abstract light) | points and filaments | a filament lighting | two dials | a chord struck once | sediment of light | a groove deepening | closest to current visuals; weakest for laymen |

## Tool split (verified 2026-10-24)
**Higgsfield** — account on Ultra, 5,042.81 credits; costs are quoted per job before generation.
- `seedance_2_5` (workhorse): text-to-video, `omni_reference` (image + video + audio references), `video_edit`, `video_extension`; 4–30 s; 1080p; start and end image. Base shots and extensions.
- `kling3_0`: start + end image (seamless loops: start = end), multi-shot, `4k` mode; `sound off` to save credits. Loops and before/after shots.
- `gemini_omni_flash_1_1`: 4K, `edit` mode. Hero master upscale path.
- `hf_mult_motion_control` (Genjutsu): motion from a reference video onto subjects in reference images. A consistent guide character's movement.
- `scene_builder_3d_*`: build one 3D location, generate camera angles — the consistency anchor: every shot starts from a still of the same city.
- Explainer presets (22 style references: Editorial Motion Graphics, Studio 3D, Low Poly, Paper Diorama, Isometric Flat Vector, …) resolve to a style `media_id` reusable across generations.
- `remove_background` (stills), `upscale_video`, `generate_audio`.
**Runway** (changelog, 2026): Aleph 2.0 + Edit Studio (21 May) — edit one frame, the change carries through the rest of the video → the *state-variant* tool (then/now, planned/live, day/night) without regenerating; Runway Characters (9 Mar, API) — real-time conversational avatars; Seedance 2.5 (Aug, up to 30 s, 1080p) — same base generator as Higgsfield, so masters move between the two; Runway Ruby — SDR→HDR grade; Gen-4.5 image-to-video (Jan). Model Routers and Task Cost API on Runway Dev.
Division of labour: **generate in Higgsfield** (credits in hand, 3D scene builder, presets); **vary and finish in Runway** (Aleph variants, HDR grade, a conversational guide later).

## Consistency recipe (the number-one risk of AI video)
1. World bible: palette (obsidian ground `#0B0D12`, violet `#7C5CFF` → cyan `#22D3EE` for trails, amber `#FBBF24` only for "planned"), lens (35 mm, slow pushes, top-down for maps), materials (wet asphalt, glass, basalt strata), time (always night; VALID changes the sky, KNOWN changes what exists).
2. Build the city block and its strata once in `scene_builder_3d`; export angles as stills; every shot is image-to-video from one of those stills.
3. One approved hero still becomes the style reference passed on every Seedance `omni_reference` call.
4. Variants by Aleph edits of a master, never by re-prompting.
5. Loops by start = end frame; extensions by `video_extension`, not new seeds.
6. Three takes per shot, pick one, log the seed/prompt/refs in `estate/../video/shots.toml` in the site repo so a shot can be re-made when the pin bumps.

## Shot list — night city on strata (v1)
| # | Scene | Shot (6–12 s unless noted) | Generation route | Interaction on the site |
|---|---|---|---|---|
| H | Hero | Aerial slow push over the city at night; visible strata beneath; one trail runs five blocks and freezes into a sealed line; loop | 3D still → Seedance omni-ref → Kling start=end loop; 4K via Gemini Omni | Video texture under the live field; the *real* flight trail from `/api/flights/{id}` drawn over it in alpha |
| 1.0 | Engine | One continuous 20–25 s flight in five beats: lantern lit at an address (seed) → street signs flare (lexical) → windows glow by colour (semantic) → light spreads along streets (graph) → five brightest converge and seal (fusion) | Seedance 30 s t2v + `video_extension`; beats timed to a shot sheet | Scroll-scrubbed (WebCodecs); the Flight Ledger counts rungs in as each beat passes; semantic beat dims with "when an embedding model is bound" |
| 2.0 | rrflowQL | The same corner, two times: sky moves (VALID), buildings appear (KNOWN) | Master + two Aleph variants (dusk/night; block absent/present) | Bi-temporal scrubber crossfades variants; live claim bars beside |
| 3.0 | rrflowDB | A block appears in one breath: building, streets, lights, sign together; reverse = abort | Kling start/end images before/after | Hover "abort" plays backwards; commit anatomy list pins to each element |
| 4.0 | rrflowKV | Camera descends below the street through snow → soil → rock layers compacting; veins of light where reads run; 12 s | Seedance t2v with style refs; vertical move | Scroll-scrubbed descent; live `diagnostics-read` counters pin to each layer |
| 5.0 | RROFLOW | Footprints across grass → worn path → paved road → highway the trail flies straight down | Seedance 15 s or four Aleph states of one field | Hover each stage plays its segment; whole scene badged IN DESIGN |
| 6.0 | Get it | The same city twice: on a desk under a lamp (DevPulse) and under a cloud (shippin.cloud); a thin light between (sync, planned) | Two stills → Kling start/end | Static with a slow loop; badges from the capability overlay |
| G | Guide (optional) | A lamplighter who walks the trail; later a conversational guide | Genjutsu motion transfer (consistent movement); later Runway Characters grounded by `/api/context` so answers come only from the estate, with the ledger as receipts | Hover/click; later ask-the-estate |

## Web delivery (sourced)
- **Scroll-scrubbed video**: demux in-browser, decode with WebCodecs `VideoDecoder`, keep a ±1 s frame window, draw to canvas; backward seeks restart from the previous keyframe, so encode with dense keyframes (every 12–24 frames); close every `VideoFrame`. Chromium complete; Safari unconfirmed → fallback to a poster or `currentTime` stepping. [Lion Keng, WebCodecs scroll sync](https://lionkeng.medium.com/a-tutorial-webcodecs-video-scroll-synchronization-8b251e1a1708)
- **Transparent overlays**: stacked luma/alpha video (colour on top, alpha below) drawn through a WebGL shader; AV1 ≈ 460 kB vs native VP9 alpha ≈ 1.1 MB vs native HEVC alpha ≈ 3.4 MB for the same clip; HEVC for older Apple hardware; avoid animated AVIF. [Jake Archibald, video with transparency](https://jakearchibald.com/2024/video-with-transparency/)
- **Textures**: looping video as a three.js/cosmos.gl texture for the field's ground; cheap on the GPU, loads after first paint.
- **Loops**: start = end frame at generation; no crossfade hacks.
- **Motion safety**: `prefers-reduced-motion` → poster frames; autoplay muted; sound only on an explicit control.
- **Budget**: video island loads after first paint; per-scene clips ≤ 2–3 MB; hero 4K only for desktop, 1080p for mobile; scroll work under ~4 ms/frame.

## Where it lands in the build order
Steps 0–3 unchanged (bootstrap, estate, gateway, site v1 with SVG/HTML scenes). Video enters at step 4 as part of the hero island and step 5 (scroll choreography); each scene's clip replaces nothing in the truth layer, it sits under it. Shots, prompts, references and encodes live in the site repo under `site/public/media/` (encodes) and `media-src/shots.toml` (recipes), never in the estate.

## Pilot (before the full list)
Three shots prove the world or kill it in one sitting: **H (hero loop)**, **4.0 (strata descent)**, **5.0 (desire paths)**. Each: one 3D-scene still, three Seedance takes, one Kling loop; Aleph variant on one of them to test the state-switch. Pass: all three read as one place, no text artefacts, loop seamless, the trail colour matches the UI. Fail: switch world or register before spending further. Credits are quoted per job at generation; nothing is generated without an explicit go.

## Sources
- Higgsfield MCP: `balance` (Ultra, 5,042.81 credits), `models_explore` (Seedance 2.5, Kling 3.0, Gemini Omni Flash 1.1, Genjutsu), `get_explainer_presets` (22 presets), workflow list.
- [Runway changelog](https://runway.com/changelog) — Aleph 2.0 & Edit Studio (2026-05-21), Runway Characters (2026-03-09, API), Seedance 2.5 (2026-08-11), Ruby (2026-08-20), Gen-4.5 image-to-video (2026-01-21).
- [WebCodecs scroll sync](https://lionkeng.medium.com/a-tutorial-webcodecs-video-scroll-synchronization-8b251e1a1708) · [Video with transparency](https://jakearchibald.com/2024/video-with-transparency/)
