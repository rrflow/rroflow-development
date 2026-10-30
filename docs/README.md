# rroflow docs

Start with [STATUS.md](STATUS.md). Everything that guides rroflow.ai and the RRFlow work lives here, in git, so nothing exists only in a chat or a project space. The claude.ai project "Rrflow" keeps copies under `claude/…`; documents written there refer to each other by those names, so the table below maps each one to its file here. When the two differ, this repository is the record.

## rroflow.ai (this product)

| File | What it is | Project name |
|---|---|---|
| [plan/rroflow-ai-plan.md](plan/rroflow-ai-plan.md) | The plan: product split (RRFlow engine vs rroflow.ai), learning vision, decisions D-1…D-6, build order | `claude/rroflow-ai-plan.md` |
| [plan/capability-map.md](plan/capability-map.md) | Raindrop + Sentry + PostHog AI capability baseline, implementation status and acceptance gaps | `claude/rroflow-capability-map.md` |
| [plan/identity-and-devspaces.md](plan/identity-and-devspaces.md) | Nomen identity, devspaces, Shippin tenant attach, API, Nomen app ids, sign-in setup on tx-infra-prod | `claude/rroflow-identity-and-devspaces.md` |
| [plan/sdk-install-and-linking.md](plan/sdk-install-and-linking.md) | Installing the rroflow SDK, detecting installed agents and subscriptions, linking them to rroflow.ai, releases | `claude/rroflow-sdk-install-and-linking.md` |
| [deploy/README.md](deploy/README.md) | Where rroflow.ai runs (Texas), the VM-behind-the-edge layout, pull deploys, the dashboard stack, owner steps | `claude/rroflow-deploy.md` |
| [STATUS.md](STATUS.md) | Where everything stands: built, released, deployed, open, and whose step it is | `claude/rroflow-status.md` |
| [plan/canon-alignment-notes.md](plan/canon-alignment-notes.md) | Vocabulary and canon constraints the mechanism spec must follow | `claude/rroflow-canon-alignment-notes.md` |
| [spec/mechanism-spec-v0.md](spec/mechanism-spec-v0.md) | RROFLOW mechanism spec v0 (behaviour signals, findings, experiments, the learning loops) | `claude/rroflow-mechanism-spec-v0.md` |
| [ux/vibe-coder-onboarding.md](ux/vibe-coder-onboarding.md) | Onboarding plan for vibe coders | `claude/rroflow-vibe-coder-onboarding.md` |

## Texas readiness and learning continuation

- [Texas application VM readiness](plan/texas-readiness.md): verified placement, deployment guards, recovery and release gates.
- [Project access tooling](plan/project-access-tooling.md): private credential references, Cloudflare/Nomen verification and strict read-only SSH inventory.
- [Subscription support](plan/subscription-support.md): capture, sign-in, history and inference capability status per surface.
- [Automatic datasets v0](spec/automatic-datasets-v0.md): policy-authorized snapshots, labels, immutable splits and training boundaries.

## Research

| File | What it is | Project name |
|---|---|---|
| [research/devpulse/devpulse-research-ledger.md](research/devpulse/devpulse-research-ledger.md) | DevPulse repos and Drive notes: signals, learning loops, cost tracking, vibe-coder UX | `claude/devpulse-research-ledger.md` |
| [research/raindrop/findings-brief.md](research/raindrop/findings-brief.md) | What we learned from Raindrop, written for Raindrop's team | `claude/raindrop-findings-brief.md` |
| [research/raindrop/source-ledger.md](research/raindrop/source-ledger.md) | Sourced claims about Raindrop (public docs, blog, API, SDKs) | `claude/raindrop-source-ledger.md` |
| [research/raindrop/live-capture-ledger.md](research/raindrop/live-capture-ledger.md) | Live capture through the Query API; vibeguard's loop | `claude/raindrop-live-capture-ledger.md` |
| [research/raindrop/live-capture-checklist.md](research/raindrop/live-capture-checklist.md) | What to capture from the live app | `claude/raindrop-live-capture-checklist.md` |
| [research/raindrop/signal-authoring-capture.md](research/raindrop/signal-authoring-capture.md) | Custom signal authoring end to end, generated classifier, precision by hand | `claude/raindrop-signal-authoring-capture.md` |
| [research/raindrop/ux-onboarding-and-gating.md](research/raindrop/ux-onboarding-and-gating.md) | Onboarding checklist, trace review, progressive unlock | `claude/raindrop-ux-onboarding-and-gating.md` |
| [research/raindrop/ux-settings.md](research/raindrop/ux-settings.md) | Settings surfaces | `claude/raindrop-ux-settings.md` |
| [research/raindrop/prototype-ledger.md](research/raindrop/prototype-ledger.md) | The rrflow-hydradb prototype's Raindrop wiring | `claude/prototype-raindrop-ledger.md` |
| [research/rea/rea-source-ledger.md](research/rea/rea-source-ledger.md) | Reverse-engineer-anything (REA) sources | `claude/rea-source-ledger.md` |

## RRFlow engine and site (sibling repos; kept here so the record is in one place)

| File | What it is | Project name |
|---|---|---|
| [rrflow/build-order.md](rrflow/build-order.md) | RRFlow build order and status (decided 2026-10-16, updated 2026-10-18) | `claude/rrflow-build-order.md` |
| [rrflow/merge-train-2026-10-19.md](rrflow/merge-train-2026-10-19.md) | Merges of 2026-10-19/20 | `claude/rrflow-merge-train-2026-10-19.md` |
| [rrflow/capability-study.md](rrflow/capability-study.md) | SurrealDB and Qdrant capabilities against RRFlow | `claude/rrflow-capability-study.md` |
| [rrflow/definitions-and-schema-audit.md](rrflow/definitions-and-schema-audit.md) | Definitions, schemas, contradictions, decisions 1–17 | `claude/rrflow-definitions-and-schema-audit.md` |
| [rrflow/development-salvage-assessment.md](rrflow/development-salvage-assessment.md) | Salvage assessment of rrflow-development (2026-10-13) | `claude/rrflow-development-salvage-assessment.md` |
| [rrflow/public-site-build-order.md](rrflow/public-site-build-order.md) | rrflow.ai public site spec and build order | `claude/rrflow-public-site-build-order.md` |
| [rrflow/site-video-pipeline.md](rrflow/site-video-pipeline.md) | Generated video for the rrflow.ai site | `claude/rrflow-site-video-pipeline.md` |

## Platform

| File | What it is | Project name |
|---|---|---|
| [platform/compute-and-data-workload-classification.md](platform/compute-and-data-workload-classification.md) | Eight workload classes, endpoint bindings, hardware facts | `claude/compute-and-data-workload-classification.md` |
| [platform/skills-and-routines-inventory.md](platform/skills-and-routines-inventory.md) | Build-workflow skills and routines | `claude/skills-and-routines-inventory.md` |

## Deployment configuration

[Collector deployment configuration and license status](plan/deployment-configuration.md) records public profiles, endpoint/account binding and remaining customization work.

## Rule

A new or changed doc lands here by pull request. The project copy is updated from this file, not the other way round.
