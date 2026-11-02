// Consumer validation: the Rust routes remain the authority. Do not treat unknown
// response shapes as empty organizations, zero activity or successful mutations.
export type Role = "viewer" | "member" | "admin" | "owner";
export type Project = { id: string; name: string; slug: string };
export type Organization = {
  id: string;
  name: string;
  role: Role;
  personal: boolean;
  projects: Project[];
};
export type Overview = {
  user: { id: string; name: string | null };
  default_project: string | null;
  organizations: Organization[];
};
export type ProjectKey = {
  id: string;
  label: string;
  prefix: string;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
};
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}
function object(v: unknown): Record<string, unknown> {
  if (!v || typeof v !== "object" || Array.isArray(v))
    throw new Error("The server returned an unreadable response.");
  return v as Record<string, unknown>;
}
function text(v: unknown): string {
  if (typeof v !== "string")
    throw new Error("The server returned an incomplete response.");
  return v;
}
function list(v: unknown): unknown[] {
  if (!Array.isArray(v))
    throw new Error("The server returned an incomplete list.");
  return v;
}
const optionalText = (v: unknown) => (v === null ? null : text(v));
export function overview(value: unknown): Overview {
  const v = object(value),
    u = object(v.user);
  return {
    user: { id: text(u.id), name: optionalText(u.name) },
    default_project: optionalText(v.default_project),
    organizations: list(v.organizations).map((raw) => {
      const o = object(raw),
        role = text(o.role);
      if (
        !["viewer", "member", "admin", "owner"].includes(role) ||
        typeof o.personal !== "boolean"
      )
        throw new Error("Your organization access could not be read.");
      return {
        id: text(o.id),
        name: text(o.name),
        role: role as Role,
        personal: o.personal,
        projects: list(o.projects).map((raw) => {
          const p = object(raw);
          return { id: text(p.id), name: text(p.name), slug: text(p.slug) };
        }),
      };
    }),
  };
}
export function keys(value: unknown): ProjectKey[] {
  return list(object(value).keys).map((raw) => {
    const k = object(raw);
    return {
      id: text(k.id),
      label: text(k.label),
      prefix: text(k.prefix),
      created_at: text(k.created_at),
      last_used_at: optionalText(k.last_used_at),
      revoked_at: optionalText(k.revoked_at),
    };
  });
}
export function createdKey(value: unknown): { key: string; id: string } {
  const v = object(value);
  return { key: text(v.key), id: text(v.id) };
}
export function createdId(value: unknown): string {
  return text(object(value).id);
}
export async function api(
  path: string,
  signal: AbortSignal,
  method = "GET",
  body?: unknown,
): Promise<unknown> {
  const r = await fetch(path, {
    method,
    signal,
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      "X-Rroflow-Request": "dashboard",
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const data: unknown = await r.json().catch(() => null);
  if (!r.ok) {
    const message =
      data &&
      typeof data === "object" &&
      "error" in data &&
      typeof data.error === "string"
        ? data.error
        : "Please try again.";
    throw new ApiError(
      r.status,
      r.status === 401 ? "Your session has ended. Sign in again." : message,
    );
  }
  return data;
}
export function monitorLink(project: string, view = "overview"): string {
  return "/app?workspace=" + encodeURIComponent(project) + "#" + view;
}
