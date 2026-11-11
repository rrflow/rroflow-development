import { useEffect, useRef, useState, type FormEvent } from "react";

type Preset = { value: number; label: string; recommended: boolean };
type Setting = {
  key: string;
  label: string;
  help: string;
  min: number;
  max: number;
  default: number;
  value: number;
  presets: Preset[];
};
type Group = { id: string; label: string; settings: Setting[] };
type Loaded = { revision: number; groups: Group[] };

const headers = {
  "Content-Type": "application/json",
  "X-Rroflow-Request": "dashboard",
};

function loaded(value: unknown): Loaded {
  const v = value as Loaded | null;
  if (!v || typeof v.revision !== "number" || !Array.isArray(v.groups))
    throw new Error("The server returned an unreadable response.");
  return v;
}

function refusal(status: number, message?: unknown) {
  if (status === 401) return "Sign in to change settings.";
  if (status === 403) return "Only an instance admin can change settings.";
  if (status === 409)
    return "Someone else changed these settings. Reload to see the latest.";
  return typeof message === "string" ? message : "Please try again.";
}

async function send(
  signal: AbortSignal,
  method: "GET" | "PUT",
  body?: unknown,
): Promise<Loaded> {
  const response = await fetch("/v1/instance/settings", {
    method,
    credentials: "same-origin",
    signal,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok)
    throw new Error(
      refusal(response.status, (data as { error?: unknown } | null)?.error),
    );
  return loaded(data);
}

/// Built entirely from what the server describes, so a new setting needs no change here.
export function SettingsPage({ name }: { name: string }) {
  const [data, setData] = useState<Loaded | null>(null);
  const [values, setValues] = useState<Record<string, number>>({});
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const controller = useRef(new AbortController());

  function accept(next: Loaded) {
    setData(next);
    setValues(
      Object.fromEntries(
        next.groups.flatMap((g) => g.settings.map((s) => [s.key, s.value])),
      ),
    );
  }

  useEffect(() => {
    const c = new AbortController();
    controller.current = c;
    send(c.signal, "GET")
      .then(accept)
      .catch((e: unknown) => {
        if (!c.signal.aborted)
          setError(e instanceof Error ? e.message : "Please try again.");
      });
    return () => c.abort();
  }, []);

  async function save(e: FormEvent) {
    e.preventDefault();
    if (!data || busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      accept(
        await send(controller.current.signal, "PUT", {
          revision: data.revision,
          values,
        }),
      );
      setNotice("Settings saved.");
    } catch (failure) {
      if (!controller.current.signal.aborted)
        setError(failure instanceof Error ? failure.message : "Please try again.");
    } finally {
      if (!controller.current.signal.aborted) setBusy(false);
    }
  }

  return (
    <main className="device-layout">
      <header className="device-heading">
        <a href="/app/manage" className="wordmark">
          {name}
        </a>
        <a href="/app/manage">Back to workspace</a>
      </header>
      <section className="device-intro">
        <div>
          <p className="eyebrow">Instance</p>
          <h1>Settings</h1>
        </div>
      </section>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="muted" role="status">
          {notice}
        </p>
      )}
      {data && (
        <form onSubmit={save}>
          {data.groups.map((group) => (
            <section className="panel" key={group.id}>
              <h2>{group.label}</h2>
              {group.settings.map((setting) => {
                const value = values[setting.key] ?? setting.value;
                return (
                  <div className="settings-field" key={setting.key}>
                    <label htmlFor={setting.key}>{setting.label}</label>
                    <p className="muted">{setting.help}</p>
                    <div className="settings-presets">
                      {setting.presets.map((preset) => (
                        <button
                          type="button"
                          key={preset.value}
                          className={
                            value === preset.value ? undefined : "secondary"
                          }
                          aria-pressed={value === preset.value}
                          disabled={busy}
                          onClick={() =>
                            setValues({
                              ...values,
                              [setting.key]: preset.value,
                            })
                          }
                        >
                          {preset.label}
                          {preset.recommended ? " (recommended)" : ""}
                        </button>
                      ))}
                    </div>
                    <input
                      id={setting.key}
                      type="number"
                      min={setting.min}
                      max={setting.max}
                      value={value}
                      disabled={busy}
                      required
                      onChange={(e) =>
                        setValues({
                          ...values,
                          [setting.key]: Number(e.target.value),
                        })
                      }
                    />
                  </div>
                );
              })}
            </section>
          ))}
          <button type="submit" disabled={busy}>
            {busy ? "Saving…" : "Save changes"}
          </button>
        </form>
      )}
    </main>
  );
}
