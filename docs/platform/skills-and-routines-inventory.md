# Skills and routines inventory (build workflow)

Date: 2026-10-15. Scope chosen by Jesse: the skills and routines we use to build RRFlow and DevPulse (not the RRFlow engine's Gate I runtime).

## Decided
- Scope: our build workflow (2026-10-15).
- Clyffy loader / `taxonomy.go` compatibility is not a constraint; pre-release, design it correctly (2026-10-15).
- Primary grouping: by primary graph (2026-10-15). Each skill lives under the one graph it is authoritative for and declares any other graphs it reads. Skills whose result is an effect on an outside system are `activities/`. Routines are their own `routines/` tree. Evals are their own `evals/` tree. Build stage is a property of a routine step, not a folder.
- Graph families (2026-10-15): source, knowledge, reasoning, lexicon, routine, evidence. The family set is configuration, not hardcoded, so test runs can show which families overload context and which improve results.
- Test configuration settings per graph (2026-10-15): on/off; fold into another graph; load cap (items or tokens the graph may add to context). A configuration is run as one eval arm.
- Eval measurements (2026-10-15): every arm records outcome (success, attempts, retries), cost (input/output tokens, tool calls, latency), trigger accuracy (right skill fired, wrong skill did not), and tokens added per graph. The metric set is configurable: we ship a schema and a template so metrics can be customised and new ones added.
- Config format (2026-10-15): every config file is TOML, validated by JSON Schema. Covers graph families, test configurations, metrics, skill manifests and routines. The 9 YAML files in agent-skills are to be converted.
- Schema source (2026-10-15): Rust types are the single source; `schemars` generates the JSON Schema files, which are committed so Node, Python and editors validate without a Rust toolchain. Same direction RRFlow's contracts already use.
- Eval harness (2026-10-15): extend `rrflow-eval` as the one canonical runner. Add configurable arms (replacing the hardcoded `baseline`/`runtime` pairing), trigger accuracy (reusing skill-creator's temporary-skill technique), and tokens added per graph. skill-creator remains only for tuning skill descriptions.
- Repository for config types and harness (2026-10-16, follows from decisions in `claude/compute-and-data-workload-classification.md`): config is UI-managed instance data stored in rrflowKV, so its Rust types belong in `rrflow-development/crates/transport/rrd-contract` beside the existing revisioned `EstateConfiguration`, and `rrflow-eval` stays in `rrflow-development`. agent-skills keeps skill content and consumes the generated schema. Jesse can override.

### Design principle observed across decisions
Graph families, test configurations and metrics are all configuration with a shipped schema and template, not hardcoded. Applied by default to the rest of this design (e.g. verdict thresholds) unless Jesse says otherwise.

### Placement of existing skills under the decided grouping
```
graphs/
  source/       (none yet)
  knowledge/    rust-conventions, styling-resolver
  reasoning/    structured-discussion, ai-orchestration, fork-deliberate-collapse
  lexicon/      (none yet)
  routine/      (none yet)
  evidence/     (none yet)
activities/     proxmox, cloudflare, server-administration, figma, preflight-scripts
routines/       change-authoring (from RRFlow AGENTS.md)
evals/          proxmox-workspace
unplaced:       server-ops-orchestrator (a dispatcher, not a skill)
```

### Graph family meanings
| Family | Holds |
|---|---|
| source | Files, symbols, dependencies, build targets, tests |
| knowledge | Docs, ADRs, claims, entities, conventions |
| reasoning | Decisions, plans, hypotheses, verdicts, reasoning trees |
| lexicon | LFG canonical labels and boundary questions |
| routine | Routine steps and their dependencies |
| evidence | Commits, test and CI runs, operation captures, change journals, linked by cause |

## Existing measurement tools
| Tool | Records | Limits found |
|---|---|---|
| `rrflow-development/crates/evaluation/rrflow-eval` (Rust, 318 lines) | Drives the `claude`, `codex` and `gemini` CLIs; per trial: success, attempts, retries, tool calls, input/output tokens, latency | `summarize` pairs only arms named `baseline` and `runtime` (hardcoded); no trigger accuracy; no tokens per graph |
| skill-creator benchmark (`agent-skills/hubs/infrastructure/proxmox-workspace/iteration-*/benchmark.md`) | With vs. without skill: pass rate, time, tokens. Proxmox iteration 2: pass rate 100% vs 26%, +39.8 s, +13,332 tokens | Two configurations only (with/without) |
| skill-creator trigger eval (`proxmox-workspace/trigger-eval/*/results.json`) | Description optimisation: train and held-out test trigger scores | Anthropic plugin's Python scripts; Jesse maintains 9 Windows patches in `agent-skills/tools/skill-creator-windows-patches/` because plugin updates overwrite them |

## Config and schema facts
- agent-skills today mixes 14 TOML files (per-skill `manifest.toml`, two policy files), 9 YAML files (`connectome.registry.yaml`, `domains/*/manifest.yaml`, `server-admin-policy.yaml`, `preflight-scripts.yaml`, CI) and JSON for evals/npm.
- RRFlow takes estate configuration as TOML and derives JSON Schema from Rust types with `schemars` in `rrd-contract`, `rrd-engine` and `rrd-kubernetes`. `rrd-contract/src/deployment.rs` already has `EstateConfigurationInput` and a revisioned `EstateConfiguration`.
- KnowQL's `shape` is JSON Schema.
- agent-skills tooling today is Node (`build/validate-all.mjs`, `build/compose.mjs`) plus Python helpers (`hubs/_shared/*.py`) and skill-creator's Python eval scripts.
- `rrflow-development`'s AGENTS.md requires engine "verification ... tooling" to live in that repository and every change there to carry a planning commit and journal. `rrflow-eval` and 36 result files already live there. The repo is listed as public on GitHub although AGENTS.md calls it private.

## Sources inspected
| Source | State |
|---|---|
| `EonsofStupid/agent-skills` @ `6265c88` (2026-05-18) | 11 skill folders in the 7-hub Clyffy taxonomy; last push 5 months ago |
| claude.ai account skills | One custom skill: `fork-deliberate-collapse`; the rest are Anthropic built-ins |
| `rrflow/rrflow-development` @ `cfa0e80` | Build routine lives in `AGENTS.md` (codebase-grounded change-authoring routine); engine-side skill/routine/trigger design is Gate I and unbuilt; `rrd-core/src/reasoning_tree.rs` contract exists; attunement phases `Connect → Inventory → Parse → Normalize → EntityLink → LexicalIndex → Embed → VectorIndex → Graph → Ground → Verify` are declared |

## agent-skills today
| Hub | Skills |
|---|---|
| infrastructure | proxmox, proxmox-workspace (`kind = "macro"`, eval workspace), server-administration, server-ops-orchestrator, cloudflare |
| coding | rust-conventions (183 words) |
| content | figma, styling-resolver |
| intelligence | ai-orchestration |
| meta | preflight-scripts, structured-discussion |
| platform, personal | empty |

Shared machinery that already exists: `manifest.toml` schema (`kind` = skill/macro/preset, `category`, `capabilities`, `modalities`, `dependencies`, `permission_tier`), `connectome.registry.yaml` router, `_shared/` capture, namespace and preflight helpers, three-tier policy gating.

## Gaps found
1. No skill touches a knowledge graph, source graph or reasoning graph. Coding has one 183-word conventions file.
2. No routine layer. `kind = "macro"` is used once, for an eval workspace, not for an ordered multi-skill routine.
3. `domains/devpulse/manifest.yaml` references 11 skills that do not exist: tauri2-patterns, svelte5-runes, error-handling, state-management, data-fetching, proxmox-management (renamed to `proxmox`), devpulse-cortex, devpulse-pipeline-editor, devpulse-teaching-layer, devpulse-rro, devpulse-state.
4. The 800-word skill cap is not enforced by `ci/skills-ci.yml` or `build/validate-all.mjs`. Three skills exceed it: structured-discussion 1,805, proxmox 1,333, figma 1,204.
5. `hubs/_PATTERN.md` has 12 references to pre-reshape paths (`claude-code-skills/`, `categories/`).
6. The RRFlow build routine (AGENTS.md) is not a skill anywhere; it is repository prose.
7. `hubs/_shared/` uses a folder name banned by the Pre-Release Operating Mode (DDD) rule (`core`, `shared`, `utils`, `helpers`, `common`, `lib`).

## Grouping facts (basis for the decided grouping)
- RRFlow canon already separates skill packages (content), routine definitions (ordered steps), external activities (effects on outside systems), and typed graph relation families. Using the same split here lets the build workflow later import into RRFlow unchanged.
- RRFlow's single-owner rule: each subject has exactly one owning record. A grouping axis should give each skill exactly one home.
- Build stage gives shared skills several homes (e.g. inventory is used in discover and verify), so stage belongs on routine steps, not folders.
- Domain categories were ambiguous: structured-discussion sat in `meta` and ai-orchestration in `intelligence`, yet both produce decisions/verdicts.

## Status
Paused 2026-10-15 at Jesse's direction to classify the compute and data workloads first; see `claude/compute-and-data-workload-classification.md`.

## Parked
- Verdict thresholds (configurable per the design principle above).
- First increment format (taxonomy map vs. skill review cards vs. code).
- Placement of server-ops-orchestrator (dispatcher).
- Rename of `_shared/` and the other gap fixes above.
- RRFlow dependency-spine reorder: now the open decision in `claude/compute-and-data-workload-classification.md`.
