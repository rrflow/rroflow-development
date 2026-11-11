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
  first_run: boolean;
  builtin_admin: boolean;
  password_policy: PasswordPolicy;
};
type PasswordPolicy = {
  min: number;
  max: number;
  recommended_min: number;
  recommended_max: number;
};
type Mode =
  | "login"
  | "register"
  | "reset"
  | "complete"
  | "first-run"
  | "unsaved";
type NewPassword = {
  signin: string;
  password: string;
  revision: number;
  active_keys?: number;
};

const postHeaders = {
  "Content-Type": "application/json",
  "X-Rroflow-Request": "dashboard",
};

function passwordRule(policy?: PasswordPolicy) {
  if (!policy) return "";
  return `Use at least ${policy.min} characters. ${policy.recommended_min}–${policy.recommended_max} is recommended.`;
}

function SavePassword({
  created,
  onSaved,
}: {
  created: NewPassword;
  onSaved: (revision: number) => Promise<void>;
}) {
  const [confirmed, setConfirmed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function copy() {
    try {
      await navigator.clipboard.writeText(created.password);
      setCopied(true);
    } catch {
      setError("Copy did not work here. Select the password and copy it.");
    }
  }
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!confirmed || busy) return;
    setBusy(true);
    setError("");
    try {
      await onSaved(created.revision);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Please try again.");
      setBusy(false);
    }
  }
  return (
    <form onSubmit={submit}>
      <p className="account-warning">
        This is the only time it will be shown. Keep it in a password manager,
        and do not paste this one into an AI chat.
      </p>
      {Boolean(created.active_keys) && (
        <p role="status">
          {created.active_keys} connection{" "}
          {created.active_keys === 1 ? "key" : "keys"} from before this reset
          still {created.active_keys === 1 ? "works" : "work"}. If you did not
          make {created.active_keys === 1 ? "it" : "them"}, revoke{" "}
          {created.active_keys === 1 ? "it" : "them"} in your workspace.
        </p>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <label htmlFor="new-signin">Sign-in name</label>
      <input
        id="new-signin"
        name="username"
        autoComplete="username"
        readOnly
        value={created.signin}
      />
      <label htmlFor="new-password">New password</label>
      <input
        id="new-password"
        name="password"
        type="text"
        className="account-secret"
        autoComplete="new-password"
        spellCheck={false}
        readOnly
        value={created.password}
      />
      <button type="button" className="secondary" onClick={copy}>
        {copied ? "Copied" : "Copy password"}
      </button>
      <label className="account-check">
        <input
          type="checkbox"
          checked={confirmed}
          onChange={(e) => setConfirmed(e.target.checked)}
        />
        I saved this password
      </label>
      <button type="submit" disabled={!confirmed || busy}>
        {busy ? "Working…" : "Go to my workspace"}
      </button>
    </form>
  );
}

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
  const [chosen, setChosen] = useState("");
  const [generate, setGenerate] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [created, setCreated] = useState<NewPassword | null>(null);
  const [unsaved, setUnsaved] = useState<{
    signin: string;
    revision: number;
  } | null>(null);
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
            "first_run",
            "builtin_admin",
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
        if ((data as Options).first_run && !initialToken) setMode("first-run");
      })
      .catch((e) => {
        if (!c.signal.aborted)
          setError(
            e instanceof Error ? e.message : "Please reload and try again.",
          );
      });
    return () => c.abort();
  }, [optionsAttempt]);
  useEffect(() => {
    // Only the built-in admin is ever given a generated password to save. A live setup
    // password comes first, or a reset would be hidden behind this older question.
    if (!options?.builtin_admin || options.first_run) return;
    const c = new AbortController();
    // A page closed before saving can still get a new password while signed in.
    fetch("/auth/native/password/status", {
      method: "POST",
      credentials: "same-origin",
      signal: c.signal,
      headers: postHeaders,
    })
      .then((r) => (r.ok ? r.json() : null))
      .then((status) => {
        if (status?.unsaved === true && typeof status.signin === "string") {
          setUnsaved({ signin: status.signin, revision: status.revision });
          setMode("unsaved");
        }
      })
      .catch(() => {});
    return () => c.abort();
  }, [options?.builtin_admin, options?.first_run]);
  function enterWorkspace() {
    location.assign(deviceReturn ? "/app/device" : "/app/manage");
  }
  async function markSaved(revision: number) {
    const response = await fetch("/auth/native/password/saved", {
      method: "POST",
      credentials: "same-origin",
      headers: postHeaders,
      body: JSON.stringify({ revision }),
    });
    if (response.status === 409)
      throw new Error(
        "A newer password was made in another tab. Reload this page.",
      );
    if (!response.ok) throw new Error("That did not save. Please try again.");
    enterWorkspace();
  }
  async function makeNewPassword() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const response = await fetch("/auth/native/password/replace", {
        method: "POST",
        credentials: "same-origin",
        signal: controller.current.signal,
        headers: postHeaders,
      });
      const data: unknown = await response.json().catch(() => null);
      if (!response.ok) throw new Error("Please sign in and try again.");
      setCreated(data as NewPassword);
    } catch (e) {
      if (!controller.current.signal.aborted)
        setError(e instanceof Error ? e.message : "Please try again.");
    } finally {
      pending.current = false;
      if (!controller.current.signal.aborted) setBusy(false);
    }
  }
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
          : mode === "first-run"
            ? "first-run"
            : "login";
    const body =
      endpoint === "request"
        ? { email, purpose: mode }
        : endpoint === "complete"
          ? { token, password }
          : endpoint === "first-run"
            ? generate
              ? { password }
              : { password, new_password: chosen }
            : { email, password };
    try {
      const response = await fetch(`/auth/native/${endpoint}`, {
        method: "POST",
        credentials: "same-origin",
        signal: controller.current.signal,
        headers: postHeaders,
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
        enterWorkspace();
        return;
      }
      if (mode === "first-run") {
        setPassword("");
        setChosen("");
        setShowPassword(false);
        if (!(data as NewPassword).password) {
          enterWorkspace();
          return;
        }
        setCreated(data as NewPassword);
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
  const title = created
    ? "Save your new password"
    : mode === "first-run"
      ? "Finish setting up"
      : mode === "unsaved"
        ? "You still need to save your password"
        : mode === "register"
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
        {created && <SavePassword created={created} onSaved={markSaved} />}
        {!created && mode === "unsaved" && (
          <>
            <p className="muted">
              You are signed in as {unsaved?.signin}. A password can only be
              shown once. If you did not save it, make a new one, which stops
              the old one from working.
            </p>
            <button disabled={busy} onClick={makeNewPassword}>
              {busy ? "Working…" : "Make a new password"}
            </button>
            <button
              className="secondary"
              disabled={busy}
              onClick={() => {
                setError("");
                markSaved(unsaved?.revision ?? 0).catch((e) =>
                  setError(
                    e instanceof Error ? e.message : "Please try again.",
                  ),
                );
              }}
            >
              I already saved it
            </button>
          </>
        )}
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
        {options?.enabled && !created && mode !== "unsaved" && (
          <>
            <p className="muted">
              {mode === "first-run"
                ? "Paste the setup password from your install, then choose your own password or have one made for you."
                : mode === "register"
                ? "Start with your email. We will send a link to verify it before you choose a password."
                : mode === "reset"
                  ? "Enter your account email. We will send a recovery link if it is eligible."
                  : mode === "complete"
                    ? passwordRule(options?.password_policy)
                    : "Sign in to see your organizations and projects."}
            </p>
            <form onSubmit={submit}>
              {mode !== "complete" && mode !== "first-run" && (
                <>
                  <label htmlFor="account-email">
                    {mode === "login" && options.builtin_admin
                      ? "Email or admin name"
                      : "Email address"}
                  </label>
                  <input
                    id="account-email"
                    name="email"
                    type={
                      mode === "login" && options.builtin_admin
                        ? "text"
                        : "email"
                    }
                    autoComplete="username"
                    value={email}
                    maxLength={254}
                    onChange={(e) => setEmail(e.target.value)}
                    required
                    disabled={busy}
                  />
                </>
              )}
              {(mode === "login" ||
                mode === "complete" ||
                mode === "first-run") && (
                <>
                  <label htmlFor="account-password">
                    {mode === "first-run" ? "Setup password" : "Password"}
                  </label>
                  <input
                    id="account-password"
                    name="password"
                    type={showPassword ? "text" : "password"}
                    autoComplete={
                      mode === "login"
                        ? "current-password"
                        : mode === "first-run"
                          ? "off"
                          : "new-password"
                    }
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    minLength={
                      mode === "complete"
                        ? options?.password_policy.min
                        : undefined
                    }
                    maxLength={options?.password_policy.max}
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
                    {mode === "first-run"
                      ? "It starts with rrsetup- and is safe to paste from your AI chat."
                      : mode === "complete"
                        ? passwordRule(options?.password_policy)
                        : "Spaces and password managers are welcome."}
                  </p>
                </>
              )}
              {mode === "first-run" &&
                (generate ? (
                  <p className="muted">
                    We will make a strong password for you to save.{" "}
                    <button
                      type="button"
                      className="text-button"
                      disabled={busy}
                      onClick={() => setGenerate(false)}
                    >
                      Choose my own instead
                    </button>
                  </p>
                ) : (
                  <>
                    <label htmlFor="account-chosen">Choose a password</label>
                    <input
                      id="account-chosen"
                      name="new-password"
                      type={showPassword ? "text" : "password"}
                      autoComplete="new-password"
                      value={chosen}
                      onChange={(e) => setChosen(e.target.value)}
                      minLength={options?.password_policy.min}
                      maxLength={options?.password_policy.max}
                      required
                      disabled={busy}
                      aria-describedby="chosen-help"
                    />
                    <p id="chosen-help" className="muted">
                      {passwordRule(options?.password_policy)}
                    </p>
                    <button
                      type="button"
                      className="text-button"
                      disabled={busy}
                      onClick={() => setGenerate(true)}
                    >
                      Generate one for me instead
                    </button>
                  </>
                ))}
              <button
                type="submit"
                className={federationAvailable ? "secondary" : undefined}
                disabled={busy}
              >
                {busy
                  ? "Working…"
                  : mode === "first-run"
                    ? "Continue"
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
