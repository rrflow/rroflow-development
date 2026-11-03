// Rust owns these contracts. Reject incomplete or cross-project data at the UI boundary.
export type Condition =
  | { kind: "tool_errors"; at_least: number }
  | { kind: "tool_named"; name: string }
  | { kind: "prompt_contains" | "response_contains"; text: string };
export type Rule = {
  id: string;
  name: string;
  group: string;
  enabled: boolean;
  condition: Condition;
  agent: string | null;
  project: string | null;
};
export type Settings = {
  revision: number;
  custom: Rule[];
  disabled: string[];
  options: Record<string, Record<string, number>>;
};
export type Check = {
  id: string;
  version: number;
  category: string;
  meaning: string;
  options: {
    name: string;
    default: number;
    min: number;
    max: number;
    meaning: string;
  }[];
};
export type Library = {
  settings: Settings;
  checks: Check[];
  counts: Record<string, number>;
  agents: string[];
  projects: string[];
};
export type Preview = {
  scanned: number;
  evaluated: number;
  unavailable: number;
  moreHistory: boolean;
  matches: { id: string; summary: string }[];
};
const bad = () =>
  new Error(
    "The signal response is incomplete. Refresh before making changes.",
  );
function object(v: unknown): Record<string, unknown> {
  if (!v || typeof v !== "object" || Array.isArray(v)) throw bad();
  return v as Record<string, unknown>;
}
function text(v: unknown): string {
  if (typeof v !== "string") throw bad();
  return v;
}
function list(v: unknown): unknown[] {
  if (!Array.isArray(v)) throw bad();
  return v;
}
function bool(v: unknown): boolean {
  if (typeof v !== "boolean") throw bad();
  return v;
}
function number(v: unknown): number {
  if (typeof v !== "number" || !Number.isFinite(v)) throw bad();
  return v;
}
function count(v: unknown): number {
  const n = number(v);
  if (!Number.isSafeInteger(n) || n < 0) throw bad();
  return n;
}
function nullable(v: unknown) {
  return v === null ? null : text(v);
}
function scoped(v: unknown, id: string) {
  const r = object(v);
  if (r.devspace !== id)
    throw new Error(
      "The signal response belongs to another project. Refresh your project list.",
    );
  return r;
}
function rule(v: unknown): Rule {
  const r = object(v),
    c = object(r.condition),
    kind = text(c.kind);
  let condition: Condition;
  if (kind === "tool_errors") condition = { kind, at_least: count(c.at_least) };
  else if (kind === "tool_named") condition = { kind, name: text(c.name) };
  else if (kind === "prompt_contains" || kind === "response_contains")
    condition = { kind, text: text(c.text) };
  else throw bad();
  return {
    id: text(r.id),
    name: text(r.name),
    group: text(r.group),
    enabled: bool(r.enabled),
    condition,
    agent: nullable(r.agent),
    project: nullable(r.project),
  };
}
function config(v: unknown): Settings {
  const s = object(v);
  return {
    revision: count(s.revision),
    custom: list(s.custom).map(rule),
    disabled: list(s.disabled).map(text),
    options: Object.fromEntries(
      Object.entries(object(s.options)).map(([id, raw]) => [
        id,
        Object.fromEntries(
          Object.entries(object(raw)).map(([name, value]) => [
            name,
            number(value),
          ]),
        ),
      ]),
    ),
  };
}
export function saved(v: unknown, id: string): Settings {
  return config(scoped(v, id).settings);
}
export function library(v: unknown, id: string): Library {
  const r = scoped(v, id),
    summary = object(r.summary);
  return {
    settings: config(r.settings),
    counts: Object.fromEntries(
      Object.entries(object(summary.signals)).map(([id, n]) => [id, count(n)]),
    ),
    agents: Object.keys(object(summary.by_agent)),
    projects: Object.keys(object(summary.by_project)),
    checks: list(object(r.catalogue).signals).map((raw) => {
      const c = object(raw);
      return {
        id: text(c.id),
        version: count(c.version),
        category: text(c.category),
        meaning: text(c.meaning),
        options: list(c.options).map((raw) => {
          const o = object(raw);
          return {
            name: text(o.name),
            default: number(o.default),
            min: number(o.min),
            max: number(o.max),
            meaning: text(o.meaning),
          };
        }),
      };
    }),
  };
}
export function preview(v: unknown, id: string): Preview {
  const r = scoped(v, id);
  const result = {
    scanned: count(r.scanned),
    evaluated: count(r.evaluated),
    unavailable: count(r.unavailable),
    moreHistory: bool(r.more_history),
    matches: list(r.matches).map((raw) => {
      const m = object(raw);
      return { id: text(m.event_id), summary: text(object(m.hit).summary) };
    }),
  };
  if (
    result.scanned > 100 ||
    result.evaluated + result.unavailable > result.scanned ||
    result.matches.length > result.evaluated
  )
    throw bad();
  return result;
}
const titles: Record<string, string> = {
  blocked_handoff: "Blocked handoff",
  false_done: "Possible false completion",
  tool_error_streak: "Repeated tool failures",
  usage_limit_hit: "Usage limit reached",
  context_loss: "Context compaction",
  correction_spiral: "Possible correction spiral",
  aborted_turn: "Interrupted turn",
  mood: "Mood shift",
};
export const title = (id: string) =>
  Object.hasOwn(titles, id) ? titles[id] : id.replaceAll("_", " ");
export function description(rule: Rule) {
  const c = rule.condition;
  return c.kind === "tool_errors"
    ? `At least ${c.at_least} tool calls fail in one turn.`
    : c.kind === "tool_named"
      ? `A completed tool call is named "${c.name}".`
      : `${c.kind === "prompt_contains" ? "The message sent to the AI" : "The AI response"} contains "${c.text}" (ignoring letter case).`;
}
