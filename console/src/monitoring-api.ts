// Validate metrics at the consumer boundary; missing data must never become zero.
export type Bucket = { id: string; count: number };
export type Turns = {
  count: number;
  toolCalls: number;
  toolErrors: number;
  usageMissing: number;
  agents: Bucket[];
  signals: Bucket[];
  names: Record<string, string>;
};
export type Sdk =
  | { available: false; reason: string }
  | {
      available: true;
      count: number;
      withFindings: number;
      agents: Bucket[];
      otherAgents: number;
      signals: Bucket[];
      pending: number;
      errors: number;
      asOf: string;
    };
export type Source = {
  id: string;
  receivedAt: string;
  pending: number;
  captureErrors: number;
  delivery: boolean | null;
  sessions: boolean | null;
  sdk: null | { pending: number; errors: number; retrying: number };
};
export type Sources = { rows: Source[]; asOf: string; recentFor: number };
function object(v: unknown): Record<string, unknown> {
  if (!v || typeof v !== "object" || Array.isArray(v))
    throw new Error("The monitoring response could not be read.");
  return v as Record<string, unknown>;
}
function text(v: unknown): string {
  if (typeof v !== "string")
    throw new Error("The monitoring response is incomplete.");
  return v;
}
function list(v: unknown): unknown[] {
  if (!Array.isArray(v)) throw new Error("The monitoring list is incomplete.");
  return v;
}
function count(v: unknown): number {
  if (typeof v !== "number" || !Number.isSafeInteger(v) || v < 0)
    throw new Error("A monitoring count is unavailable.");
  return v;
}
function timestamp(v: unknown): string {
  const s = text(v);
  if (!Number.isFinite(Date.parse(s)))
    throw new Error("A report time could not be read.");
  return s;
}
function optionalBoolean(v: unknown): boolean | null {
  if (v === null || v === undefined) return null;
  if (typeof v !== "boolean")
    throw new Error("A collector status could not be read.");
  return v;
}
function project(v: unknown, id: string) {
  const value = object(v);
  if (value.devspace !== id)
    throw new Error(
      "Monitoring returned a different project. Refresh your project list.",
    );
  return value;
}
function buckets(v: unknown): Bucket[] {
  return Object.entries(object(v))
    .map(([id, n]) => ({ id, count: count(n) }))
    .sort((a, b) => b.count - a.count || a.id.localeCompare(b.id));
}
export function turns(v: unknown, id: string): Turns {
  const value = project(v, id),
    s = object(value.summary),
    settings = object(value.settings);
  const names: Record<string, string> = Object.create(null);
  for (const raw of list(settings.custom)) {
    const c = object(raw);
    names[text(c.id)] = text(c.name);
  }
  return {
    count: count(s.turns),
    toolCalls: count(s.tool_calls),
    toolErrors: count(s.tool_errors),
    usageMissing: count(s.turns_without_usage),
    agents: buckets(s.by_agent),
    signals: buckets(s.signals),
    names,
  };
}
export function sdk(v: unknown, id: string): Sdk {
  const s = project(v, id);
  if (s.available === false)
    return { available: false, reason: text(s.reason) };
  if (s.available !== true)
    throw new Error("SDK activity status could not be read.");
  const c = object(s.coverage);
  const read = (v: unknown, key: string) =>
    list(v).map((raw) => {
      const row = object(raw);
      return { id: text(row[key]), count: count(row.activities) };
    });
  return {
    available: true,
    count: count(s.activities),
    withFindings: count(s.activities_with_findings),
    agents: read(s.by_agent, "agent"),
    otherAgents: count(s.other_agent_activities),
    signals: read(s.top_signals, "signal"),
    pending: count(c.pending_captures),
    errors: count(c.error_captures),
    asOf: timestamp(s.as_of),
  };
}
export function sources(v: unknown, id: string): Sources {
  const s = project(v, id);
  return {
    asOf: timestamp(s.as_of),
    recentFor: count(s.recent_for_seconds),
    rows: list(s.sources).map((raw) => {
      const row = object(raw),
        r = object(row.report),
        sdk = r.sdk == null ? null : object(r.sdk);
      return {
        id: text(r.source_id),
        receivedAt: timestamp(row.received_at),
        pending: count(r.pending_revisions),
        captureErrors: count(r.capture_errors),
        delivery: optionalBoolean(r.delivery_ok),
        sessions: optionalBoolean(r.session_capture),
        sdk: sdk
          ? {
              pending: count(sdk.pending_events),
              errors: count(sdk.queue_errors),
              retrying: count(sdk.retrying_queues),
            }
          : null,
      };
    }),
  };
}
