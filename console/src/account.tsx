import { useEffect, useRef, useState, type FormEvent } from "react";
import { FlowArt, Icon } from "./ui";

// Read once before React's development replay, then remove from the visible URL.
let initialToken = /^#rrnv_[a-f0-9]{64}$/.test(location.hash)
  ? location.hash.slice(1)
  : "";
if (initialToken) history.replaceState(null, "", location.pathname);
type Options = {
  enabled: boolean;
  signup_enabled: boolean;
  recovery_enabled: boolean;
  federation_enabled: boolean;
  federation_label: string;
};
type Mode = "login" | "register" | "reset" | "complete";

export function AccountPage({ name }: { name: string }) {
  // A closed destination choice, never an arbitrary redirect URL.
  const deviceReturn = new URLSearchParams(location.search).get("next") === "device";
  const [options, setOptions] = useState<Options | null>(null);
  const [optionsAttempt, setOptionsAttempt] = useState(0);
  const [mode, setMode] = useState<Mode>(initialToken ? "complete" : "login");
  const [token, setToken] = useState(initialToken);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const controller = useRef(new AbortController());
  useEffect(() => {
    function openLink() {
      if (!/^#rrnv_[a-f0-9]{64}$/.test(location.hash)) return;
      setToken(location.hash.slice(1));
      setMode("complete");
      setPassword("");
      setShowPassword(false);
      setError("");
      setNotice("");
      history.replaceState(null, "", location.pathname);
    }
    window.addEventListener("hashchange", openLink);
    return () => window.removeEventListener("hashchange", openLink);
  }, []);
  useEffect(() => {
    const c = new AbortController();
    controller.current = c;
    setError("");
    fetch("/auth/native/config", {
      credentials: "same-origin",
      signal: c.signal,
    })
      .then(async (r) => {
        if (!r.ok)
          throw new Error(
            "Account options could not be loaded. Reload to try again.",
          );
        const data: unknown = await r.json();
        if (
          !data ||
          typeof data !== "object" ||
          ![
            "enabled",
            "signup_enabled",
            "recovery_enabled",
            "federation_enabled",
          ].every(
            (k) =>
              k in data &&
              typeof (data as Record<string, unknown>)[k] === "boolean",
          ) ||
          !("federation_label" in data) ||
          typeof data.federation_label !== "string" ||
          !data.federation_label.trim() ||
          [...data.federation_label].length > 80
        )
          throw new Error(
            "Account options could not be read. Reload to try again.",
          );
        setOptions(data as Options);
      })
      .catch((e) => {
        if (!c.signal.aborted)
          setError(
            e instanceof Error ? e.message : "Please reload and try again.",
          );
      });
    return () => c.abort();
  }, [optionsAttempt]);
  function change(next: Mode) {
    setMode(next);
    setPassword("");
    setShowPassword(false);
    setNotice("");
    setError("");
  }
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    const endpoint =
      mode === "register" || mode === "reset"
        ? "request"
        : mode === "complete"
          ? "complete"
          : "login";
    const body =
      endpoint === "request"
        ? { email, purpose: mode }
        : endpoint === "complete"
          ? { token, password }
          : { email, password };
    try {
      const response = await fetch(`/auth/native/${endpoint}`, {
        method: "POST",
        credentials: "same-origin",
        signal: controller.current.signal,
        headers: {
          "Content-Type": "application/json",
          "X-Rroflow-Request": "dashboard",
        },
        body: JSON.stringify(body),
      });
      const data: unknown = await response.json().catch(() => null);
      const message =
        data &&
        typeof data === "object" &&
        "message" in data &&
        typeof data.message === "string"
          ? data.message
          : "Please try again.";
      if (!response.ok) throw new Error(message);
      if (mode === "login") {
        location.assign(deviceReturn ? "/app/device" : "/app/manage");
        return;
      }
      setNotice(message);
      setPassword("");
      setShowPassword(false);
      if (mode === "complete") {
        initialToken = "";
        setToken("");
        setMode("login");
      }
    } catch (e) {
      if (!controller.current.signal.aborted)
        setError(e instanceof Error ? e.message : "Please try again.");
    } finally {
      pending.current = false;
      if (!controller.current.signal.aborted) setBusy(false);
    }
  }
  const title =
    mode === "register"
      ? "Your AI work, in one place"
      : mode === "reset"
        ? "Let's get you back in"
        : mode === "complete"
          ? "Choose your password"
          : "Welcome back";
  const federationAvailable =
    options?.federation_enabled && !deviceReturn && mode === "login";
  return (
    <main className="account-layout">
      <section className="account-story" aria-label="About your account">
        <a href="/" className="wordmark">
          {name}
        </a>
        <FlowArt />
        <h2>A clear home for your AI work.</h2>
        <p>
          Connect your tools, follow their activity, and open the evidence
          behind each signal.
        </p>
        <p className="muted">
          Your projects stay private to the people you invite.
        </p>
      </section>
      <section className="account-form" aria-labelledby="account-title">
        <p className="eyebrow">Your account</p>
        <h1 id="account-title">{title}</h1>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        {notice && <p role="status">{notice}</p>}
        {!options && !error && <p role="status">Loading sign-in options…</p>}
        {!options && error && (
          <button
            className="secondary"
            onClick={() => setOptionsAttempt((attempt) => attempt + 1)}
          >
            Reload sign-in options
          </button>
        )}
        {federationAvailable && (
          <div className="account-federation">
            <a className="button" href="/auth/federation">
              <Icon name="key" /> Continue with {options.federation_label}
            </a>
            <p className="muted">
              Use your shared {options.federation_label} sign-in.
            </p>
            {options.enabled && (
              <p className="account-divider">Or use your {name} password</p>
            )}
          </div>
        )}
        {options?.enabled && (
          <>
            <p className="muted">
              {mode === "register"
                ? "Start with your email. We will send a link to verify it before you choose a password."
                : mode === "reset"
                  ? "Enter your account email. We will send a recovery link if it is eligible."
                  : mode === "complete"
                    ? "Use at least 15 characters. A long phrase works well."
                    : "Sign in to see your organizations and projects."}
            </p>
            <form onSubmit={submit}>
              {mode !== "complete" && (
                <>
                  <label htmlFor="account-email">Email address</label>
                  <input
                    id="account-email"
                    name="email"
                    type="email"
                    autoComplete="username"
                    value={email}
                    maxLength={254}
                    onChange={(e) => setEmail(e.target.value)}
                    required
                    disabled={busy}
                  />
                </>
              )}
              {(mode === "login" || mode === "complete") && (
                <>
                  <label htmlFor="account-password">Password</label>
                  <input
                    id="account-password"
                    name="password"
                    type={showPassword ? "text" : "password"}
                    autoComplete={
                      mode === "login" ? "current-password" : "new-password"
                    }
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    minLength={15}
                    maxLength={256}
                    required
                    disabled={busy}
                    aria-describedby="password-help"
                  />
                  <button
                    type="button"
                    className="text-button account-password-toggle"
                    aria-controls="account-password"
                    aria-pressed={showPassword}
                    disabled={busy}
                    onClick={() => setShowPassword(!showPassword)}
                  >
                    {showPassword ? "Hide password" : "Show password"}
                  </button>
                  <p id="password-help" className="muted">
                    15–128 characters. Spaces and password managers are welcome.
                  </p>
                </>
              )}
              <button
                type="submit"
                className={federationAvailable ? "secondary" : undefined}
                disabled={busy}
              >
                {busy
                  ? "Working…"
                  : mode === "login"
                    ? "Sign in"
                    : mode === "complete"
                      ? "Save password"
                      : "Send email"}
              </button>
            </form>
            <div className="account-options">
              {mode !== "login" && (
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => change("login")}
                >
                  Back to sign in
                </button>
              )}
              {mode === "login" && options.recovery_enabled && (
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => change("reset")}
                >
                  Forgot password?
                </button>
              )}
              {mode === "login" && options.signup_enabled && (
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() => change("register")}
                >
                  Create an account
                </button>
              )}
            </div>
          </>
        )}
        {options && !options.enabled && !federationAvailable && (
          <p role="status">
            {deviceReturn
              ? "This connection needs password sign-in, which is not available here."
              : "Sign-in is not available right now. Please try again later."}
          </p>
        )}
        <p className="muted">
          <a href="/">Back to home</a>
        </p>
      </section>
    </main>
  );
}
