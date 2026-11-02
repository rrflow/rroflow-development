import { useEffect, useRef, useState, type FormEvent } from "react";
import { api, ApiError, overview, type Overview } from "./api";
import { Dialog, FlowArt } from "./ui";

type Inspection = { account_email: string; client_name: string; device_name: string; expires_at: string; scope: "capture" };
type Grant = { id: string; client_name: string; device_name: string; project_id: string; created_at: string; expires_at: string; last_used_at: string };
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("The connection response could not be read.");
  return value as Record<string, unknown>;
}
function text(value: unknown): string {
  if (typeof value !== "string") throw new Error("The connection response is incomplete.");
  return value;
}
function date(value: unknown): string {
  const s = text(value);
  if (!Number.isFinite(Date.parse(s))) throw new Error("The connection expiry could not be read.");
  return s;
}
function inspect(value: unknown): Inspection {
  const v = object(value);
  if (v.scope !== "capture") throw new Error("This collector requested an unsupported permission.");
  return { account_email: text(v.account_email), client_name: text(v.client_name), device_name: text(v.device_name), expires_at: date(v.expires_at), scope: "capture" };
}
function grants(value: unknown): Grant[] {
  if (!Array.isArray(value)) throw new Error("Connected collectors could not be read.");
  return value.map(raw => {
    const v = object(raw);
    return { id: text(v.id), client_name: text(v.client_name), device_name: text(v.device_name), project_id: text(v.project_id), created_at: date(v.created_at), expires_at: date(v.expires_at), last_used_at: date(v.last_used_at) };
  });
}
function explain(error: unknown): string {
  const message = error instanceof Error ? error.message : "Please try again.";
  if (message === "invalid_request") return "This code or connection is no longer available. Start again from your terminal, or refresh the list.";
  if (message === "access_denied") return "This connection could not be approved. Check your project access and try again.";
  if (message === "invalid_client") return "This collector is not registered with this service.";
  if (message === "temporarily_unavailable") return "The connection service is busy or unavailable. Please try again shortly.";
  return message;
}
const endpoint = (action: string) => "/auth/native/device/" + action;

export function DevicePage({ name }: { name: string }) {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [member, setMember] = useState<Overview | null>(null);
  const [connections, setConnections] = useState<Grant[] | null>(null);
  const [signIn, setSignIn] = useState(false);
  const [code, setCode] = useState("");
  const [inspection, setInspection] = useState<Inspection | null>(null);
  const [project, setProject] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [disconnect, setDisconnect] = useState<Grant | null>(null);
  const [now, setNow] = useState(Date.now());
  const pending = useRef(false), controller = useRef(new AbortController());
  const projects = member?.organizations.filter(o => o.role !== "viewer").flatMap(o => o.projects.map(p => ({ ...p, organization: o.name }))) || [];
  const expired = !!inspection && Date.parse(inspection.expires_at) <= now;

  function failed(e: unknown) {
    if (controller.current.signal.aborted) return;
    if (e instanceof ApiError && e.status === 401) {
      setSignIn(true); setMember(null); setConnections(null); setInspection(null); setDisconnect(null);
    } else setError(explain(e));
  }
  async function reload(signal: AbortSignal) {
    const [me, list] = await Promise.all([api("/v1/me", signal), api(endpoint("grants"), signal, "POST")]);
    const data = overview(me), items = grants(list);
    if (signal.aborted) return;
    setMember(data); setConnections(items); setSignIn(false);
  }
  useEffect(() => {
    const c = new AbortController(); controller.current = c;
    api(endpoint("config"), c.signal).then(async raw => {
      const config = object(raw);
      if (typeof config.enabled !== "boolean") throw new Error("Connection options could not be read.");
      if (c.signal.aborted) return;
      setEnabled(config.enabled);
      if (config.enabled) await reload(c.signal);
    }).catch(e => { if (!c.signal.aborted) failed(e); });
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => { c.abort(); window.clearInterval(timer); };
  }, []);
  async function act(operation: () => Promise<void>) {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(""); setNotice("");
    try { await operation(); } catch (e) { failed(e); }
    finally { pending.current = false; if (!controller.current.signal.aborted) setBusy(false); }
  }
  function review(e: FormEvent) {
    e.preventDefault();
    void act(async () => {
      const data = inspect(await api(endpoint("inspect"), controller.current.signal, "POST", { user_code: code }));
      setInspection(data); setNow(Date.now());
      setProject(projects.find(p => p.id === member?.default_project)?.id || projects[0]?.id || "");
    });
  }
  function decide(approve: boolean) {
    void act(async () => {
      await api(endpoint("decide"), controller.current.signal, "POST", { user_code: code, project_id: project, approve });
      setInspection(null); setCode("");
      setNotice(approve ? "Approval saved. Return to your terminal to finish connecting." : "Connection declined. No collector was connected.");
    });
  }
  return <main className="device-layout">
    <header className="device-heading"><a href="/app/manage" className="wordmark">{name}</a><a href="/app/manage">Back to your projects</a></header>
    <section className="device-intro"><div><p className="eyebrow">Connect your tools</p><h1>Bring your AI activity home.</h1><p>Approve the collector you started, and choose where its activity belongs.</p></div><FlowArt /></section>
    {error && !disconnect && <p role="alert" className="error">{error}</p>}
    {notice && <p role="status" className="notice">{notice}</p>}
    {enabled === null && !error && <p role="status">Loading connection options...</p>}
    {enabled === false && <section className="panel"><h2>Collector sign-in is not enabled here yet</h2><p>This deployment has not enabled native collector connections.</p></section>}
    {signIn && <section className="panel"><h2>Sign in before connecting</h2><p>Use your RROFLOW email account to approve this collector.</p><a className="button" href="/app/account?next=device">Sign in to connect</a></section>}
    {enabled && !member && !signIn && !error && <p role="status">Loading your projects and collectors...</p>}
    {enabled && member && <div className="device-panels">
      <section className="panel" aria-labelledby="device-approval-title"><h2 id="device-approval-title">{inspection ? "Review this connection" : "Enter the code from your terminal"}</h2>
        {!inspection ? <form className="device-form" onSubmit={review}>
          <label htmlFor="device-code">Connection code</label><input id="device-code" autoComplete="off" autoCapitalize="characters" spellCheck={false} value={code} onChange={e => setCode(e.target.value)} maxLength={11} required placeholder="ABCDE-FGHJK" disabled={busy} />
          <p className="muted">Only use a code from a collector you started. A code sent by someone else could connect their device to your project.</p><button disabled={busy || !code.trim()}>{busy ? "Checking..." : "Review connection"}</button>
        </form> : <div className="device-form">
          <p>Signed in as <strong>{inspection.account_email}</strong></p><p><strong>{inspection.client_name}</strong></p><p>Device name supplied by the collector: <strong>{inspection.device_name}</strong></p><p>Code: <code>{code.toUpperCase()}</code></p>
          <label htmlFor="device-project">Send activity to</label><select id="device-project" value={project} onChange={e => setProject(e.target.value)} disabled={busy || expired}>{projects.map(p => <option key={p.id} value={p.id}>{p.organization} / {p.name}</option>)}</select>
          {!projects.length && <p>You need a project with permission to send activity before connecting.</p>}
          <p>This collector can send captures and delivery reports, and read this project's capture settings. It cannot read your conversations, manage your account, or approve dataset collection.</p>
          <p className="muted">Your plan limits and content-sharing choices still apply. This approval does not connect an AI subscription or start training.</p>
          <p role={expired ? "status" : undefined}>{expired ? "This code has expired. Start a new connection from your terminal." : `Approve before ${new Date(inspection.expires_at).toLocaleTimeString()}.`}</p>
          <div className="dialog-actions"><button disabled={busy || expired || !project} onClick={() => decide(true)}>Approve collector</button><button className="secondary" disabled={busy || expired} onClick={() => decide(false)}>Decline</button><button className="text-button" disabled={busy} onClick={() => setInspection(null)}>Use another code</button></div>
        </div>}
      </section>
      <section className="panel" aria-labelledby="device-list-title"><div className="panel-heading"><h2 id="device-list-title">Collector approvals</h2><button className="text-button" disabled={busy} onClick={() => void act(() => reload(controller.current.signal))}>Refresh collectors</button></div>
        <p className="muted">These approvals allow uploads; they do not prove a collector is online. Disconnect to stop its next upload. Captures already saved remain in the project.</p>
        {connections === null ? <p>Collector status is unavailable.</p> : connections.length === 0 ? <p>No active collector approvals for this email account.</p> : <ul className="device-list">{connections.map(g => <li key={g.id}><div><strong>{g.device_name}</strong><p>{g.client_name}</p><p>{projects.find(p => p.id === g.project_id)?.name || "Project access changed"}</p><p className="muted">Authorization ends {new Date(g.expires_at).toLocaleDateString()}.</p></div><button className="text-button danger" disabled={busy} onClick={() => setDisconnect(g)}>Disconnect<span className="sr-only"> {g.device_name}</span></button></li>)}</ul>}
      </section>
    </div>}
    {disconnect && <Dialog title="Disconnect this collector?" busy={busy} onClose={() => setDisconnect(null)}>{error && <p role="alert" className="error">{error}</p>}<p><strong>{disconnect.device_name}</strong> will need a new approval before it can upload again. Its local queue stays on that device.</p><div className="dialog-actions"><button disabled={busy} onClick={() => void act(async () => { await api(endpoint("disconnect"), controller.current.signal, "POST", { grant_id: disconnect.id }); setDisconnect(null); await reload(controller.current.signal); setNotice("Collector disconnected."); })}>Disconnect collector</button><button className="secondary" disabled={busy} onClick={() => setDisconnect(null)}>Keep connected</button></div></Dialog>}
  </main>;
}
