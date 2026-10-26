# Captured 2026-10-25: Raindrop Settings (org WiredFront, project claude-code)

Routes: `/settings/<slug>`. Sidebar: team management, billing, notifications (`/settings/alerts`), issue detection, pii redaction, slack, connectors, repositories, api keys, memory, projects, filters, customization. The header always shows a masked API KEY that links to `/settings/api-keys#write-key`.

| Section | What it holds (observed) |
|---|---|
| projects | "Projects isolate your data. Every workspace has an immutable default project." The guidance is "Create projects from your code": set `projectId` in the SDK and the project appears automatically. Each project has Rename, Description and Archive. Ours: Production (default), claude-code, rrflow-hydradb, ShippinAi (each carries the user's own description). |
| issue detection | A per-project toggle for "Latency issues". Turning it off stops new latency issues for everyone in the project. |
| pii redaction | "This section isn't available for your organization." Plan-gated, yet server-side redaction still ran on our events: "RROFLOW" and "Claude" were rewritten to `[PERSON_NAME]`. So redaction is on, but this plan can't tune or allowlist it. |
| memory | "What Raindrop has learned about WiredFront", with tabs ALL / ORGANIZATION / USERS / PRODUCT / YOUR AGENT / GUIDANCE. It is empty so far, and is filled by the Triage agent. |
| connectors | MCP servers for the Triage agent: Linear, Datadog, Sentry, PostHog, GitHub, plus "40 services". Connections are shared org-wide and credentials are encrypted. "Raindrop connects once to verify the URL and credentials, then lists the tools the agent gains." |
| repositories | "Connect GitHub and discover the agents in your repositories. Simulations and Change reports use this shared catalog." |
| filters | Exclude users by ID, trait or event property. Excluded users are hidden from queries but still ingested. Examples: `test-`, `email contains yourcompany.com`, `environment equals staging`. Saved event filters can be reused across pages. |
| notifications | Timezone, delivery, issue tags (applied automatically to each new issue across projects and usable in routing instructions), Briefs, and signal alerts with "New alert". |
| customization | Theme (Light / Dark beta / System). Display options: full text by default, auto-translate non-English, dim unselected messages. Conversation participant label field. **Custom Event Links**: templated links to your own tools that appear on every event, with fields and properties inserted as variables. |

## Patterns worth taking (vibe-coder angle)
- Projects appear when code first sends with a new id, so there is zero setup in the UI. The catch: an unclaimed slug stays hidden until someone clicks "Create project", and nothing tells you that.
- "Excluded but still ingested" filters: hiding test traffic is reversible because nothing is dropped.
- Custom Event Links turn every event into a jump-off point into your own tools.
- Memory is categorised by subject (org / users / product / your agent / guidance), the same split as our memory layout. It is where the Triage agent stores what it learns.
