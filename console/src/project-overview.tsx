import { useEffect, useState, type ReactNode } from "react";
import { api, ApiError, monitorLink } from "./api";
import {
  turns,
  sdk,
  sources,
  type Turns,
  type Sdk,
  type Sources,
  type Bucket,
} from "./monitoring-api";
import { Icon } from "./ui";

type Load<T> =
  | { state: "loading" }
  | { state: "error"; message: string }
  | { state: "ready"; data: T };
const initial = { state: "loading" } as const;
const fmt = (n: number) => n.toLocaleString();
const labels: Record<string, string> = {
  blocked_handoff: "Blocked handoff",
  false_done: "Possible false completion",
  tool_error_streak: "Repeated tool failures",
  usage_limit_hit: "Usage limit reached",
  context_loss: "Context compaction",
  correction_spiral: "Possible correction spiral",
  aborted_turn: "Interrupted turn",
  mood: "Mood shift",
  sdk_call_failed: "Call failed",
  sdk_tool_failed: "Tool failed",
  sdk_stream_cancelled: "Stream cancelled",
  sdk_incomplete: "Incomplete activity",
};
const title = (id: string, names: Record<string, string>) =>
  names[id] || labels[id] || id.replaceAll("_", " ");
const agent = (id: string, kind: "turn" | "sdk") =>
  ({
    "claude-code": "Claude Code",
    codex: kind === "sdk" ? "Codex SDK" : "Codex",
    claude: "Claude SDK",
    grok: "Grok SDK",
    rrflow: "RRFlow",
  })[id] || id;
function link(
  project: string,
  kind: "turn" | "sdk",
  filter: Record<string, string> = {},
) {
  return monitorLink(
    project,
    "activity?" + new URLSearchParams({ ...filter, kind }),
  );
}
function Status<T>({
  value,
  children,
}: {
  value: Load<T>;
  children: (v: T) => ReactNode;
}) {
  return value.state === "loading" ? (
    <p role="status">Loading saved activity...</p>
  ) : value.state === "error" ? (
    <p role="alert" className="error">
      {value.message}
    </p>
  ) : (
    children(value.data)
  );
}
function Bars({
  rows,
  unit,
  project,
  kind,
  names = {},
}: {
  rows: Bucket[];
  unit: string;
  project: string;
  kind: "turn" | "sdk";
  names?: Record<string, string>;
}) {
  const shown = rows.filter((r) => r.count > 0).slice(0, 6),
    max = Math.max(1, ...shown.map((r) => r.count));
  return shown.length ? (
    <ul className="overview-signals">
      {shown.map((r) => (
        <li key={r.id}>
          <a href={link(project, kind, { signal: r.id })}>
            <span>{title(r.id, names)}</span>
            <strong>
              {fmt(r.count)}{" "}
              <small>
                {r.count === 1
                  ? unit === "matches"
                    ? "match"
                    : "activity"
                  : unit}
              </small>
            </strong>
            <svg
              viewBox="0 0 100 4"
              preserveAspectRatio="none"
              aria-hidden="true"
            >
              <rect className="bar-track" width="100" height="4" rx="2" />
              <rect
                className="bar-value"
                width={(100 * r.count) / max}
                height="4"
                rx="2"
              />
            </svg>
          </a>
        </li>
      ))}
    </ul>
  ) : (
    <p>No signal matches are stored in this activity yet.</p>
  );
}
function Agents({
  rows,
  kind,
  project,
}: {
  rows: Bucket[];
  kind: "turn" | "sdk";
  project: string;
}) {
  return (
    <ul className="overview-agents">
      {rows.slice(0, 12).map((r) => (
        <li key={r.id}>
          <a href={link(project, kind, { agent: r.id })}>
            <Icon name="pulse" />
            <span>{agent(r.id, kind)}</span>
            <strong>{fmt(r.count)}</strong>
          </a>
        </li>
      ))}
    </ul>
  );
}
export function ProjectOverview({
  project,
  onExpired,
}: {
  project: string;
  onExpired: () => void;
}) {
  const [turnData, setTurns] = useState<Load<Turns>>(initial),
    [sdkData, setSdk] = useState<Load<Sdk>>(initial),
    [health, setHealth] = useState<Load<Sources>>(initial);
  const [revision, setRevision] = useState(0),
    [loadedAt, setLoadedAt] = useState<number | null>(null),
    [, setTick] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => setTick((t) => t + 1), 15000);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    const c = new AbortController();
    setTurns(initial);
    setSdk(initial);
    setHealth(initial);
    setLoadedAt(null);
    async function load<T>(
      path: string,
      parse: (v: unknown, id: string) => T,
      set: (v: Load<T>) => void,
    ) {
      try {
        const v = parse(
          await api(path, c.signal, "GET", undefined, project),
          project,
        );
        if (!c.signal.aborted) {
          set({ state: "ready", data: v });
          if (path === "/v1/sources/health") setLoadedAt(performance.now());
        }
      } catch (e) {
        if (c.signal.aborted) return;
        if (e instanceof ApiError && e.status === 401) onExpired();
        else
          set({
            state: "error",
            message:
              e instanceof Error
                ? e.message
                : "Monitoring is unavailable. Try refreshing.",
          });
      }
    }
    void load("/v1/summary", turns, setTurns);
    void load("/v1/sdk/summary", sdk, setSdk);
    void load("/v1/sources/health", sources, setHealth);
    return () => c.abort();
  }, [project, revision]);
  const pending = [turnData, sdkData, health].some(
    (v) => v.state === "loading",
  );
  const elapsed =
    loadedAt === null ? 0 : Math.max(0, performance.now() - loadedAt);
  return (
    <section
      className="project-overview"
      aria-labelledby="project-overview-title"
    >
      <div className="section-heading">
        <div>
          <p className="eyebrow">Your AI, in view</p>
          <h2 id="project-overview-title">What reached this project</h2>
          <p className="muted">
            All captured history. Open a count or a signal to see its evidence.
          </p>
        </div>
        <button
          className="secondary"
          disabled={pending}
          onClick={() => setRevision((r) => r + 1)}
        >
          Refresh overview
        </button>
      </div>
      <div className="overview-metrics">
        <a className="panel" href={link(project, "turn")}>
          <Icon name="pulse" />
          <span>Captured turns</span>
          <strong data-testid="overview-turn-count">
            {turnData.state === "ready"
              ? fmt(turnData.data.count)
              : turnData.state === "loading"
                ? "Loading..."
                : "Unavailable"}
          </strong>
          <small>Claude Code and Codex transcripts</small>
        </a>
        <a className="panel" href={link(project, "sdk")}>
          <Icon name="grid" />
          <span>SDK activities</span>
          <strong data-testid="overview-sdk-count">
            {sdkData.state === "ready" && sdkData.data.available
              ? fmt(sdkData.data.count)
              : sdkData.state === "loading"
                ? "Loading..."
                : "Unavailable"}
          </strong>
          <small>Indexed calls, tools and streams</small>
        </a>
        <a className="panel" href={monitorLink(project, "sources")}>
          <Icon name="team" />
          <span>Collectors seen</span>
          <strong data-testid="overview-source-count">
            {health.state === "ready"
              ? fmt(health.data.rows.length)
              : health.state === "loading"
                ? "Loading..."
                : "Unavailable"}
          </strong>
          <small>Saved reports, not an online count</small>
        </a>
      </div>
      <details className="overview-explainer">
        <summary>What do these numbers mean?</summary>
        <p>
          A turn comes from an agent transcript. SDK activity comes from an
          instrumented application. The same work can appear in both, so these
          counts are shown separately. Signals are findings to review, not
          confirmed failures or training labels.
        </p>
        <p>
          New SDK events may still be waiting to be indexed. Collector reports
          describe their last upload; a silent collector cannot report the size
          of its current offline queue.
        </p>
      </details>
      <div className="overview-panels">
        <section className="panel" aria-labelledby="turn-signals-title">
          <div className="panel-heading">
            <h3 id="turn-signals-title">Signals in captured turns</h3>
            <span className="badge">Transcript evidence</span>
          </div>
          <Status value={turnData}>
            {(v) => (
              <>
                <Bars
                  rows={v.signals}
                  project={project}
                  kind="turn"
                  unit="matches"
                  names={v.names}
                />
                <Agents rows={v.agents} kind="turn" project={project} />
                {v.count === 0 ? (
                  <p>
                    Connect a source to start seeing turns here.{" "}
                    <a href={monitorLink(project, "sources")}>
                      Open connection guide
                    </a>
                  </p>
                ) : (
                  <p className="muted">
                    {fmt(v.toolErrors)} tool errors across {fmt(v.toolCalls)}{" "}
                    recorded tool calls. Usage was not reported for{" "}
                    {fmt(v.usageMissing)} turns.
                  </p>
                )}
                <a href={monitorLink(project, "signals")}>
                  Explore signals and create your own <Icon name="arrow" />
                </a>
              </>
            )}
          </Status>
        </section>
        <section className="panel" aria-labelledby="sdk-signals-title">
          <div className="panel-heading">
            <h3 id="sdk-signals-title">Signals in SDK activity</h3>
            <span className="badge">Indexed evidence</span>
          </div>
          <Status value={sdkData}>
            {(v) =>
              !v.available ? (
                <p>{v.reason}</p>
              ) : (
                <>
                  {v.pending > 0 && (
                    <p className="notice">
                      {fmt(v.pending)} captures need indexing; {fmt(v.errors)}{" "}
                      have an indexing error. Counts below may be behind
                      received events.
                    </p>
                  )}
                  <Bars
                    rows={v.signals}
                    project={project}
                    kind="sdk"
                    unit="activities"
                  />
                  <Agents rows={v.agents} kind="sdk" project={project} />
                  <p className="muted">
                    {fmt(v.withFindings)}{" "}
                    {v.withFindings === 1 ? "activity has" : "activities have"}{" "}
                    one or more findings.
                    {v.otherAgents > 0
                      ? ` ${fmt(v.otherAgents)} activities from other agents are outside this breakdown.`
                      : ""}
                  </p>
                  <a href={link(project, "sdk")}>
                    Open SDK activity and evidence <Icon name="arrow" />
                  </a>
                </>
              )
            }
          </Status>
        </section>
      </div>
      <section className="panel" aria-labelledby="overview-delivery-title">
        <div className="panel-heading">
          <h3 id="overview-delivery-title">Are your collectors reporting?</h3>
          <a href={monitorLink(project, "sources")}>
            All delivery details <Icon name="arrow" />
          </a>
        </div>
        <Status value={health}>
          {(v) => (
            <>
              {v.rows.length === 0 ? (
                <p>
                  No collector report has arrived for this project.{" "}
                  <a href={monitorLink(project, "sources")}>
                    Connect a source and check its first delivery.
                  </a>
                </p>
              ) : (
                <ul className="overview-delivery">
                  {v.rows.slice(0, 4).map((r) => {
                    const age =
                        Date.parse(v.asOf) - Date.parse(r.receivedAt) + elapsed,
                      recent = age >= 0 && age <= v.recentFor * 1000;
                    const attention =
                      r.captureErrors > 0 ||
                      r.delivery === false ||
                      (r.sdk !== null &&
                        (r.sdk.errors > 0 || r.sdk.retrying > 0));
                    const status = !recent
                      ? "No recent report loaded"
                      : attention
                        ? "Reported a problem"
                        : "Reported recently";
                    return (
                      <li key={r.id}>
                        <div>
                          <strong>Collector {r.id.slice(0, 8)}</strong>
                          <p>
                            Report received{" "}
                            {new Date(r.receivedAt).toLocaleString()}.
                          </p>
                          <p>
                            {fmt(r.pending)} turn revisions waiting.
                            {r.sdk
                              ? ` ${fmt(r.sdk.pending)} SDK events waiting.`
                              : " SDK queue counts were not reported."}
                          </p>
                          <p className="muted">
                            {r.delivery === true
                              ? "Last delivery check succeeded."
                              : r.delivery === false
                                ? "Last delivery check failed. Local records remain queued."
                                : "No delivery result was reported."}
                            {r.captureErrors > 0
                              ? ` ${fmt(r.captureErrors)} capture errors need attention.`
                              : ""}
                          </p>
                        </div>
                        <span
                          className={
                            "badge " + (recent && !attention ? "positive" : "")
                          }
                        >
                          {status}
                        </span>
                      </li>
                    );
                  })}
                </ul>
              )}
              <p className="muted">
                Last loaded server reports: {new Date(v.asOf).toLocaleString()}.
                Refresh to check for newer reports. Collector approval alone
                does not prove activity arrived.
              </p>
            </>
          )}
        </Status>
      </section>
    </section>
  );
}
