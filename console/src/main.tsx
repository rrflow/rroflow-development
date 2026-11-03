import { StrictMode, useEffect, useRef, useState, type FormEvent } from "react";
import { createRoot } from "react-dom/client";
import {
  api,
  ApiError,
  createdId,
  createdKey,
  keys,
  monitorLink,
  overview,
  type Overview,
  type Organization,
  type Project,
  type ProjectKey,
} from "./api";
import { Dialog, FlowArt, Icon } from "./ui";
import "./styles.css";
import { AccountPage } from "./account";
import { DevicePage } from "./device";
import { ProjectOverview } from "./project-overview";

const brand = (name: string) =>
  document.querySelector(`[data-brand="${name}"]`)?.textContent || "";
const identity = {
  name: brand("name"),
  wordmark: brand("wordmark"),
  mark: brand("mark"),
  tagline: brand("tagline"),
};
const canManage = (o: Organization) => o.role === "owner" || o.role === "admin";
const message = (e: unknown) =>
  e instanceof Error ? e.message : "Something went wrong. Please try again.";
const cancelled = (e: unknown) => e instanceof Error && e.name === "AbortError";
function route() {
  return new URLSearchParams(location.hash.slice(1));
}
function destination(org: string, project?: string) {
  const q = new URLSearchParams({ org });
  if (project) q.set("project", project);
  return "#" + q;
}
function when(value: string | null) {
  if (!value) return "Not used yet";
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "Time not reported"
    : date.toLocaleString();
}

function NameDialog({
  kind,
  onClose,
  onCreate,
  onExpired,
}: {
  kind: "organization" | "project";
  onClose: () => void;
  onCreate: (name: string, signal: AbortSignal) => Promise<void>;
  onExpired: () => void;
}) {
  const [name, setName] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const controller = useRef(new AbortController()),
    pending = useRef(false);
  useEffect(() => {
    controller.current = new AbortController();
    return () => controller.current.abort();
  }, []);
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (pending.current || !name.trim()) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      await onCreate(name.trim(), controller.current.signal);
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) onExpired();
      else if (!cancelled(e)) setError(message(e));
    } finally {
      pending.current = false;
      if (!controller.current.signal.aborted) setBusy(false);
    }
  }
  return (
    <Dialog
      title={
        kind === "organization" ? "Create an organization" : "Create a project"
      }
      onClose={onClose}
      busy={busy}
    >
      <p className="muted">
        {kind === "organization"
          ? "Keep a team or a separate area of work together. You will be its owner."
          : "Give this work a home. Its activity, signals and connection keys stay together."}
      </p>
      <form onSubmit={submit}>
        <label htmlFor="new-name">
          {kind === "organization" ? "Organization name" : "Project name"}
        </label>
        <input
          id="new-name"
          autoFocus
          required
          maxLength={100}
          value={name}
          disabled={busy}
          onChange={(e) => setName(e.target.value)}
          autoComplete="off"
          placeholder={
            kind === "organization"
              ? "e.g. Studio North"
              : "e.g. Customer website"
          }
        />
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <div className="dialog-actions">
          <button
            type="button"
            className="secondary"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button disabled={busy || !name.trim()}>
            {busy ? "Creating…" : `Create ${kind}`}
          </button>
        </div>
      </form>
    </Dialog>
  );
}

function KeyDialog({
  project,
  onClose,
  onCreated,
  onExpired,
}: {
  project: Project;
  onClose: () => void;
  onCreated: () => void;
  onExpired: () => void;
}) {
  const [label, setLabel] = useState(""),
    [busy, setBusy] = useState(false),
    [secret, setSecret] = useState(""),
    [error, setError] = useState(""),
    [copy, setCopy] = useState("");
  const controller = useRef(new AbortController()),
    pending = useRef(false),
    secretField = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    controller.current = new AbortController();
    return () => controller.current.abort();
  }, []);
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (pending.current || !label.trim()) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const value = createdKey(
        await api(
          `/v1/projects/${encodeURIComponent(project.id)}/keys`,
          controller.current.signal,
          "POST",
          { label: label.trim() },
        ),
      );
      if (!controller.current.signal.aborted) {
        setSecret(value.key);
        onCreated();
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) onExpired();
      else if (!cancelled(e)) setError(message(e));
    } finally {
      pending.current = false;
      if (!controller.current.signal.aborted) setBusy(false);
    }
  }
  async function copyKey() {
    try {
      await navigator.clipboard.writeText(secret);
      if (!controller.current.signal.aborted)
        setCopy("Copied. Store it somewhere private.");
    } catch {
      secretField.current?.focus();
      secretField.current?.select();
      setCopy("Copy was unavailable. Select the key and copy it manually.");
    }
  }
  return (
    <Dialog
      title={secret ? "Save your connection key" : "Create a connection key"}
      onClose={onClose}
      busy={busy}
    >
      {secret ? (
        <>
          <p>
            This is the only time the full key is shown. It sends records to{" "}
            <strong>{project.name}</strong> and cannot read your activity.
          </p>
          <label htmlFor="one-time-key">One-time connection key</label>
          <textarea
            ref={secretField}
            id="one-time-key"
            readOnly
            value={secret}
            rows={3}
            spellCheck={false}
          />
          <p className="muted">
            Keep it out of source code and shared notes. Closing this dialog
            clears the key from this page.
          </p>
          <p role="status">{copy}</p>
          <div className="dialog-actions">
            <button className="secondary" onClick={copyKey}>
              Copy key
            </button>
            <button onClick={onClose}>I've saved this key</button>
          </div>
        </>
      ) : (
        <form onSubmit={submit}>
          <p className="muted">
            For a collector, server or CI job that sends to this project. A key
            is a credential, not a person.
          </p>
          <label htmlFor="key-label">A name you will recognize</label>
          <input
            id="key-label"
            autoFocus
            required
            maxLength={100}
            autoComplete="off"
            placeholder="e.g. Work laptop"
            value={label}
            disabled={busy}
            onChange={(e) => setLabel(e.target.value)}
          />
          {error && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
          <div className="dialog-actions">
            <button
              type="button"
              className="secondary"
              disabled={busy}
              onClick={onClose}
            >
              Cancel
            </button>
            <button disabled={busy || !label.trim()}>
              {busy ? "Creating…" : "Create key"}
            </button>
          </div>
        </form>
      )}
    </Dialog>
  );
}

function ProjectDetail({
  project,
  organization,
  onExpired,
}: {
  project: Project;
  organization: Organization;
  onExpired: () => void;
}) {
  const [items, setItems] = useState<ProjectKey[] | null>(null),
    [error, setError] = useState(""),
    [refresh, setRefresh] = useState(0),
    [create, setCreate] = useState(false),
    [revoke, setRevoke] = useState<ProjectKey | null>(null),
    [busy, setBusy] = useState(false);
  const controller = useRef(new AbortController()),
    alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    controller.current = new AbortController();
    return () => {
      alive.current = false;
      controller.current.abort();
    };
  }, []);
  useEffect(() => {
    const c = new AbortController();
    setItems(null);
    setError("");
    if (canManage(organization))
      api(`/v1/projects/${encodeURIComponent(project.id)}/keys`, c.signal)
        .then(keys)
        .then((v) => {
          if (!c.signal.aborted) setItems(v);
        })
        .catch((e) => {
          if (e instanceof ApiError && e.status === 401) onExpired();
          else if (!cancelled(e)) setError(message(e));
        });
    return () => c.abort();
  }, [project.id, organization.role, refresh]);
  async function revokeKey() {
    if (!revoke || busy) return;
    setBusy(true);
    setError("");
    try {
      await api(
        `/v1/projects/${encodeURIComponent(project.id)}/keys/${encodeURIComponent(revoke.id)}`,
        controller.current.signal,
        "DELETE",
      );
      if (alive.current) {
        setRevoke(null);
        setRefresh((n) => n + 1);
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) onExpired();
      else if (!cancelled(e)) setError(message(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  return (
    <>
      <a className="back-link" href={destination(organization.id)}>
        ← All projects
      </a>
      <header className="page-title">
        <div>
          <p className="eyebrow">{organization.name} / Project</p>
          <h1>{project.name}</h1>
          <p className="muted">
            One place for this project's AI activity and evidence.
          </p>
        </div>
        <a className="button" href={monitorLink(project.id)}>
          Open monitoring <Icon name="arrow" />
        </a>
      </header>
      <div className="quick-links">
        {[
          ["activity", "Activity", "See the turns that arrived."],
          ["signals", "Findings", "Follow a signal to its evidence."],
          ["sources", "Sources", "Check delivery and waiting records."],
          ["datasets", "Datasets", "Review consent and collected examples."],
        ].map(([view, label, detail]) => (
          <a href={monitorLink(project.id, view)} key={view}>
            <Icon name="pulse" />
            <strong>{label}</strong>
            <span>{detail}</span>
            <Icon name="arrow" />
          </a>
        ))}
      </div>
      <ProjectOverview project={project.id} onExpired={onExpired} />
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">Connect your tools</p>
            <h2>Connection keys</h2>
          </div>
          {canManage(organization) && (
            <button onClick={() => setCreate(true)}>
              <Icon name="plus" /> Create key
            </button>
          )}
        </div>
        <p className="muted">
          Keys can send records to this project. They cannot read your activity.
          Never put one in a public repository.
        </p>
        {!canManage(organization) ? (
          <div className="empty">
            <Icon name="key" />
            <h3>An admin manages connections</h3>
            <p>
              Your {organization.role} role can open monitoring. Ask an
              organization admin to manage keys.
            </p>
          </div>
        ) : error && !revoke ? (
          <div className="error" role="alert">
            {error}{" "}
            <button
              className="text-button"
              onClick={() => setRefresh((n) => n + 1)}
            >
              Try again
            </button>
          </div>
        ) : items === null ? (
          <p role="status">Loading connection keys…</p>
        ) : items.length === 0 ? (
          <div className="empty">
            <Icon name="key" />
            <h3>No connection keys yet</h3>
            <p>
              Create one when you are ready to connect a collector or server.
            </p>
          </div>
        ) : (
          <ul className="key-list">
            {items.map((k) => (
              <li key={k.id}>
                <span className="tile-icon">
                  <Icon name="key" />
                </span>
                <div className="key-copy">
                  <strong>{k.label}</strong>
                  <code>{k.prefix}…</code>
                  <span className="muted">
                    {k.revoked_at
                      ? "Revoked " + when(k.revoked_at)
                      : "Last used: " + when(k.last_used_at)}
                  </span>
                </div>
                <span className={"badge " + (k.revoked_at ? "" : "positive")}>
                  {k.revoked_at ? "Revoked" : "Active"}
                </span>
                {!k.revoked_at && (
                  <button
                    className="text-button danger"
                    aria-label={`Revoke ${k.label}`}
                    onClick={() => {
                      setError("");
                      setRevoke(k);
                    }}
                  >
                    Revoke
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
        <details>
          <summary>Advanced: project identity</summary>
          <p className="muted">
            Use this non-secret ID when selecting the project's account-scoped
            API. The server still checks membership.
          </p>
          <code className="identifier">{project.id}</code>
        </details>
      </section>
      {create && canManage(organization) && (
        <KeyDialog
          project={project}
          onClose={() => {
            setCreate(false);
            setRefresh((n) => n + 1);
          }}
          onCreated={() => setRefresh((n) => n + 1)}
          onExpired={onExpired}
        />
      )}{" "}
      {revoke && canManage(organization) && (
        <Dialog
          title={`Revoke ${revoke.label}?`}
          busy={busy}
          onClose={() => {
            setRevoke(null);
            setError("");
          }}
        >
          <p>
            Uploads using this key will stop. Existing records stay. Waiting
            uploads need an authorized connection before they can resume.
          </p>
          {error && (
            <p role="alert" className="error">
              {error}
            </p>
          )}
          <div className="dialog-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => {
                setRevoke(null);
                setError("");
              }}
            >
              Keep key
            </button>
            <button
              className="danger-button"
              disabled={busy}
              onClick={revokeKey}
            >
              {busy ? "Revoking…" : "Revoke key"}
            </button>
          </div>
        </Dialog>
      )}
    </>
  );
}

function App() {
  const [data, setData] = useState<Overview | null>(null),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true),
    [signedOut, setSignedOut] = useState(false),
    [signIn, setSignIn] = useState<string | null>(null),
    [configError, setConfigError] = useState(""),
    [locationState, setLocationState] = useState(route()),
    [dialog, setDialog] = useState<"organization" | "project" | null>(null),
    [help, setHelp] = useState(false),
    [outBusy, setOutBusy] = useState(false);
  const controller = useRef(new AbortController());
  function expired() {
    controller.current.abort();
    controller.current = new AbortController();
    setData(null);
    setDialog(null);
    setHelp(false);
    setSignedOut(true);
    setLoading(false);
    setError("Your session has ended. Sign in again.");
  }
  async function reload() {
    const signal = controller.current.signal;
    setError("");
    setLoading(true);
    try {
      const value = overview(await api("/v1/me", signal));
      if (!signal.aborted) {
        setData(value);
        setSignedOut(false);
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) expired();
      else if (!cancelled(e)) setError(message(e));
    } finally {
      if (!signal.aborted) setLoading(false);
    }
  }
  useEffect(() => {
    controller.current = new AbortController();
    void reload();
    const c = new AbortController();
    fetch("/auth/config", { credentials: "same-origin", signal: c.signal })
      .then(async (r) => {
        if (!r.ok) throw new Error("Sign-in settings are unavailable.");
        const v = await r.json();
        if (c.signal.aborted) return;
        if (v.enabled && typeof v.member_origin === "string") {
          const u = new URL(v.member_origin);
          if (
            u.protocol !== "https:" ||
            u.username ||
            u.password ||
            u.search ||
            u.hash ||
            u.pathname !== "/"
          )
            throw new Error("Sign-in configuration is invalid.");
          setSignIn(u.origin + "/auth/login");
        } else
          setConfigError(
            "Account sign-in is not configured on this deployment yet.",
          );
      })
      .catch((e) => {
        if (!cancelled(e)) setConfigError(message(e));
      });
    const change = () => {
      setLocationState(route());
      setDialog(null);
    };
    window.addEventListener("hashchange", change);
    return () => {
      c.abort();
      controller.current.abort();
      window.removeEventListener("hashchange", change);
    };
  }, []);
  const organization =
    data?.organizations.find((o) => o.id === locationState.get("org")) ||
    (!locationState.has("org") ? data?.organizations[0] : undefined);
  const project = organization?.projects.find(
    (p) => p.id === locationState.get("project"),
  );
  async function create(name: string, signal: AbortSignal) {
    const path =
      dialog === "organization"
        ? "/v1/organizations"
        : `/v1/organizations/${encodeURIComponent(organization!.id)}/projects`;
    const id = createdId(await api(path, signal, "POST", { name }));
    if (signal.aborted) return;
    const kind = dialog;
    setDialog(null);
    await reload();
    location.hash =
      kind === "organization"
        ? destination(id)
        : destination(organization!.id, id);
  }
  async function logout() {
    if (outBusy) return;
    setOutBusy(true);
    try {
      const r = await fetch("/auth/logout", {
        method: "POST",
        credentials: "same-origin",
        headers: { "X-Rroflow-Request": "dashboard" },
      });
      if (!r.ok)
        throw new Error("Sign-out could not be confirmed. Please try again.");
      controller.current.abort();
      setData(null);
      setDialog(null);
      setHelp(false);
      setSignedOut(true);
      setLoading(false);
      setError("");
    } catch (e) {
      setError(message(e));
    } finally {
      setOutBusy(false);
    }
  }
  return (
    <>
      <a
        className="skip"
        href="#main"
        onClick={(e) => {
          e.preventDefault();
          document.getElementById("main")?.focus();
        }}
      >
        Skip to content
      </a>
      <div className="app-frame">
        <aside className="sidebar">
          <a className="brand" href="/">
            <span className="brand-mark">{identity.mark}</span>
            <span>{identity.wordmark}</span>
          </a>
          <p className="sidebar-label">YOUR SPACE</p>
          <a className="nav-active" href="/app/manage">
            <Icon name="grid" /> Organizations
          </a>
          <a href={project ? monitorLink(project.id) : "/app"}>
            <Icon name="pulse" /> Monitoring
          </a>
          <div className="sidebar-note">
            <span className="dot" />
            <p>
              Clarity starts with
              <br />
              <strong>a place for your work.</strong>
            </p>
          </div>
          <button className="help-button" onClick={() => setHelp(true)}>
            <Icon name="help" /> How it fits together
          </button>
          <p className="tagline">{identity.tagline}</p>
        </aside>
        <div className="workspace">
          <header className="topbar">
            <span className="breadcrumb">
              Workspace <span aria-hidden="true">/</span> Organization &
              projects
            </span>
            {data && (
              <div className="account">
                <span>{data.user.name || "Your account"}</span>
                <a href="/app/device">Collectors</a>
                <button
                  className="text-button"
                  disabled={outBusy}
                  onClick={logout}
                >
                  {outBusy ? "Signing out…" : "Sign out"}
                </button>
              </div>
            )}
          </header>
          <main id="main" tabIndex={-1}>
            {!data ? (
              <section className="welcome panel">
                <FlowArt />
                <p className="eyebrow">Your AI, organized</p>
                <h1>
                  {loading
                    ? "Opening your workspace…"
                    : signedOut
                      ? "Your workspace starts here"
                      : "We could not open your workspace"}
                </h1>
                <p className="muted">
                  Keep your projects, monitoring and connections together. Sign
                  in with your account to continue.
                </p>
                {loading ? (
                  <p role="status">Checking your membership…</p>
                ) : (
                  <>
                    {error && (
                      <p className="error" role="alert">
                        {error}
                      </p>
                    )}
                    {signIn ? (
                      <a className="button" href={signIn}>
                        Sign in with your account <Icon name="arrow" />
                      </a>
                    ) : (
                      <p role="status">
                        {configError || "Checking sign-in settings…"}
                      </p>
                    )}
                    <button
                      className="text-button"
                      onClick={() => {
                        controller.current = new AbortController();
                        void reload();
                      }}
                    >
                      Check again
                    </button>
                  </>
                )}
              </section>
            ) : (
              <>
                {error && (
                  <div className="error" role="alert">
                    {error}{" "}
                    <button
                      className="text-button"
                      onClick={() => void reload()}
                    >
                      Refresh
                    </button>
                  </div>
                )}
                <div className="organization-picker">
                  <label htmlFor="organization">Organization</label>
                  <select
                    id="organization"
                    value={organization?.id || ""}
                    onChange={(e) => {
                      location.hash = destination(e.target.value);
                    }}
                  >
                    {!organization && (
                      <option value="" disabled>
                        Select an organization
                      </option>
                    )}
                    {data.organizations.map((o) => (
                      <option key={o.id} value={o.id}>
                        {o.name}
                      </option>
                    ))}
                  </select>
                  <button
                    className="secondary"
                    onClick={() => setDialog("organization")}
                  >
                    <Icon name="plus" /> New organization
                  </button>
                </div>
                {!organization ? (
                  <div className="empty panel">
                    <h1>This organization is unavailable</h1>
                    <p>
                      Select an organization you belong to. Your permissions may
                      have changed.
                    </p>
                  </div>
                ) : locationState.has("project") && !project ? (
                  <div className="empty panel">
                    <h1>This project is unavailable</h1>
                    <a href={destination(organization.id)}>Back to projects</a>
                  </div>
                ) : project ? (
                  <ProjectDetail
                    key={project.id}
                    project={project}
                    organization={organization}
                    onExpired={expired}
                  />
                ) : (
                  <>
                    <section className="hero">
                      <div>
                        <p className="eyebrow">Make room for good work</p>
                        <h1>
                          Your work,
                          <br />
                          <em>organized.</em>
                        </h1>
                        <p className="hero-description">
                          A project brings your AI activity, signals and
                          evidence into one place. Choose where you want to go
                          next.
                        </p>
                        <div className="chips">
                          <span className="badge">
                            {organization.personal
                              ? "Personal organization"
                              : "Team organization"}
                          </span>
                          <span className="badge">
                            Your role: {organization.role}
                          </span>
                        </div>
                      </div>
                      <FlowArt />
                    </section>
                    <section aria-labelledby="projects-heading">
                      <div className="section-heading">
                        <div>
                          <p className="eyebrow">{organization.name}</p>
                          <h2 id="projects-heading">
                            Projects{" "}
                            <span className="count">
                              {organization.projects.length}
                            </span>
                          </h2>
                        </div>
                        {canManage(organization) && (
                          <button onClick={() => setDialog("project")}>
                            <Icon name="plus" /> New project
                          </button>
                        )}
                      </div>
                      {organization.projects.length === 0 ? (
                        <div className="empty panel">
                          <Icon name="grid" />
                          <h3>A fresh space for your next project</h3>
                          <p>
                            {canManage(organization)
                              ? "Create a project, then connect a source to start receiving activity."
                              : "An organization admin can create the first project."}
                          </p>
                        </div>
                      ) : (
                        <div className="project-grid">
                          {organization.projects.map((p) => (
                            <article className="project-card" key={p.id}>
                              <div className="card-top">
                                <span className="tile-icon">
                                  <Icon name="grid" />
                                </span>
                                {p.id === data.default_project && (
                                  <span className="badge">Default project</span>
                                )}
                              </div>
                              <h3>
                                <a href={destination(organization.id, p.id)}>
                                  {p.name}
                                </a>
                              </h3>
                              <p>
                                Activity, signals and evidence
                                <br />
                                for this space.
                              </p>
                              <div className="card-actions">
                                <a href={monitorLink(p.id)}>
                                  Open monitoring <Icon name="arrow" />
                                </a>
                                <a
                                  className="subtle-link"
                                  href={destination(organization.id, p.id)}
                                >
                                  {canManage(organization)
                                    ? "Manage connections"
                                    : "Project details"}
                                </a>
                              </div>
                            </article>
                          ))}
                        </div>
                      )}
                    </section>
                    <section className="next-step">
                      <span className="tile-icon">
                        <Icon name="pulse" />
                      </span>
                      <div>
                        <h3>Connect first. See what actually arrives.</h3>
                        <p>
                          A project is a place to collect work. Creating one
                          does not install a collector or prove your AI is
                          reporting.
                        </p>
                      </div>
                      <button
                        className="text-button"
                        onClick={() => setHelp(true)}
                      >
                        How it works <Icon name="arrow" />
                      </button>
                    </section>
                  </>
                )}
              </>
            )}
            <footer className="page-footer">
              {data
                ? "Source: your signed-in membership and project records."
                : "Private project data is shown only after sign-in."}
            </footer>
          </main>
        </div>
      </div>
      {dialog && data && (
        <NameDialog
          kind={dialog}
          onClose={() => setDialog(null)}
          onCreate={create}
          onExpired={expired}
        />
      )}{" "}
      {help && (
        <Dialog title="A home for your AI work" onClose={() => setHelp(false)}>
          <ol className="help-steps">
            <li>
              <strong>Organization</strong>
              <p>
                The people who can work together, and the role each person has.
              </p>
            </li>
            <li>
              <strong>Project</strong>
              <p>
                A separate space for received activity, signals and evidence.
              </p>
            </li>
            <li>
              <strong>Connection</strong>
              <p>
                A collector or SDK sends to that project. A connection key can
                send records, but cannot read your activity.
              </p>
            </li>
          </ol>
          <p className="muted">
            Creating a project or key does not install anything, connect a
            provider subscription, grant a paid plan or start training.
          </p>
          <button onClick={() => setHelp(false)}>Got it</button>
        </Dialog>
      )}
    </>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {location.pathname === "/app/account" ? <AccountPage name={identity.name} /> : location.pathname === "/app/device" ? <DevicePage name={identity.name} /> : <App />}
  </StrictMode>,
);
