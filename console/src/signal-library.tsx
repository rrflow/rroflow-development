import { useEffect, useRef, useState, type FormEvent } from "react";
import { api, ApiError, monitorLink } from "./api";
import { Dialog, Icon } from "./ui";
import {
  library,
  saved,
  preview,
  title,
  description,
  type Library,
  type Rule,
  type Settings,
  type Check,
  type Preview,
  type Condition,
} from "./signal-api";
const message = (e: unknown) =>
  e instanceof ApiError && e.status === 409
    ? "Someone changed the signal settings. Reload current settings before saving again."
    : e instanceof Error
      ? e.message
      : "Signals are unavailable. Try again.";
const groupNames: Record<string, string> = {
  failure: "Quality",
  friction: "Working together",
  cost: "Usage and interruptions",
  context: "Context",
};
const groupName = (id: string) =>
  Object.hasOwn(groupNames, id) ? groupNames[id] : id;
const findings = (project: string, signal: string) =>
  monitorLink(project, "signals?" + new URLSearchParams({ signal }));
type Draft = {
  name: string;
  group: string;
  kind: Condition["kind"];
  number: string;
  text: string;
  agent: string;
  sourceProject: string;
};
function CreateSignal({
  project,
  data,
  onSave,
  onReload,
  onClose,
  onExpired,
}: {
  project: string;
  data: Library;
  onSave: (s: Settings) => Promise<void>;
  onReload: () => Promise<void>;
  onClose: () => void;
  onExpired: () => void;
}) {
  const [id] = useState(() => "custom_" + crypto.randomUUID());
  const [draft, setDraft] = useState<Draft>({
    name: "",
    group: "My signals",
    kind: "tool_errors",
    number: "3",
    text: "",
    agent: "",
    sourceProject: "",
  });
  const [result, setResult] = useState<{
      signature: string;
      value: Preview;
    } | null>(null),
    [busy, setBusy] = useState(""),
    [error, setError] = useState(""),
    [conflict, setConflict] = useState(false);
  const c = useRef(new AbortController()),
    version = useRef(0);
  useEffect(() => {
    c.current = new AbortController();
    return () => {
      c.current.abort();
      version.current++;
    };
  }, []);
  const condition: Condition =
    draft.kind === "tool_errors"
      ? { kind: draft.kind, at_least: Number(draft.number) }
      : draft.kind === "tool_named"
        ? { kind: draft.kind, name: draft.text.trim() }
        : { kind: draft.kind, text: draft.text.trim() };
  const rule: Rule = {
    id,
    name: draft.name.trim(),
    group: draft.group.trim(),
    enabled: true,
    agent: draft.agent || null,
    project: draft.sourceProject || null,
    condition,
  };
  const signature = JSON.stringify(rule);
  function change(key: keyof Draft, value: string) {
    version.current++;
    setDraft((d) => ({ ...d, [key]: value }));
    setResult(null);
    setError("");
  }
  async function tryRule(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    const attempt = ++version.current,
      controller = c.current;
    setBusy("preview");
    setResult(null);
    setError("");
    try {
      const value = preview(
        await api(
          "/v1/signals/preview",
          controller.signal,
          "POST",
          rule,
          project,
        ),
        project,
      );
      if (!controller.signal.aborted && attempt === version.current)
        setResult({ signature, value });
    } catch (e) {
      if (!controller.signal.aborted) {
        if (e instanceof ApiError && e.status === 401) onExpired();
        else if (attempt === version.current) setError(message(e));
      }
    } finally {
      if (!controller.signal.aborted) setBusy("");
    }
  }
  async function save() {
    if (busy || !result || result.signature !== signature) return;
    setBusy("save");
    setError("");
    const controller = c.current;
    try {
      await onSave({
        ...data.settings,
        custom: [...data.settings.custom, rule],
      });
      if (!controller.signal.aborted) onClose();
    } catch (e) {
      if (!controller.signal.aborted) {
        setError(message(e));
        setConflict(e instanceof ApiError && e.status === 409);
      }
    } finally {
      if (!controller.signal.aborted) setBusy("");
    }
  }
  async function reload() {
    setBusy("reload");
    try {
      await onReload();
      if (!c.current.signal.aborted) {
        setConflict(false);
        setError("");
      }
    } catch (e) {
      if (!c.current.signal.aborted) setError(message(e));
    } finally {
      if (!c.current.signal.aborted) setBusy("");
    }
  }
  const textRule =
    draft.kind === "prompt_contains" || draft.kind === "response_contains";
  return (
    <Dialog title="Create a signal" onClose={onClose} busy={busy !== ""}>
      <p>
        Choose something to notice in received turns. Try it on recent activity
        before turning it on.
      </p>
      <form className="signal-form" onSubmit={tryRule}>
        <label>
          Signal name
          <input
            autoFocus
            required
            maxLength={80}
            value={draft.name}
            disabled={busy === "save"}
            onChange={(e) => change("name", e.target.value)}
          />
        </label>
        <label>
          Group
          <input
            required
            maxLength={40}
            value={draft.group}
            disabled={busy === "save"}
            onChange={(e) => change("group", e.target.value)}
          />
        </label>
        <label>
          What should this notice?
          <select
            value={draft.kind}
            disabled={busy === "save"}
            onChange={(e) => change("kind", e.target.value)}
          >
            <option value="tool_errors">Several tools fail in one turn</option>
            <option value="tool_named">A particular tool is used</option>
            <option value="response_contains">
              The AI response contains a phrase
            </option>
            <option value="prompt_contains">
              Your message contains a phrase
            </option>
          </select>
        </label>
        {draft.kind === "tool_errors" ? (
          <label>
            Number of failed tools
            <input
              type="number"
              required
              min={1}
              max={100}
              step={1}
              value={draft.number}
              disabled={busy === "save"}
              onChange={(e) => change("number", e.target.value)}
            />
          </label>
        ) : (
          <label>
            {draft.kind === "tool_named" ? "Tool name" : "Phrase to match"}
            <input
              required
              maxLength={160}
              value={draft.text}
              disabled={busy === "save"}
              onChange={(e) => change("text", e.target.value)}
            />
          </label>
        )}
        <p className="muted">
          {textRule
            ? "Needs opted-in uploaded text. Turns without that text are counted as unavailable, not as non-matches."
            : "Uses completed tool names or statuses. Prompt and response text are not required."}
        </p>
        <details>
          <summary>Optional: limit the source activity</summary>
          <p>
            This rule belongs to this member project. These filters narrow the
            agent and source project label inside its captured turns.
          </p>
          <label>
            Agent
            <select
              value={draft.agent}
              disabled={busy === "save"}
              onChange={(e) => change("agent", e.target.value)}
            >
              <option value="">All captured agents</option>
              {data.agents.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          </label>
          <label>
            Source project label
            <select
              value={draft.sourceProject}
              disabled={busy === "save"}
              onChange={(e) => change("sourceProject", e.target.value)}
            >
              <option value="">All source project labels</option>
              {data.projects.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </label>
        </details>
        <button className="secondary" disabled={busy !== ""}>
          {busy === "preview"
            ? "Checking recent activity..."
            : "Try on recent activity"}
        </button>
      </form>
      {result && (
        <section className="signal-preview" aria-label="Preview results">
          <p role="status">
            {result.value.matches.length}{" "}
            {result.value.matches.length === 1 ? "match" : "matches"} in{" "}
            {result.value.evaluated} checked{" "}
            {result.value.evaluated === 1 ? "turn" : "turns"}.{" "}
            {result.value.unavailable} could not be checked because text was not
            uploaded.
          </p>
          <p className="muted">
            {result.value.moreHistory
              ? "More history exists; this preview covers the newest 100 scoped turns. "
              : ""}
            This is a preview, not an accuracy score. Saving checks newly
            received turns; it does not backfill this history.
          </p>
          {result.value.matches.length === 0 ? (
            <p>
              No matches in this preview. You can adjust the rule or save it for
              future activity.
            </p>
          ) : (
            <ul>
              {result.value.matches.map((m) => (
                <li key={m.id}>
                  <p>{m.summary}</p>
                  <a
                    href={monitorLink(
                      project,
                      "activity?" + new URLSearchParams({ event: m.id }),
                    )}
                  >
                    Open captured turn
                  </a>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {conflict && (
        <button
          className="secondary"
          disabled={!!busy}
          onClick={() => void reload()}
        >
          Reload current settings
        </button>
      )}
      <div className="dialog-actions">
        <button className="secondary" disabled={!!busy} onClick={onClose}>
          Cancel
        </button>
        <button
          disabled={
            !!busy || !result || result.signature !== signature || conflict
          }
          onClick={() => void save()}
        >
          {busy === "save" ? "Saving..." : "Save and turn on"}
        </button>
      </div>
    </Dialog>
  );
}
function AdjustCheck({
  check,
  settings,
  onSave,
  onReload,
  onClose,
}: {
  check: Check;
  settings: Settings;
  onSave: (s: Settings) => Promise<void>;
  onReload: () => Promise<void>;
  onClose: () => void;
}) {
  const [values, setValues] = useState(() =>
      Object.fromEntries(
        check.options.map((o) => [
          o.name,
          String(settings.options[check.id]?.[o.name] ?? o.default),
        ]),
      ),
    ),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [conflict, setConflict] = useState(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await onSave({
        ...settings,
        options: {
          ...settings.options,
          [check.id]: {
            ...settings.options[check.id],
            ...Object.fromEntries(
              Object.entries(values).map(([k, v]) => [k, Number(v)]),
            ),
          },
        },
      });
      if (alive.current) onClose();
    } catch (e) {
      if (alive.current) {
        setError(message(e));
        setConflict(e instanceof ApiError && e.status === 409);
      }
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  return (
    <Dialog title={"Adjust " + title(check.id)} onClose={onClose} busy={busy}>
      <p>
        Connected collectors use these choices when they next refresh settings.
        Existing findings and reviews are retained.
      </p>
      <form className="signal-form" onSubmit={submit}>
        {check.options.map((o) => (
          <label key={o.name}>
            {o.meaning}
            <input
              type="number"
              required
              min={o.min}
              max={o.max}
              step="any"
              value={values[o.name]}
              disabled={busy}
              onChange={(e) =>
                setValues((v) => ({ ...v, [o.name]: e.target.value }))
              }
            />
            <small>
              Allowed: {o.min} to {o.max}. Default: {o.default}.
            </small>
          </label>
        ))}
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        {conflict && (
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                await onReload();
                if (alive.current) {
                  setConflict(false);
                  setError("");
                }
              } catch (e) {
                if (alive.current) setError(message(e));
              } finally {
                if (alive.current) setBusy(false);
              }
            }}
          >
            Reload current settings
          </button>
        )}
        <div className="dialog-actions">
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={() =>
              setValues(
                Object.fromEntries(
                  check.options.map((o) => [o.name, String(o.default)]),
                ),
              )
            }
          >
            Use defaults
          </button>
          <button disabled={busy || conflict}>
            {busy ? "Saving..." : "Save choices"}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
export function SignalLibrary({
  project,
  canWrite,
  onExpired,
}: {
  project: string;
  canWrite: boolean;
  onExpired: () => void;
}) {
  const [data, setData] = useState<Library | null>(null),
    [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [query, setQuery] = useState(""),
    [group, setGroup] = useState(""),
    [kind, setKind] = useState("all"),
    [dialog, setDialog] = useState<"create" | Check | null>(null);
  const controller = useRef(new AbortController()),
    loadingRequest = useRef<AbortController | null>(null);
  async function reload() {
    const c = new AbortController();
    loadingRequest.current?.abort();
    loadingRequest.current = c;
    setLoading(true);
    setError("");
    try {
      const next = library(
        await api("/v1/summary", c.signal, "GET", undefined, project),
        project,
      );
      if (!c.signal.aborted) setData(next);
    } catch (e) {
      if (!c.signal.aborted) {
        if (e instanceof ApiError && e.status === 401) onExpired();
        else setError(message(e));
        throw e;
      }
    } finally {
      if (!c.signal.aborted) setLoading(false);
    }
  }
  useEffect(() => {
    controller.current = new AbortController();
    void reload().catch(() => {});
    return () => {
      controller.current.abort();
      loadingRequest.current?.abort();
    };
  }, [project]);
  useEffect(() => {
    if (!canWrite) setDialog(null);
  }, [canWrite]);
  async function save(cfg: Settings) {
    if (!canWrite || busy || loading)
      throw new Error("Refresh your project access before making changes.");
    const c = controller.current;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const settings = saved(
        await api("/v1/signals/settings", c.signal, "PUT", cfg, project),
        project,
      );
      if (!c.signal.aborted) {
        setData((d) => (d ? { ...d, settings } : d));
        setNotice(
          "Saved. Existing findings and reviews are retained. Custom rules check newly received turns; collectors refresh built-in choices.",
        );
      }
    } catch (e) {
      if (!c.signal.aborted) {
        if (e instanceof ApiError && e.status === 401) onExpired();
        else setError(message(e));
      }
      throw e;
    } finally {
      if (!c.signal.aborted) setBusy(false);
    }
  }
  if (!data)
    return (
      <section className="panel">
        {loading ? (
          <p role="status">Loading this project's signals...</p>
        ) : (
          <>
            <p role="alert" className="error">
              {error}
            </p>
            <button onClick={() => void reload().catch(() => {})}>
              Try again
            </button>
          </>
        )}
      </section>
    );
  const known = new Set([
    ...data.checks.map((s) => s.id),
    ...data.settings.custom.map((s) => s.id),
  ]);
  const rows = [
    ...data.checks.map((s) => ({
      id: s.id,
      name: title(s.id),
      group: groupName(s.category),
      detail: s.meaning,
      enabled: !data.settings.disabled.includes(s.id),
      kind: "built_in",
      rule: null as Rule | null,
      check: s as Check | null,
    })),
    ...data.settings.custom.map((s) => ({
      id: s.id,
      name: s.name,
      group: s.group,
      detail: description(s),
      enabled: s.enabled,
      kind: "custom",
      rule: s,
      check: null as Check | null,
    })),
    ...Object.keys(data.counts)
      .filter((id) => !known.has(id))
      .map((id) => ({
        id,
        name: title(id),
        group: "Other recorded signals",
        detail:
          "Received with captured activity. Its definition is unavailable; its evidence is still accessible.",
        enabled: null,
        kind: "recorded",
        rule: null as Rule | null,
        check: null as Check | null,
      })),
  ];
  const matching = rows.filter(
    (r) =>
      (!group || r.group === group) &&
      (kind === "all" || r.kind === kind) &&
      (r.name + " " + r.detail).toLowerCase().includes(query.toLowerCase()),
  );
  return (
    <section
      className="signal-workspace"
      aria-labelledby="signal-library-title"
    >
      <div className="section-heading">
        <div>
          <p className="eyebrow">Notice what matters</p>
          <h2 id="signal-library-title">Your signal library</h2>
          <p className="muted">
            Built-in checks, your rules and recorded findings for this project.
          </p>
        </div>
        <div className="signal-actions">
          <button
            className="secondary"
            disabled={busy || loading}
            onClick={() => void reload().catch(() => {})}
          >
            {loading ? "Refreshing..." : "Refresh signals"}
          </button>
          {canWrite && (
            <button
              disabled={busy || loading || data.settings.custom.length >= 32}
              onClick={() => setDialog("create")}
            >
              <Icon name="plus" />
              Create a signal
            </button>
          )}
        </div>
      </div>
      {!canWrite && (
        <p className="notice">
          You can explore signals and evidence. A member, admin or owner can
          change this project's checks.
        </p>
      )}
      {data.settings.custom.length >= 32 && (
        <p className="notice">
          This version supports 32 custom signals. Saved definitions remain
          available so earlier findings keep their meaning.
        </p>
      )}
      <details className="overview-explainer">
        <summary>How signals become useful evidence</summary>
        <p>
          A signal points to work worth reviewing. It is not a confirmed
          failure, a training label or an automatic agent change. Your review
          stays attached to the captured evidence.
        </p>
        <p>
          This library manages transcript checks.{" "}
          <a href={monitorLink(project, "activity?kind=sdk")}>
            SDK findings and reviews
          </a>{" "}
          remain available in SDK activity. Pausing a check keeps earlier
          findings. To change a saved custom definition, create a new signal.
        </p>
      </details>
      <div className="signal-filters">
        <label>
          Find a signal
          <input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <label>
          Group
          <select value={group} onChange={(e) => setGroup(e.target.value)}>
            <option value="">All groups</option>
            {[...new Set(rows.map((r) => r.group))].sort().map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        </label>
        <label>
          Signal type
          <select value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="all">All signals</option>
            <option value="built_in">Built-in</option>
            <option value="custom">Your signals</option>
            <option value="recorded">Other recorded</option>
          </select>
        </label>
      </div>
      {error && (
        <p className="error" role="alert">
          {error} Use Refresh signals to load the latest saved settings.
        </p>
      )}
      {notice && (
        <p role="status" className="notice">
          {notice}
        </p>
      )}
      {matching.length === 0 ? (
        <div className="panel empty">
          <h3>No matching signals</h3>
          <p>
            Try another group or search, or create a signal for something you
            want to notice.
          </p>
        </div>
      ) : (
        <div className="signal-table-wrap">
          <table className="signal-table">
            <caption className="sr-only">Signals in this project</caption>
            <thead>
              <tr>
                <th scope="col">Group</th>
                <th scope="col">Name</th>
                <th scope="col">Source</th>
                <th scope="col">Status</th>
                <th scope="col">Matches</th>
                <th scope="col">Actions</th>
              </tr>
            </thead>
            <tbody>
              {matching.map((r) => (
                <tr className="signal-row" key={r.id}>
                  <td data-label="Group">
                    <span className="badge">{r.group}</span>
                  </td>
                  <td data-label="Name">
                    <h3>
                      <a href={findings(project, r.id)}>{r.name}</a>
                    </h3>
                    <details>
                      <summary>Definition and scope</summary>
                      <p>{r.detail}</p>
                      {r.rule && (
                        <p>
                          {r.rule.agent || "All captured agents"} /{" "}
                          {r.rule.project || "All source project labels"}
                        </p>
                      )}
                      {r.check && (
                        <p>
                          Rule version {r.check.version}. Runs in the collector,
                          which refreshes your choices. Historical findings are
                          not recalculated.
                        </p>
                      )}
                    </details>
                  </td>
                  <td data-label="Source">
                    {r.kind === "custom"
                      ? "Your signal"
                      : r.kind === "recorded"
                        ? "Recorded"
                        : "Built-in"}
                  </td>
                  <td data-label="Status">
                    {r.enabled === null
                      ? "Definition unavailable"
                      : r.enabled
                        ? "On"
                        : "Paused"}
                  </td>
                  <td data-label="Matches">
                    <span>
                      {(data.counts[r.id] ?? 0).toLocaleString()} recorded{" "}
                      {(data.counts[r.id] ?? 0) === 1 ? "match" : "matches"}
                    </span>
                  </td>
                  <td data-label="Actions">
                    <div className="signal-actions">
                      <a href={findings(project, r.id)}>
                        Open signal dashboard
                      </a>
                      {canWrite && r.enabled !== null && (
                        <button
                          className="secondary"
                          disabled={busy || loading}
                          aria-label={
                            (r.enabled ? "Pause " : "Turn on ") + r.name
                          }
                          onClick={() => {
                            const cfg = r.rule
                              ? {
                                  ...data.settings,
                                  custom: data.settings.custom.map((s) =>
                                    s.id === r.id
                                      ? { ...s, enabled: !s.enabled }
                                      : s,
                                  ),
                                }
                              : {
                                  ...data.settings,
                                  disabled: r.enabled
                                    ? [...data.settings.disabled, r.id]
                                    : data.settings.disabled.filter(
                                        (id) => id !== r.id,
                                      ),
                                };
                            void save(cfg).catch(() => {});
                          }}
                        >
                          {r.enabled ? "Pause" : "Turn on"}
                        </button>
                      )}
                      {canWrite && r.check && r.check.options.length > 0 && (
                        <button
                          className="secondary"
                          disabled={busy || loading}
                          onClick={() => setDialog(r.check)}
                        >
                          Adjust check
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="muted">
        Source: this project's saved signal definitions and received transcript
        findings. Refresh to load newer counts.
      </p>
      {dialog === "create" && canWrite && (
        <CreateSignal
          project={project}
          data={data}
          onSave={save}
          onReload={reload}
          onClose={() => setDialog(null)}
          onExpired={onExpired}
        />
      )}
      {dialog && dialog !== "create" && canWrite && (
        <AdjustCheck
          check={dialog}
          settings={data.settings}
          onSave={save}
          onReload={reload}
          onClose={() => setDialog(null)}
        />
      )}
    </section>
  );
}
