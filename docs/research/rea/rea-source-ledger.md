# REA ("Reverse Engineer Anything") source ledger

Harvested 2026-10-24 for the trace-driven learning layer design spec. Facts with sources only. Every claim line ends with a `[sourced: ...]` tag; inferences are tagged `[inferred]`.

Path shorthands (all under `/home/claude/rroflow-absorb/sources/rea/`):

| Alias | Directory | Clone / fetch | Head commit |
|---|---|---|---|
| `AW/` | `AIshihzong_awesome-reverse-engineer-anything/` | shallow clone | `31f300c` 2026-10-16, author AIshihzong [sourced: `git log -1` of that clone, 2026-10-24] |
| `AR/` | `git-sanbu_awesome-rea/` | shallow clone | `fc79e3f` 2026-10-21, author Sanbu [sourced: `git log -1` of that clone, 2026-10-24] |
| `RG/` | `ReverseEngineer-Anything_REA-Github/` | shallow clone | `137b392` 2026-10-21, author `zgdczrgwj2xi6ghi2e` [sourced: `git log -1` of that clone, 2026-10-24] |
| `RF/` | `sickpyramid_ReaFuse/` | shallow clone | `c64502a` 2026-10-15, author sickpyramid [sourced: `git log -1` of that clone, 2026-10-24] |
| `AT/` | `jammyfu_rea-atlas/` | shallow clone | `474f528` 2026-10-24, author PaintingCoder [sourced: `git log -1` of that clone, 2026-10-24] |
| `OS/` | `glauberlima_open-scrantic/` | shallow clone | `60e8e5e` 2026-10-20, author Glauber Lima [sourced: `git log -1` of that clone, 2026-10-24] |
| `UP/` | `morluto_rea_webfetch/` | raw files fetched 2026-10-24 from `raw.githubusercontent.com/morluto/rea/main/` (plus four pinned `website/evidence/*.md` at commit `42fc6c53`) and `registry.npmjs.org/rea-agents` | see `UP/FETCH-NOTE.txt` [sourced: `UP/FETCH-NOTE.txt`; curl transcripts 2026-10-24] |

All cloned/fetched content was read as untrusted data; nothing in it was executed. The two scripts written for this harvest (`atlas_dump.py`, `cases_dump.py`) live in the scratchpad and only parse JSON.

---

## 1. What REA is

### 1.1 Provenance and canonical source

- REA is `morluto/rea` on GitHub, an MCP server and CLI published on npm as `rea-agents`; the upstream README names these three (GitHub repo, `rea.tools`, npm package) as the only official sources and warns that a look-alike repository name or a copied README does not make a download official. [sourced: UP/README.md:29-33]
- Upstream one-line pitch: "One MCP for reverse engineering across binaries, applications, and runtime behavior." [sourced: UP/README.md:8]
- REA connects an agent to tools for inspecting native binaries, JavaScript/Electron apps, .NET assemblies and websites; analysis runs locally; results include the evidence and limitations behind each conclusion. [sourced: UP/README.md:60-64]
- The npm package `rea-agents` was created 2026-07-12, has 33 published versions, `latest` = 6.7.0 (published 2026-10-18T08:12Z), single maintainer `morluto`, license MIT, repository `git+https://github.com/morluto/rea.git`. [sourced: UP/npm_rea-agents.json via registry.npmjs.org/rea-agents, queried 2026-10-24]
- The upstream CHANGELOG has 38 version headings from 0.2.0 (2026-07-12, "rename package and CLI to REA") to 6.7.0 (2026-10-18). [sourced: UP/CHANGELOG.md:3, :2637-2643]
- The GitHub page showed 378k stars and 94.4k forks when fetched 2026-10-24 (the AW list, written 2026-10-14..16, says "200k+ stars"). [sourced: https://github.com/morluto/rea fetched 2026-10-24; AW/README.md:9]
- Version milestones relevant to the cluster: 4.0.0 on 2026-10-05 (the viral release; it also *removed* the controlled-replay, Node-characterization, finite replay-machine and process-replay tools), 4.1.0 on 2026-10-06 (the version used for the official DX-Ball, Notion and CTF showcases), 6.3.0 tagged 2026-10-09 / npm-published 2026-10-10T00:02Z, 6.4.0 on 2026-10-11, 6.5.0 on 2026-10-13, 6.7.0 on 2026-10-18 (6.6.0 was tagged but never published to npm). [sourced: UP/CHANGELOG.md:1396,:1403,:1260,:268,:204,:91,:3-19; UP/npm_rea-agents.json; UP/website_evidence_dx-ball-sound-pan.md:12]
- The real 6.3.0 entry contains the items the RG repo paraphrases: streaming JSON inputs beyond the whole-document string limit, streaming JSON results at the command boundary, recording Electron integrity mismatches, preserving WebMCP declarations, and pinning packaged skill commands to the shipping package version. [sourced: UP/CHANGELOG.md:307,:308,:285,:298,:356-357]
- The author posts as `@morluto`; `@reatools` is the official account; the 6.4 release post is linked from the AW press page. [sourced: AW/docs/press-and-posts.md:49-50]
- N0zoM1z0 (author of the DX-Ball and Touhou reconstructions) is described as a co-builder of REA, so the flagship DX-Ball showcase is a first-party case. [sourced: AW/README.md:62,:71]

### 1.2 License, cost, what it is and is not

- License: MIT for REA-authored code; third-party material keeps its own licenses (`THIRD_PARTY_NOTICES.md`). [sourced: UP/README.md:412-418]
- Free and open source: install is `npx rea-agents setup` or `npm install --global rea-agents`; no account, key or payment is mentioned anywhere in the upstream README. [sourced: UP/README.md:25,:69-130]
- REA does not decompile by itself: deep native analysis requires one of Hopper, Ghidra or IDA; static JavaScript and .NET inspection need no native engine; setup can install Hopper after approval, Ghidra/IDA use existing installs. [sourced: UP/README.md:296-300 (FAQ entry on whether Hopper, Ghidra or IDA is needed); AW/README.md:9]
- REA grants no licence, key or activation entitlement for the commercial tools it drives (IDA, Hopper, JEB must be licensed by the user). [sourced: https://github.com/morluto/rea README fetched 2026-10-24; AW/README.md:195]
- What it returns: native analysis returns pseudocode and assembly; JavaScript/Electron analysis recovers modules and relationships; the agent writes and tests an implementation from those findings. [sourced: UP/README.md:325-329]
- Local-only claim with a caveat: REA analyses targets locally, but the agent receives the tool results and the model provider's own data policy applies to them. [sourced: UP/README.md:334-337]
- Not a sandbox: SECURITY.md states it is "not a sandbox" and gives no protection against malicious processes already running as the same OS user; opening an untrusted binary delegates parsing to the local provider with the user's permissions. [sourced: UP/SECURITY.md:33-36]
- Target content is treated as untrusted data: results carry `_meta.provenance = { untrusted_content: true, source: analysis_target }` and server instructions tell the agent never to follow directives found in tool results. [sourced: UP/docs_mcp-contracts.md:328-345]
- Permitted-use position (DISCLAIMER.md): security research, CTFs, interoperability, preservation, auditing software you own; explicitly refuses DRM/licence-check circumvention, cheats, copying proprietary software or reproducing its protected code and assets, and evading AI-provider policies. [sourced: UP/DISCLAIMER.md:5-19]
- Results disclaimer: "Decompilation, reconstruction, heuristics, and AI-assisted analysis may be incomplete or incorrect." [sourced: UP/DISCLAIMER.md:63-66]
- External tools REA drives or relies on (from the What-you-can-analyze table): Hopper, Ghidra, IDA, JADX, JEB (via `REA_JEB_MCP_URL`), Apktool, Binwalk/Unblob, pwntools, GDB/pwndbg, TShark/capinfos, sigrok-cli, mitmdump, objdump/Rizin, WABT, adb, Kuna, a Chrome-family browser. [sourced: UP/README.md:183-220]
- REA calls Ghidra's `analyzeHeadless` directly (so GhidraMCP is a peer, not a dependency); the IDA Pro profile uses `mrexodia/ida-pro-mcp` and IDA Free uses `0xshlomil/ida-free-mcp`. [sourced: AW/README.md:154-156]
- Ghidra sessions run a packaged headless script in an isolated temporary project that is deleted on close; "REA never falls back automatically to Hopper". [sourced: UP/SECURITY.md:38-43; UP/docs_native-investigation.md:1007-1008]

### 1.3 Credibility assessment of the six-repo cluster

Overall: the canonical tool is real, large, actively released and MIT; the GitHub topic `reverse-engineer-anything` (10 repos on 2026-10-24: three lists, six code repos, one undescribed) is a mix of two serious curated catalogues, one serious reimplementation, one small real tool, one tagging map, and one decoy/lure repository. [sourced: https://github.com/topics/reverse-engineer-anything fetched 2026-10-24] [inferred: the "mix" summary]

| Repo | What it is | Credibility signals | Verdict |
|---|---|---|---|
| `AW/` awesome-reverse-engineer-anything | Curated list, CC0, EN+zh-CN | States that it is unofficial and not affiliated with morluto/rea; defines evidence tags `REA` / `AI-RE` / `Rebuild` / `Claim?`; per-case claim check in `docs/is-it-rea.md`; daily discovery workflow that only opens an issue (nothing auto-added); records controversy and a warning about fake download repositories. [sourced: AW/README.md:7,:13-18,:188-201; AW/docs/is-it-rea.md:5-36; AW/.github/workflows/discover-candidates.yml:1-3,:27-34; AW/LICENSE:1-3] | Credible curation; no code; 150 stars on the topic page. [inferred] |
| `AR/` awesome-rea | 34 source-reviewed case files generated from `data/cases.json`, 12 language editions, Cloudflare Pages site | Pins every source to a commit SHA, dates each review, states "Sources reviewed; author tests not independently rerun", excludes "Claude/Codex with IDA/Ghidra" as proof of REA use, keeps a Chinese research log of rejected leads; also keeps an explicit SEO/discoverability playbook (`DISCOVERABILITY.md`) and a GEO-style topic strategy. [sourced: AR/README.md:13-15,:100,:106; AR/SOURCES.md:9-13,:23,:76; AR/research/2026-10-21.md:44-49; AR/DISCOVERABILITY.md:5-12,:44-50; AR/CHANGELOG.md:8] | Credible, deliberately discoverability-optimised; 420 generated HTML pages inflate the file count (887 tracked files) but the English case data is 34 entries. [inferred] |
| `RG/` REA-Github | Claims to be REA Reverse Engineer Anything 6.3.0 with Hopper/Ghidra/IDA hooks | README line 3 is keyword-stuffed ("rea github, github rea, rea mcp, morluto rea"); install step is to extract a `REA-6.3.0.zip` archive; all 20 so-called source files are 3-line stubs containing only a stamp, the file name and the folder name (60 lines total); `notes/USAGE.txt` reads "Extract. Run. See readme."; README says copyright reawin while LICENSE says `zgdczrgwj2xi6ghi2e`; single commit "Add files via upload"; `Screenshots/images5.png` is a Rust stack-and-heap visualization video thumbnail unrelated to REA; `images4.png` reproduces the upstream REA-investigation-flow figure; `images2.png` is a Hopper window on `/usr/bin/true`. The what's-new bullets are paraphrases of the real 6.3.0 changelog. [sourced: RG/README.md:3,:7-12,:25-30,:43; RG/LICENSE:3; RG/agent/Pad.cpp:1-3 and the other 19 stubs; RG/notes/USAGE.txt:1-4; RG/notes/VERSION.json:1-6; RG/Screenshots/images2.png, images4.png, images5.png (viewed); UP/CHANGELOG.md:285,:298,:307-308,:356-357; UP/README.md:172] | Matches the AW warning about GitHub repositories that offer an official-looking free zip download of REA; treat as a decoy/lure, not a source. [sourced: AW/README.md:201] [inferred: decoy] |
| `RF/` ReaFuse | v0.1 Node CLI + MCP server + web lab that pins `rea-agents 6.4.0` and shells out to `rea analyze-javascript-application` | Real code (~3,000 lines), 22 tests, two audited showcase runs with evidence IDs and SHA-256 report certificates; states it is independent and not endorsed by REA; listed by AR as a case and by AT as `Yes`. [sourced: RF/package.json:35; RF/src/rea.mjs:24-29; RF/README.md:19,:295; RF/showcase/ratewise/REPORT.md:73-77; AR/cases/reafuse.md:36; AT/data.json:1141] | Credible small project; its clean-room label is a tool policy, not a legal finding (AR says so). [sourced: AR/cases/reafuse.md:26] |
| `AT/` rea-atlas | 167-entry map (mostly classic game decomps) tagged `Uses rea: Yes/No/Unverified`, CC0 | 46 Yes / 12 Unverified / 109 No; "Yes only with public evidence"; cross-checked against AW; one PR merged 2026-10-24 adding 8 entries. [sourced: AT/data.json (parsed with scratchpad `atlas_dump.py`: totals Yes 46, Unverified 12, No 109); AT/README.md:5,:421; AT/CONTRIBUTING.md:2; git log of AT] | Credible tagging layer; most of its content is unrelated to REA (109 tagged No). [inferred] |
| `OS/` open-scrantic | Rust reimplementation of the 1992 *Johnny Castaway* screensaver engine, MIT, v0.1.0 (2026-10-17) to v0.4.0 (2026-10-21) | Full engine code with 49 original-address citations in `src/`, headless deterministic snapshot harness, CI/release workflow, AI disclosure naming Claude Code + REA; a commit titled "Credit REA for decompilation". [sourced: OS/README.md:268-276; OS/CHANGELOG.md:3,:132,:148; grep count of `seg\d+:[0-9a-f]{4}` / `DS:xxxx` in OS/src = 49] | Credible first-party reconstruction write-up; strongest documented method in the cluster. [inferred] |

Cluster-level observations:
- All community repos in the cluster were created or first committed between 2026-10-09 and 2026-10-24, i.e. within three weeks of REA 4.0.0 (2026-10-05) going viral (Matthew Berman post, #1 on GitHub Trending, 25k stars in a day). [sourced: AW/docs/new-this-week.md:9-21; AW/docs/press-and-posts.md:19-22; UP/CHANGELOG.md:1396]
- A skeptical counterpoint is on record: REA is "not a new reverse-engineering technique, only a way for agents to call existing tools". [sourced: AW/docs/press-and-posts.md:65]
- The only controlled measurement in the cluster (`pablodiazjorge/rea-lab`, five preregistered studies pinned to REA 6.4.0) found no significant success-rate difference with vs. without REA (28/30 vs 28/30; 20/20 vs 19/20), roughly double the time and higher cost with REA, and that none of 339 REA-calling runs proactively retained Evidence. [sourced: AR/cases/rea-lab.md:22-26; AW/README.md:140]
- AW explicitly notes critics' objection that one agent doing both analysis and rebuild "does not meet the personnel separation of traditional clean-room development". [sourced: AW/README.md:192]
- Multiple same-week DeepSeek Harness adapter repos (`dsh-rea`, `rea-dsh-plugin`, `dsh-rea-mcp`, `dsh-plugin-rea`) that only register the REA MCP server are listed as plugins; they add no analysis capability. [sourced: AW/README.md:123-125,:134]

---

## 2. MCP tool surface (upstream `morluto/rea`)

The full catalog is generated (`docs/public/product-catalog.json`), not checked in; `tools/list` advertises only operations callable for the current target/provider/host, and `binary_session` is "the authoritative source of catalog identity and tool availability". [sourced: UP/docs_mcp-contracts.md:1372-1383; UP/docs_mcp-contracts.md:567-689 (Identity and discovery) as summarised by WebFetch 2026-10-24]
A community test reports 206 tool descriptions in a historical PR build and a 10-tool allowlist (open target, locate, decompile, close) sufficient for a local 27B model. [sourced: AR/cases/local-model-mcp.md:18-22]
Tool names below are those that appear in the fetched upstream docs (counts are occurrences across `UP/docs_*.md`); descriptions are from the cited lines.

| Tool (MCP name) | What it does | Source |
|---|---|---|
| `binary_session` | Reports active package/server/SDK/protocol; `tool_availability` lists advertised and hidden operations with reasons; `analysis_provider_candidates` lists deep-engine providers. | [sourced: UP/docs_mcp-contracts.md:567-689 (Identity and discovery); WebFetch summary 2026-10-24] |
| `open_binary` / `close_binary` | Opens a native target with `provider_id` (`auto` or a concrete provider: Hopper/Ghidra/IDA); closes the session with cleanup diagnostics. | [sourced: UP/docs_native-investigation.md:30-31; UP/docs_mcp-contracts.md:349-350] |
| `list_procedures`, `list_names`, `list_strings`, `list_segments`, `list_architectures`, `search_procedures`, `search_strings`, `search_bytes`, `read_bytes`, `xrefs` | Native inventory and primitives: procedures, symbols, strings, segments, cross-references, raw byte reads. | [sourced: UP/docs_native-investigation.md:18-31,:54; UP/website_evidence_ctf-masked-squares.md:36-43; UP/website_evidence_dx-ball-sound-pan.md:50-52] |
| `analyze_function` | Returns a function "dossier" (pseudocode, assembly, instruction view, callers/callees); by default retains the complete dossier in-session and returns an `inspect_analysis_view` summary whose `parent_evidence_id` names it. | [sourced: UP/docs_mcp-contracts.md:16-27; UP/website_evidence_dx-ball-sound-pan.md:45-49] |
| `inspect_analysis_view` | Pages a retained or saved Evidence record (summary, module page, javascript-facts, etc.); a 334 MB Obsidian evidence file yields a ~10 KB summary view. | [sourced: UP/docs_cli.md:25-52] |
| `compare_functions`, `match_native_functions`, `build_call_path`, `trace_call_path`, `trace_native_values`, `get_call_graph`, `resolve_containing_procedure`, `inspect_address_context`, `get_navigation_context` | Function comparison, bounded static traces, value flow, call graph, address-to-procedure resolution, aggregate native context. | [sourced: UP/docs_mcp-contracts.md:16-27,:1068-1081; UP/docs_native-investigation.md:328,:1068; UP/website_evidence_ctf-masked-squares.md:38] |
| `decompile_all`, `inspect_native_instruction`, `inspect_native_dispatch_metadata`, `get_objc_classes`, `trace_dylib_resolution`, `inspect_macho`, `inspect_plist`, `inspect_keyed_archive`, `inspect_asset_catalog`, `inspect_signature`, `inspect_binary_layout`, `open_bundle_resources`, `extract_artifact`, `inspect_artifact`, `compare_artifacts` | Static inspection of binaries, Apple bundles, dispatch metadata, signatures, archives; artifact inventory and comparison with `coverage.scope`/`coverage.status`. | [sourced: UP/docs_native-investigation.md:416; UP/docs_mcp-contracts.md:68-80; AR/cases/adamterm.md:18] |
| `annotate_native_function`, `set_address_name`, `set_addresses_names` | Rename/comment in the ephemeral Ghidra/Hopper database without changing bytes. | [sourced: UP/docs_native-investigation.md:1001-1020] |
| `observe_native_ui`, `observe_native_calls`, `trace_native_ui_action`, `capture_native_ui_scenario`, `capture_process_scenario` | Runtime observation: desktop UI capture/actions, call observation, process launch with input and retained terminal output and exit code. | [sourced: UP/docs_native-investigation.md:850,:894; UP/website_evidence_ctf-masked-squares.md:94-106] |
| `analyze_javascript_application` | Static analysis of an extracted JS/Electron directory or ASAR: modules, exports, imports, source maps, IPC, context bridges; returns result + Evidence inline; can be hundreds of MB. | [sourced: UP/docs_cli.md:18-27; UP/website_evidence_notion-clipboard.md:16-24; RF/src/rea.mjs:24-28] |
| `trace_javascript_semantics`, `trace_feature`, `trace_application_feature`, `compare_application_versions`, `compare_javascript_export_shapes`, `compare_source_to_bundle`, `analyze_web_bundle`, `export_web_scripts` | JS feature traces from a seed (API name or IPC channel), version diffs, export-shape diffs, historical source-to-bundle mapping (`unchanged/modified/removed/split/merged/duplicated/unknown`). | [sourced: UP/docs_javascript-application-workflows.md:6-7,:371-384; UP/website_evidence_notion-clipboard.md:38-47] |
| `list_browser_targets`, `inspect_web_page`, `observe_web_session`, `capture_browser_scenario`, `capture_web_screenshot`, `compare_web_captures`, `compare_web_screenshots` | Website mode: page structure, scripts, network observations, screenshots and capture diffs via a Chrome-family browser; the only *passive* capture path a community case used on public pages. | [sourced: UP/README.md:205; UP/docs_browser-observation.md; AR/cases/terminal-rs.md:18] |
| `inspect_wasm_artifact`, `inspect_apk_signature`, `inspect_adb_client`, `inspect_apktool_client` | WASM sections/WAT (needs WABT), APK signer/cert checks, adb/apktool client probes. | [sourced: UP/README.md:200,:209,:210,:212] |
| `record_unknown`, `update_unknown`, `list_unknowns` | An explicit **unknowns ledger**: residual questions retained with their immutable result Evidence ID, revisioned (`expected_revision`, `revision_conflict`), pageable. | [sourced: UP/docs_mcp-contracts.md:270-290] |
| `get_evidence_bundle`, `export_evidence_bundle` | Retained session history or a portable authenticated bundle (`rea inspect-evidence-bundle` on the CLI). | [sourced: UP/docs_mcp-contracts.md:26-27,:292-296] |
| `evaluate_reconstruction_coverage`, `build_reconstruction_obligation_ledger` | Reconstruction-claim tooling: an `overall` structural-function claim selects every constituent function dimension together with its active scoped unknowns; unknown citations and recommended probes are retained. | [sourced: UP/docs_mcp-contracts.md:1096-1100,:1249-1251] |

Evidence envelope (the contract every tool above returns):
- Evidence-producing tools return `{ result, evidence_id, evidence }`; `evidence.normalized_result` equals `result`; `raw_result` holds the provider's own representation or `null`; `analysis_profile` is mandatory and part of semantic identity. [sourced: UP/docs_mcp-contracts.md:16-23]
- Evidence IDs are content hashes of the form `ev_<64 hex>`, and showcase pages cite one per request. [sourced: UP/website_evidence_dx-ball-sound-pan.md:13-16; RF/showcase/ratewise/lab/A.rea-summary.json:3]
- JS evidence is a graph: nodes carry `observations[].properties` with `semantic_role` (e.g. `source-module`), `path`, `module_format`, `export_names`; edges carry `relation` (e.g. `imports`) with `specifier`/`resolved_path`; plus `statistics` and `limitations`. [sourced: RF/src/rea.mjs:52-94; RF/showcase/ratewise/lab/A.rea-summary.json:4-16]
- Limitations are explicit strings, e.g. "Static imports, entrypoints, workers, endpoints, and storage relationships do not prove runtime execution." [sourced: RF/showcase/ratewise/lab/A.rea-summary.json:113-119]
- Community reports describe Evidence labelled observed vs derived (bzip2 case) and `proven / inferred / hidden` claim marking (Wispr Flow case). [sourced: AR/cases/bzip2-groundtruth.md:18; AR/cases/wispr-flow.md:18]
- Artifact integrity fails closed by default; `integrity_policy=record-and-continue` quarantines contradictory bytes and records declared vs observed hashes. [sourced: UP/docs_mcp-contracts.md:1102-1110]
- CLI output: `--json` for a whole document, `--format jsonl` for row-oriented records (one per line, with a control record); saved Evidence files are re-read and authenticated rather than charged as inline input. [sourced: UP/docs_cli.md:25-40,:54-80]
- Transport: stdio MCP by default; serving MCP over streamable HTTP is a documented CLI mode (heading at that line). [sourced: UP/docs_cli.md:725]

---

## 3. Method: stage table

Assembled from the upstream investigation flow, the official showcase evidence pages, open-scrantic's process docs, ReaFuse's pipeline and the AR case files. Verbs are the sources' own where possible.

| # | Stage | Verb(s) | Inputs | Outputs | Verification at this stage | Source |
|---|---|---|---|---|---|---|
| 0 | Fix the target | pin, hash, reject other builds | Local artifact (exe, ASAR, APK, ELF, resource files) | Target SHA-256, size, analysis-profile digest; refusal of unknown builds | Digest checked before and after analysis; loader validates known strings and rejects other builds | [sourced: UP/website_evidence_ctf-masked-squares.md:10-21; UP/website_evidence_dx-ball-sound-pan.md:9-11; OS/README.md:108-116,:168; OS/src/assets/CLAUDE.md:3-7] |
| 1 | Ask | ask a concrete question | Agent + local target + one observable behaviour (e.g. how an on-screen position becomes a sound-pan value) | A bounded question; the AR catalogue requires a concrete question per case | — | [sourced: UP/README.md:165-172; AR/README.md:100; AR/cases/dx-ball.md:14] |
| 2a | Dissect, static (native) | search strings, xrefs, resolve containing procedure, analyze function, read bytes, follow callers | Opened binary via Hopper/Ghidra/IDA | Function dossiers (pseudocode + assembly + callers), constants decoded from raw bytes, each with an `ev_` id | Dossier instruction listing transcribed verbatim; constants decoded from observed hex, with decoded values flagged as decodings rather than fields of the raw REA result | [sourced: UP/website_evidence_ctf-masked-squares.md:34-43; UP/website_evidence_dx-ball-sound-pan.md:25-57] |
| 2b | Dissect, static (JS/Electron) | analyze-javascript-application, trace feature from seed, read bundled code | Extracted app dir or ASAR | Module graph (modules/exports/imports/IPC/context-bridge findings with line:column ranges), feature-trace graphs, statistics, limitations | Independent source build vs published package compared ("returned the same API and invoke findings"); unresolved traces reported as unresolved, not as the route | [sourced: UP/website_evidence_notion-clipboard.md:9-47; RF/src/rea.mjs:20-46] |
| 2c | Dissect, static (packaging/metadata) | inspect Mach-O, plists, signatures, Objective-C metadata, Interface Builder, dylib resolution | App bundle | Architecture answers "before deep decompilation" | — | [sourced: AR/cases/adamterm.md:18,:30] |
| 2d | Dissect, dynamic | capture_process_scenario, observe UI/calls, browser capture | Original program + chosen inputs | Terminal output, exit codes, filesystem/network observations, screenshots, each with `ev_` id | Correct and incorrect inputs both run through the original; cleanup verified | [sourced: UP/website_evidence_ctf-masked-squares.md:94-106; UP/README.md:205-219] |
| 2e | Dissect, protocol | REA + Ghidra on vendor libraries / client DLLs; traffic comparison; companion-APK entry points | Shared libraries, client binaries, captures | Protocol and parameter specs; handshake differences; unknown control codes left unknown | Reported end-to-end connectivity against real gateways/devices, scoped by version | [sourced: AR/cases/vsl-dsp.md:18-30; AR/cases/v5vpn-rea-client.md:18-26; AR/cases/picoprint.md:18-30] |
| 2f | Dissect, UI/web | REA website mode passive capture of public pages | Public logged-out pages | Findings, Evidence IDs and design implications in `docs/analysis` | Explicitly not a recovery of TradingView or Bloomberg source code | [sourced: AR/cases/terminal-rs.md:18-30] |
| 3 | Read the evidence | interpret observations, label | Dossiers, byte reads, graphs | A table of *saved observation → interpretation* ; names assigned by the maintainer (e.g. `dxball_pan_scale`) kept separate from the executable's addresses | Each interpretation row cites the observation it rests on | [sourced: UP/website_evidence_dx-ball-sound-pan.md:23-39; UP/README.md:172 (figure alt text: code, references and unknowns)] |
| 4 | Induce the behaviour spec | write address-backed behaviour specifications; write spec cards (behaviour only, never code); record formats, VM semantics, story rules, timing | Evidence + README + tests (+ disassembly) | Spec with exact input/output examples precise enough to become tests; `docs/ENGINE.md`-style notes with `segment:offset` for every rule; approximations labelled | ReaFuse schema forces `examples[{input,output}]`, `edgeCases`, `limitations`, `evidence` refs per capability | [sourced: AR/cases/ski-or-die.md:30; RF/src/prompts.mjs:10-41; RF/src/schemas.mjs:8-28; OS/docs/ENGINE.md:5-6,:18-33; OS/CONTRIBUTING.md:32] |
| 5 | Record unknowns | record_unknown / list_unknowns; mark proven / inferred / hidden; "Unknown control codes should remain unknown" | Residual questions | Revisioned unknowns ledger tied to result Evidence IDs; `unknown_members`, `unresolved` fields in results | Zero findings never read as absence (rea-groundtruth) | [sourced: UP/docs_mcp-contracts.md:270-290; AR/cases/wispr-flow.md:18; AR/cases/vsl-dsp.md:30; AR/cases/rea-groundtruth.md:30; UP/website_evidence_notion-clipboard.md:23] |
| 6 | Rebuild (clean-room) | grow fresh code from the spec; port logic citing original addresses; keep original integer widths and RNG | Spec cards / ENGINE notes; the lab-owned contract tests | New codebase that reads the user's own original assets at runtime; code comments cite `seg10:1acb`-style addresses | See §4 (allowed-in / excluded evidence) | [sourced: RF/src/prompts.mjs:86-103; OS/CONTRIBUTING.md:30-35; OS/docs/ARCHITECTURE.md:11-23] |
| 7 | Verify | replay, differential test, compiler replay, contract tests in sandbox, snapshot render, source cross-check | Rebuild + original | Pass/fail counts scoped to one function/feature; byte-match counts; verdict (`ALIVE/UNSTABLE/DEAD/REJECTED`) | See §5 | [sourced: UP/website_evidence_dx-ball-sound-pan.md:97-108; RF/src/verify.mjs:81-89] |
| 8 | Iterate | follow-up question; bounded repair rounds that cannot touch the contract; port more logic one address at a time; document deliberate deviations | Failing tests / open unknowns | Repaired files; new ENGINE.md deliberate-differences entries | Re-verify after each repair; max 2 repairs by default | [sourced: UP/README.md:172 (follow-up-question loop in the figure); RF/src/pipeline.mjs:205-228; RF/src/prompts.mjs:106-117; OS/docs/ENGINE.md:150-181; OS/CONTRIBUTING.md:35] |
| 9 | Report / attribute | write REPORT.md + report.json with donors, commits, licences, evidence IDs, test results, SHA-256 certificate; separate REA's stage from other tools | Run folder | Auditable, replayable run; AR insists "Attribute only the stage REA actually handled" | Certificate = sha256 of the report body | [sourced: RF/README.md:152,:177; RF/src/report.mjs:69; AR/CONTRIBUTING.md:22; AR/cases/openbattlefield2.md:18] |

## 4. Clean-room rules observed in the sources (what evidence is allowed in, what is excluded)

Allowed in:
- Observable behaviour: what donors do (behaviour, inputs, outputs, ideas), learned from REA evidence, READMEs and tests. [sourced: RF/src/prompts.mjs:3-5; RF/README.md:146]
- Disassembly as ground truth; original arithmetic (wrapping `u16`/`i16`) and the original RNG; original address citations in comments. [sourced: OS/AGENTS.md:12-15,:38-40; OS/docs/ARCHITECTURE.md:11-18]
- Public standard test vectors (RFC 6238 vectors for TOTP) to constrain the rebuild. [sourced: AR/cases/aegis-totp.md:22,:30]
- Public web observations, labelled as such and not described as binary decompilation. [sourced: AR/cases/terminal-rs.md:30]
- Donor licence, commit hash and REA evidence id recorded per donor; permissive licences pass, copyleft/unknown are recorded with a warning unless `--strict-license` refuses them. [sourced: RF/src/license.mjs:4-5,:68-80; RF/showcase/ratewise/REPORT.md:5-10]

Excluded:
- Copied code, comments, identifiers beyond common words, or README text; a deterministic fingerprint scan (20-token shingles, max 60-token shared run, max 15% overlap, comments/whitespace stripped) turns the verdict into `REJECTED`. [sourced: RF/src/prompts.mjs:5-6; RF/src/originality.mjs:4-15,:57-64]
- Donor execution: donors are parsed as inert text, cloned shallowly with symlinks disabled; only freshly grown code runs. [sourced: RF/SECURITY.md:7; RF/src/verify.mjs:6-10]
- Original assets and data: never committed; read at runtime from the user's own copy; "No original data in the binary"; AW/AT/AR exclude projects distributing copyrighted assets. [sourced: OS/AGENTS.md:10; OS/docs/ARCHITECTURE.md:14-16; AW/README.md:199; AT/CONTRIBUTING.md:4; AR/CONTRIBUTING.md:24]
- Third-party reimplementations as evidence: "only hints". [sourced: OS/AGENTS.md:13]
- Closed-source products as direct donors: ReaFuse resolves an app name to open-source "twins" instead. [sourced: RF/src/prompts.mjs:120-129; RF/src/pipeline.mjs:277-336]
- The acceptance contract itself: grow/repair steps cannot write under `test/contract/`; attempts are logged and ignored, and the contract is reinstalled after growing. [sourced: RF/src/pipeline.mjs:368-377,:202; RF/SECURITY.md:9]
- Counter-example recorded: DiskLens ports some GPL source directly, so AR states it "is not a clean-room reconstruction of closed-source software". [sourced: AR/cases/disklens.md:26]
- Honest limit: the same workflow produces both specs and implementations, and critics say one agent doing both does not meet traditional clean-room personnel separation. [sourced: AR/cases/reafuse.md:26; AW/README.md:192]

---

## 5. Verification techniques (catalogue)

| Technique | How it was applied | Scope caveat stated by the source | Source |
|---|---|---|---|
| Differential execution against the original function | 3,205 original-x86 cases (positions 0–640 at five scales) vs the C reconstruction | One function at one checkpoint; "Game-level integration remains a separate project task" | [sourced: UP/website_evidence_dx-ball-sound-pan.md:99-108] |
| Compiler replay / byte matching | VC4.0 recompilation reproduces all 63 bytes after relocation review; TH04: two cold Turbo C++ 4.0J builds reproduce a 2,139-byte extent with `raw_exact`, `map_exact`, `relocations_exact` receipts, receipt SHA-256 recorded | "not a whole-executable or full-game equivalence claim" | [sourced: UP/website_evidence_dx-ball-sound-pan.md:103-104; UP/website_evidence_th04-bullet-ring.md:100-121] |
| Semantic tests recorded separately from byte matching | AR takeaway for DX-Ball: "Record semantic tests separately from byte matching"; dx-ball keeps a `semantic-acceptance.csv` ledger and `match-units.toml` | — | [sourced: AR/cases/dx-ball.md:30; UP/website_evidence_dx-ball-sound-pan.md:114-115] |
| Golden-output replay on the original program | `capture_process_scenario` with the recovered flag → `Correct!`/exit 0, and a one-character mutation → `Incorrect!`/exit 255 | Solver assumed flag format; uniqueness not proved (second-model search timed out) | [sourced: UP/website_evidence_ctf-masked-squares.md:94-106,:74-85; AR/cases/ductf-masked-squares.md:26-30] |
| Source cross-check after the fact | Organizers' source confirmed masks/targets *after* the candidate was obtained; hexgrab checked each REA finding against public PowerToys source (8 of 9 fully correct) | 8/9 is one investigation, "not a general REA accuracy rate" | [sourced: UP/website_evidence_ctf-masked-squares.md:87-92; AR/cases/hexgrab.md:18-26] |
| Ground truth from a symbolized twin build | Stripped bzip2 analysed; function, 1 caller and 24 callees matched the unstripped build, `nm` and `objdump` | 24 edges are provider-resolved direct calls, not a runtime graph | [sourced: AR/cases/bzip2-groundtruth.md:18-26] |
| Analyzer-coverage controls | Minimal Electron fixtures with different webpack wrappers reveal APIs REA missed; "Zero findings do not establish absence" | Specific builds, REA 6.4.0 | [sourced: AR/cases/rea-groundtruth.md:18-30] |
| Falsifiable predictions before reading answers | REA analysed its own package; agent wrote predictions, ran `doctor`, then graded itself: four answers partly correct, 3 of 8 predictions wrong | Single self-assessment | [sourced: AR/cases/rea-on-rea.md:18-30] |
| Independent probe of extracted modules | Node/JSDOM probes of lifted Notion modules show rich-text markers and Markdown tables | Not proof of OS clipboard hydration; web and desktop samples are different versions | [sourced: UP/website_evidence_notion-clipboard.md:93-98; AR/cases/notion-clipboard.md:26] |
| Two providers on the same image + classic tools | REA pwntools layout + Ghidra path, cross-checked with readelf/objdump/nm/strings; raw JSON kept under `files/` | Static only; not a security certification | [sourced: AR/cases/netcat-analysis.md:18-30] |
| Contract tests written before code, sandboxed | 10–20 deterministic `node:test` tests importing only `../../src/index.mjs`, each mapped to origin A/B/fusion; run with `node --permission --allow-fs-read=<creature>` (no writes, no child processes); missing tests counted as failures; verdict thresholds 100% = ALIVE, ≥50% = UNSTABLE, else DEAD | Node permission model does not block network on 22/24 | [sourced: RF/src/prompts.mjs:61-83; RF/src/verify.mjs:11-24,:67-89; RF/SECURITY.md:10,:14] |
| Originality (clean-room) scan | SHA-1 fingerprints of 20-token windows vs every donor source file; ratewise/objdelta: 0.0% overlap, longest run 0 | Passing scans "do not establish general originality" | [sourced: RF/src/originality.mjs:17-64; RF/showcase/ratewise/REPORT.md:67-71; AR/cases/reafuse.md:26] |
| Deterministic headless snapshot | `examples/snapshot.rs` runs the engine with injected clock, fixed seed, story day, time and date and writes PNGs, which the project names as its way to verify rendering changes; data-dependent tests run only when `OPEN_SCRANTIC_DATA` is set | Data-dependent tests pass silently without the original files, so a green run must be checked | [sourced: OS/AGENTS.md:29-36; OS/CONTRIBUTING.md:22-28,:37-47; OS/docs/ARCHITECTURE.md:265-273] |
| Address-cited quirk preservation | Behaviours that look like bugs are kept because they "match the disassembly (checked against seg3, seg4 and seg12)"; deliberate deviations listed separately | — | [sourced: OS/docs/ENGINE.md:135-148,:150-181] |
| Evidence-ID citation discipline | Function notes list GuiIndex/position addresses *and* REA Evidence IDs; a Claude Code mod (`rea-casebook`) refuses conclusions that do not cite real `evidence_id`s | OpenBattlefield2 results "involve multiple tools and cannot all be attributed to REA" | [sourced: AR/cases/openbattlefield2.md:18-30; AW/README.md:127] |
| Layered acceptance outcomes | Co-op Anything: validate "client connection, synchronized visibility and genuinely playable co-op as separate outcomes"; Pixel Watch: separate opening the app, delivering config, installation, connectivity; VD streamer: simulated-device tests vs real-hardware acceptance | — | [sourced: AR/cases/coop-anything.md:24; AR/cases/watch-esim-helper.md:30; AR/cases/vd-streamer-linux.md:30] |
| Controlled A/B study | Preregistered tasks with/without REA; grader verifies cited Evidence (Study 3: 144/144 after a grader fix) | Pinned versions; differences not significant | [sourced: AR/cases/rea-lab.md:18-26] |

---

## 6. ReaFuse fusion loop (`dissect two → fuse → verify`)

Pipeline string from the README: `find → dissect → fuse → contract → grow → verify → originality → report`. [sourced: RF/README.md:138-154]

1. **Intake / find.** App names or websites are resolved to open-source "twins" by one GitHub search per app ranked `0.5·similarity + 0.2·relevance + 0.3·compatibility` (compatibility = JS/TS language, size < 30 MB, permissive licence, not archived); repo URLs and local folders skip this. [sourced: RF/src/twins.mjs:38-81; RF/src/pipeline.mjs:277-336]
2. **Licence gate.** Each donor's SPDX licence is detected; permissive passes, copyleft/unknown is recorded with a warning (or refused under `--strict-license`); a failed gate yields verdict `REJECTED` before any analysis. [sourced: RF/src/license.mjs:68-80; RF/src/pipeline.mjs:100-106]
3. **Dissect with REA.** `rea analyze-javascript-application <donor> --json` (5-minute timeout, 200 MB output cap); a non-zero exit with a usable `normalized_result.graph` is kept as `partial`; the graph is condensed to ≤150 modules with exports/internal/external imports plus `evidenceId`, `statistics`, `limitations`; full evidence stays on disk and is cached per donor commit. [sourced: RF/src/rea.mjs:24-46,:52-94; RF/src/pipeline.mjs:108-137]
4. **Spec card.** The brain writes a behaviour-only card: `capabilities[2-6]{id, behavior, inputs, outputs, edgeCases, examples[{input,output}]}`, `designIdeas`, `limitations`, `evidence[]` (file paths or REA evidence ids); prompt says every example "must be precise enough to become an automated test"; source excerpts are supplied for understanding only and must never be reproduced. [sourced: RF/src/schemas.mjs:8-28; RF/src/prompts.mjs:10-41; RF/showcase/ratewise/lab/spec-A.response.json:71]
5. **Fuse.** Exactly three proposals, each inheriting ≥1 capability from each donor plus one capability that exists only because the two donors were combined, deterministic (no network/clocks/randomness), zero runtime dependencies; a human or agent picks one (`reafuse_pick`). [sourced: RF/src/prompts.mjs:44-58; RF/src/schemas.mjs:30-53; RF/src/pipeline.mjs:156-177]
6. **Contract first.** 10–20 acceptance tests written *before* code, each mapped to origin `A | B | fusion` and capability ids; the creature "can't change them". [sourced: RF/src/prompts.mjs:61-83; RF/src/schemas.mjs:60-69; RF/README.md:148]
7. **Grow.** Fresh ES-module code + CLI + own tests + README from the proposal and contract (file-by-file "parts" mode for weak models); contract files are then installed over it. [sourced: RF/src/prompts.mjs:86-103; RF/src/pipeline.mjs:193-203,:338-361]
8. **Verify + repair.** Sandboxed test run; on failure the failing TAP blocks go back to the brain for up to `--max-repairs` (default 2) rounds; each attempt's counts are recorded. [sourced: RF/src/pipeline.mjs:205-228; RF/src/verify.mjs:11-48]
9. **Originality.** Fingerprint scan against every donor source file; breach → `REJECTED` regardless of tests. [sourced: RF/src/pipeline.mjs:230-236; RF/src/originality.mjs:26-64]
10. **Preview + report.** Optional single-file demo that must import `../src/index.mjs` and load nothing from the network; `REPORT.md`/`report.json` (schema `reafuse.lab-report/v1`) with donors, commits, licences, REA version/evidence id, contract lineage table, originality numbers, and a SHA-256 certificate of the report body. [sourced: RF/src/pipeline.mjs:238-263; RF/src/report.mjs:69; RF/showcase/ratewise/report.json:1-10; RF/showcase/ratewise/REPORT.md:44-77]
11. **Brains and replay.** Every reasoning stage is a `brain.ask({stage, prompt, schema})` whose answer is cached as `brain/<stage>.response.json`; `handoff` mode writes `<stage>.prompt.md` and waits for a response file, which is how the MCP server makes the calling agent the brain (`reafuse_fuse → reafuse_answer → reafuse_pick → reafuse_status`, plus `reafuse_dissect` for one donor). [sourced: RF/src/brain.mjs:109-110,:217; RF/README.md:124-132,:255-257; RF/skills/reafuse/SKILL.md:12-23]
12. Reported results: ratewise 18/18 contract (A 2, B 7, fusion 9) + 7/7 own, objdelta 17/17 + 6/6, both first attempt, both 0% overlap; REA 6.4.0; brain `handoff` with `claude-opus-5-5`. [sourced: RF/showcase/ratewise/REPORT.md:37-71; RF/showcase/objdelta/report.json:260-287; RF/showcase/ratewise/report.json:6-9]
13. Limits stated by the project: JS/TS donors only; library + CLI creatures only; v0.1 alpha; Node permission model does not restrict network; donor excerpts are sent to the configured model provider. [sourced: RF/README.md:336,:343; RF/SECURITY.md:14-15]

---

## 7. Case-study catalogue

Columns: title | target type | tool(s) used (as attributed by the source) | REA verified? (AR = awesome-rea source review; AT = rea-atlas tag; AW = awesome-REA claim check) | repo/source path.

### 7.1 Official showcases (REA team, rea.tools)

| Case | Target type | Tool used | Verified | Path |
|---|---|---|---|---|
| DX-Ball: sound-pan function | binary (Win32 PE i386 game) | REA 4.1.0 + Ghidra 12.1.4; VC4.0 compiler replay by the reconstruction project | AT Yes; AR official; AW `REA` confirmed; 3,205 cases + 63 bytes, one function | [sourced: AR/cases/dx-ball.md; AT/data.json:5; UP/website_evidence_dx-ball-sound-pan.md] |
| Notion: Electron clipboard bridge | app (Electron ASAR) | REA 4.1.0 `analyze_javascript_application` + manual bundle reading + JSDOM probes | AT Yes; AR official; REA graph did not resolve the full channel chain | [sourced: AR/cases/notion-clipboard.md; AT/data.json:13; UP/website_evidence_notion-clipboard.md] |
| TH04: PC-98 bullet-ring maths | binary (DOS MZ 16-bit game) | REA 4.1.0 + Ghidra; Turbo C++ 4.0J compiler checks from the TH04 project | AT Yes; AR official; AW: REA used only for the showcase inspection | [sourced: AR/cases/th04.md; AT/data.json:21; UP/website_evidence_th04-bullet-ring.md; AW/docs/is-it-rea.md:10] |
| Aegis: TOTP code generation | app (Android APK) | REA 6.1.0 Android analysis (JADX path) + browser demo; 133 cases + RFC 6238 vectors | AT Yes; AR official | [sourced: AR/cases/aegis-totp.md; AT/data.json:29] |
| DownUnderCTF masked-squares flag checker | binary (stripped Linux ELF, CTF) | REA 4.1.0 + Ghidra, Z3, `capture_process_scenario` | AT Yes; AR official | [sourced: AR/cases/ductf-masked-squares.md; AT/data.json:37; UP/website_evidence_ctf-masked-squares.md] |

### 7.2 Community cases with REA attribution in a primary source

| Case | Target type | Tool used | Verified | Path |
|---|---|---|---|---|
| N0zoM1z0/dx-ball reconstruction | game (Win32) | REA + Ghidra (author is a REA contributor) | AT Yes; AW confirmed, full game not finished | [sourced: AT/data.json:45; AW/docs/is-it-rea.md:9; AW/README.md:71] |
| N0zoM1z0/th04 reconstruction | game (PC-98) | agents + ReC98 base; REA via showcase only | AT Yes (via showcase); AW: REA used only for the showcase inspection, not the reconstruction | [sourced: AT/data.json:53; AW/docs/is-it-rea.md:10] |
| N0zoM1z0 th08/th09/th095/th10/th105/th08-web/factory | game (PC) | Codex/GPT + IDA MCP / Ghidra | AT Unverified; AW `AI-RE`: none of these READMEs mentions REA | [sourced: AT/data.json:61,:69,:1029-1061; AW/README.md:73] |
| ucodia/desktop-games-nostalgia | game (Win32 with embedded Peter interpreter) | Claude Code + REA + Ghidra, Python asset tools | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/desktop-games-nostalgia.md; AT/data.json:77] |
| tuunanet/rea-ski-or-die-reconstruction | game (DOS MZ) | REA CLI + Ghidra 16-bit adapter; Rust + SDL2 | AR reviewed; AT Yes; AW confirmed; no side-by-side with the running original | [sourced: AR/cases/ski-or-die.md; AT/data.json:85] |
| Moxnatiy/OpenBattlefield2 | game (Win32 PE client + DLLs) | REA 4.1.0 + Ghidra 12.1.4 dossiers, direct Ghidra, game data, server symbols | AR reviewed; AT Yes; AW: REA in CLAUDE.md/`tools/rea.sh`, not README | [sourced: AR/cases/openbattlefield2.md; AT/data.json:93; AW/docs/is-it-rea.md:24] |
| OpenRsG/GunZRs | game (packed Win32 + data archives) | REA + Ghidra (found Themida packing boundary); formats from data files | AR reviewed; AT Yes; AW confirmed with REA's role limited by the packing | [sourced: AR/cases/gunzrs.md; AT/data.json:101] |
| cranebeaker/coop-anything (C&C3, World in Conflict) | game (Win32) | REA/Ghidra 12.1.4 observations + scripts/debuggers | AR reviewed (721 stars); LAN-only verification | [sourced: AR/cases/coop-anything.md] |
| a-barzanti/ai-rebuilds | game (web rebuilds from REA-written specs) | REA for specs | AT Yes; AR: not yet included as a completed case | [sourced: AT/data.json:109; AR/research/2026-10-21.md:48] |
| michaelshimeles/paper-rs → Sketch | app (desktop design app behaviour) | REA + Claude Code; Rust wgpu/egui | AR reviewed; AT Yes; AW confirmed; author had not run on real macOS hardware | [sourced: AR/cases/paper-rs.md; AT/data.json:253] |
| michaelshimeles/rec-rs | app (OBS Studio architecture, GPL) | REA + Claude Code; "only the architecture was studied" | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/rec-rs.md; AT/data.json:261; AW/docs/is-it-rea.md:12] |
| michaelshimeles/terminal-rs | UI/web (public TradingView pages) | REA website mode passive capture | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/terminal-rs.md; AT/data.json:1117] |
| az9713/wispr-flow-rea-teardown | app (Electron + native helper) | REA CLI JS static analysis + Ghidra; claims marked proven/inferred/hidden; first two MCP calls timed out | AR reviewed; AT Yes; AW confirmed (follows a tutorial video) | [sourced: AR/cases/wispr-flow.md; AT/data.json:285] |
| iammukeshm/hexgrab | app (.NET PowerToys binaries) | REA; findings checked against public source (8/9) | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/hexgrab.md; AT/data.json:269] |
| AdamLinCodes/AdamTerm | app (macOS bundle, Ghostty) | REA Mach-O/plist/ObjC/IB inspection, no Hopper/Ghidra | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/adamterm.md; AT/data.json:277] |
| lxistired/ledger-reborn | app (QuickBooks Desktop for Mac) | REA; local SQLite engine | AR reviewed; AT Yes; AW confirmed via discussion #2198 | [sourced: AR/cases/ledger-reborn.md; AT/data.json:1109] |
| KarlssonChristoffer/DiskLens | app (WinDirStat 2.5.0 exe + GPL source) | Claude Code + REA + Ghidra; some GPL code ported directly | AR reviewed (not a clean-room reconstruction); AT Yes | [sourced: AR/cases/disklens.md; AT/data.json:1093] |
| payswapdotorg/hyper3d-clone | UI/web (captured JS bundles) | rea-agents 6.5.0 `analyze-javascript-application` on 21 files; backend not recovered | AR reviewed; 67 author-reported tests | [sourced: AR/cases/hyper3d-clone.md] |
| JimLiu/baocut (Claude Slides editor) | UI/web (deobfuscated front end) | REA per author's X post; one commit; source-available not OSS | AW: claimed by the author, not in the README | [sourced: AW/README.md:28,:88; AW/docs/is-it-rea.md:19] |
| LilZeeCN/zee-rp | app (Axure RP file format) | REA per author's reply | AT Unverified; AW: claimed by the author | [sourced: AT/data.json:1101; AW/docs/is-it-rea.md:20] |
| grisuno/VSL-DSP | hardware/driver (vendor .so libraries) | LazyOwn `rea` extension driving REA + Ghidra 12.1.4 | AR reviewed; AT Yes; AW `REA (partial)` | [sourced: AR/cases/vsl-dsp.md; AT/data.json:317] |
| cudanexus/vd-streamer-linux | hardware/protocol (VR streamer packages) | Claude Code + REA; vendor protocol library reused | AR reviewed; AT Yes; AW confirmed; no real headset | [sourced: AR/cases/vd-streamer-linux.md; AT/data.json:293] |
| AllMySlop/pixel-watch-esim-writer | hardware/app (companion APK) | Codex + morluto/rea | AR reviewed; later README reports real Pixel Watch 2 success | [sourced: AR/cases/watch-esim-helper.md] |
| bigtower17/picoprint | hardware/protocol (BLE thermal printer + RN app) | REA 6.5.0 doctor/ADB/entry; Hermes decompilation by hermes-dec | AR reviewed; discussion #4152 | [sourced: AR/cases/picoprint.md] |
| hygkui/v5vpn-rea-client | protocol (VPN client DLLs, gateway v2.0) | REA cited as reference; Go/MyGo client; full REA logs not public | AR reviewed; the author's fully-cracked wording is not accepted by AR | [sourced: AR/cases/v5vpn-rea-client.md] |
| NullLabTests/OpenBSD_netcat_REA | binary (Ubuntu ELF) | REA pwntools layout + Ghidra; readelf/objdump/nm/strings cross-check | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/netcat-analysis.md; AT/data.json:333] |
| az9713/REA-demo ("REA on REA") | app (rea-agents 6.3.0 package) | `analyze_javascript_application`, `doctor`; falsifiable predictions | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/rea-on-rea.md; AT/data.json:1261] |
| UnknownHacker1/rea-groundtruth | app (MongoDB Compass + fixtures) | REA 6.4.0 vs known source | AR reviewed; AT Yes; AW confirmed | [sourced: AR/cases/rea-groundtruth.md; AT/data.json:1253] |
| pablodiazjorge/rea-lab | binary (Windows crackmes, real OSS programs) | REA 6.4.0 + Ghidra, Claude Code 2.1.296, DeepSeek; preregistered studies | AR reviewed; AT Yes | [sourced: AR/cases/rea-lab.md; AT/data.json:1269] |
| CassieFox23 / REA discussion #2254 (bzip2) | binary (stripped ELF) | REA 6.4.0 + Ghidra; unstripped twin build | AR reviewed (editable discussion); AT Yes | [sourced: AR/cases/bzip2-groundtruth.md; AT/data.json:341] |
| CassieFox23 / REA discussion #3098 (local 27B model) | binary (self-built C checker) | REA MCP (PR #3026 build) + Ghidra 12.1.4, llama.cpp/OpenCode, 10-tool allowlist | AR reviewed (editable discussion) | [sourced: AR/cases/local-model-mcp.md] |
| martinlofranodeoliveira/opensource-rea-lab | app (npm rea-agents 6.4.0 artifact) | Python auditor + REA static JS analysis | AR reviewed; 110 semantic unknowns retained | [sourced: AR/cases/opensource-rea-lab.md] |
| jinshi666888/rea-playbook | app (8-file Electron sample) | REA 6.3.0; analyze → extract → verify; 10/10 checks | AR reviewed; AT Yes | [sourced: AR/cases/rea-playbook.md; AT/data.json:1237] |
| sickpyramid/ReaFuse (ratewise, objdelta) | app/library (JS donors) | REA 6.4.0 JS static analysis + brain | AR reviewed; AT Yes | [sourced: AR/cases/reafuse.md; AT/data.json:1141; RF/showcase/*] |
| MiaAI-Lab/sparkDash | integration (remote REA CLI jobs) | rea-agents 6.4.0 CLI only; fake-REA tests in CI | AR reviewed (738 stars); not a reconstruction | [sourced: AR/cases/sparkdash-rea.md] |
| glauberlima/open-scrantic | binary (Win16 NE screensaver) | Claude Code + REA (disassemble/decompile), Rust rebuild | Self-attributed in README; not in AR/AT/AW lists | [sourced: OS/README.md:268-276; OS/docs/ENGINE.md] |
| EndLess728/MiTV-Remote; B-Howell/soundconnect-desktop; Xiangyu-CAS/rea-capcut; chilliesdev/fmseasonhub-re (rea-agents 6.7.0); rodneypantonial/sims3-modernization; nicolvus/a6000-fw-decomp (REA/Ghidra traces); tanepiper/game-frame | protocol / app / game tools / firmware | README or repo-description attribution only | AT Yes (added 2026-10-24, not reviewed by AR) | [sourced: AT/data.json:1277-1325] |

### 7.3 Listed but not REA (background)

| Case | Target type | Tool | Verdict | Path |
|---|---|---|---|---|
| GLinnik21/plx-native (Plex for LG TV) | app | Ghidra + Claude Code | AW: not REA, tagged `AI-RE` | [sourced: AW/docs/is-it-rea.md:31] |
| a-funk/open-appleremote | protocol (BLE) | unstated | AT Unverified; AW `Claim?` | [sourced: AT/data.json:301; AW/docs/is-it-rea.md:32] |
| rehan-remade/universal-modder | game modding | Claude Code/Codex plugin; predates REA 4.0 | AW: not REA | [sourced: AW/docs/is-it-rea.md:33] |
| Slyvan25/OpenGHub | hardware (HID++) | manual RE from captures | AW `Manual RE` | [sourced: AW/docs/is-it-rea.md:34] |
| storytold Craft apps, AsyncFinn/rook, tallowmere/free-heroes4, designgears/wine-uwp, ESR Firefighter, MW2 decompilation | apps/games | clean-room rewrites or AI-RE with IDA/Ghidra; code often not public | AW `Rebuild` / `AI-RE` | [sourced: AW/README.md:113-119] |
| schererstefan/rea-doom-term; BryansGlizzy/AnyMouse-Vectoring | — | claims "REA method" but uses objdump / future roadmap | AR rejected for lack of actual REA output | [sourced: AR/research/2026-10-21.md:46-47] |
| wquguru/cameo | app | REA-based reconstruction described on X, source not published | AW: not confirmed | [sourced: AW/docs/is-it-rea.md:21] |
| laaielichter/Reverseport, dav1dngy/era-agent, agioracle/rea-unity-game | tooling | "based on / inspired by REA" | AT Unverified | [sourced: AT/data.json:1149,:1245,:1333] |

---

## 8. Reusable ideas for a trace-driven learning layer (strictly what the sources show)

- Make every finding an immutable, content-addressed Evidence record (`ev_<sha256>`) with `normalized_result`, `raw_result`, `analysis_profile`, `limitations`, and pass records by id rather than by copy. [sourced: UP/docs_mcp-contracts.md:16-23]
- Keep an explicit, revisioned unknowns ledger linked to the Evidence that raised each question, with duplicate rejection and paging. [sourced: UP/docs_mcp-contracts.md:270-290]
- Return compact summaries with paged views of large evidence instead of inline dumps (10 KB summary of a 334 MB file). [sourced: UP/docs_cli.md:25-52]
- Mark every tool result derived from the target as untrusted (`_meta.provenance.untrusted_content`) so hosts can gate follow-on actions. [sourced: UP/docs_mcp-contracts.md:328-345]
- Separate three layers in every note: saved observation, interpretation, and maintainer-assigned names. [sourced: UP/website_evidence_dx-ball-sound-pan.md:23-39]
- Label each claim proven / inferred / hidden (or observed / derived), and keep "unknown" as a first-class value. [sourced: AR/cases/wispr-flow.md:18; AR/cases/bzip2-groundtruth.md:18; AR/cases/vsl-dsp.md:30]
- Spec cards are behaviour-only and must carry concrete input/output examples precise enough to become tests, plus evidence references. [sourced: RF/src/schemas.mjs:8-28; RF/src/prompts.mjs:13]
- Write the acceptance contract before the implementation, map each test to its origin, and make the contract immutable to the generator. [sourced: RF/src/prompts.mjs:61-83; RF/src/pipeline.mjs:368-377]
- Bound the repair loop (default 2 rounds) and feed back only the failing TAP blocks. [sourced: RF/src/pipeline.mjs:205-228; RF/src/verify.mjs:26-48]
- Cache every reasoning stage's answer on disk keyed by stage so runs resume and replay deterministically; "handoff" mode lets the calling agent be the brain. [sourced: RF/src/brain.mjs:109-110,:217; RF/README.md:255-257]
- Treat zero findings as not established, and test analyzer coverage with fixtures of known source. [sourced: AR/cases/rea-groundtruth.md:30]
- Validate interpretations against the original by replay with both a passing and a deliberately failing input. [sourced: UP/website_evidence_ctf-masked-squares.md:99-102; AR/cases/ductf-masked-squares.md:30]
- Record semantic acceptance tests separately from byte-exact matches, and scope every metric to the unit it covers (one function does not prove whole-app compatibility). [sourced: AR/cases/dx-ball.md:30; AR/CONTRIBUTING.md:22]
- Pin the target's digest and the tool versions in every record; reject targets whose digest or known strings do not match. [sourced: UP/website_evidence_ctf-masked-squares.md:10-21; OS/src/assets/CLAUDE.md:7]
- Cite the original address (or evidence id) next to every ported rule in code and docs, and keep a deliberate-differences list. [sourced: OS/CONTRIBUTING.md:32-35; OS/docs/ENGINE.md:150-181]
- Make the rebuilt engine deterministic and headless (injected clock, fixed seed) so renders are reproducible golden outputs. [sourced: OS/docs/ARCHITECTURE.md:22-23; OS/CONTRIBUTING.md:39-47]
- Enforce clean-room by a deterministic fingerprint scan with stated thresholds, and emit a verdict that is `REJECTED` on breach regardless of tests. [sourced: RF/src/originality.mjs:4-6; RF/src/pipeline.mjs:236]
- Attribute each stage to the tool that actually did it, and date conflicting reports. [sourced: AR/CONTRIBUTING.md:22; AR/cases/watch-esim-helper.md:30]
- Distinguish connection, delivery, installation and real-device acceptance as separate verified outcomes. [sourced: AR/cases/watch-esim-helper.md:30; AR/cases/coop-anything.md:24]
- Minimise the advertised tool set per task (a 10-tool allowlist fit a 32K-context local model; 206 descriptions did not). [sourced: AR/cases/local-model-mcp.md:22]
- Expect Evidence to be lost unless retention is designed in: 0 of 339 REA-calling study runs retained Evidence proactively. [sourced: AR/cases/rea-lab.md:22]

---

## 9. Open questions

- The complete upstream MCP tool catalog (`product-catalog.json`) could not be fetched (404 at the guessed URLs); the tool table in §2 is reconstructed from documentation mentions and may be incomplete. [sourced: curl attempts 2026-10-24 to morluto.github.io/rea/product-catalog.json and rea.tools/…; UP/docs_mcp-contracts.md:1372-1383]
- Upstream docs do not define observed-vs-derived as formal Evidence fields; the labels appear only in community reports. [sourced: WebFetch summary of UP/docs_mcp-contracts.md 2026-10-24; AR/cases/bzip2-groundtruth.md:18]
- Whether the controlled-replay/snapshot tools removed in 4.0.0 have a current equivalent beyond `capture_process_scenario` and `compare_web_captures` is not established from the fetched docs. [sourced: UP/CHANGELOG.md:1403]
- The RG repo's upload date (2026-10-21) post-dates the AW fake-downloads warning (2026-10-14..16); whether it is the same actor as the other free-download repos is unknown. [sourced: RG git log; AW/README.md:201]
- GitHub API metadata (creation dates, star counts) for the six repos could not be queried from this session (proxy 403); star counts above come from the topic page snapshot only. [sourced: curl to api.github.com 2026-10-24 returned 403; https://github.com/topics/reverse-engineer-anything]
- open-scrantic documents *what* was recovered but not the REA tool calls or evidence ids used; its process evidence is the address citations and the snapshot harness, not REA records. [sourced: grep of `rea|evidence|ghidra` in OS/src returned nothing; OS/README.md:270-276]
- The rea-lab finding (no significant accuracy gain, ~2× time) is pinned to REA 6.4.0 and Claude Code 2.1.296; no later-version replication is in the cluster. [sourced: AR/cases/rea-lab.md:26]
- AR's 2026-10-21 head comparison shows several upstream community repos (vsl-dsp, hyper3d-clone, opensource-rea-lab) moved past the pinned commits; the catalogue retains old versions. [sourced: AR/research/2026-10-21.md:30,:38,:41]
