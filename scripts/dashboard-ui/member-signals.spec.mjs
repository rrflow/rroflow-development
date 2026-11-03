import { test as base, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { startDatasetFixture } from "./dataset-service.mjs";
const test = base.extend({
  member: async ({}, use) => {
    test.skip(
      !process.env.RROFLOW_DATASET_UI_DATABASE_URL,
      "Requires disposable PostgreSQL",
    );
    const f = await startDatasetFixture(
      process.env.RROFLOW_DATASET_UI_DATABASE_URL,
    );
    try {
      await use(f);
    } finally {
      await f.close();
    }
  },
});
async function connect(page, f, token = f.token) {
  await page.setExtraHTTPHeaders({ Authorization: "Bearer " + token });
  await page.route("**/*", (r) =>
    new URL(r.request().url()).origin === f.base ? r.continue() : r.abort(),
  );
}
async function rpc(f, project, path, method = "GET", body, token = f.token) {
  const r = await fetch(f.base + path, {
    method,
    headers: {
      Authorization: "Bearer " + token,
      "Content-Type": "application/json",
      ...(project ? { "X-Rroflow-Project": project } : {}),
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  return { status: r.status, body: await r.json() };
}
async function home(f) {
  const r = await f.api("/v1/me");
  const project = r.body.default_project;
  return {
    project,
    org: r.body.organizations.find((o) =>
      o.projects.some((p) => p.id === project),
    ).id,
  };
}
const url = (f, o) =>
  f.base + "/app/manage#" + new URLSearchParams({ ...o, view: "signals" });
async function accessible(page) {
  expect(
    (
      await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
        .analyze()
    ).violations,
  ).toEqual([]);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
}
async function proof(page, info, name) {
  const path = info.outputPath(name + "-test-fixture.png");
  await page.screenshot({ path, fullPage: true });
  await info.attach(name + " (fixture activity)", {
    path,
    contentType: "image/png",
  });
}
async function draft(page, name = "Response check") {
  await page
    .getByRole("button", { name: "Create a signal", exact: true })
    .click();
  await page.getByLabel("Signal name", { exact: true }).fill(name);
  await page
    .getByRole("dialog")
    .getByLabel("Group", { exact: true })
    .fill("Release checks");
  await page
    .getByLabel("What should this notice?")
    .selectOption("response_contains");
  await page
    .getByLabel("Phrase to match")
    .fill("PRIVATE_DATASET_FIXTURE_RESPONSE");
}
const card = (page, name) =>
  page
    .locator("tr.signal-row")
    .filter({ has: page.getByRole("heading", { name, exact: true }) });

test("member previews, creates, finds evidence and pauses a signal while keeping its history", async ({
  page,
  member: f,
}, info) => {
  const h = await home(f),
    seed = (await f.api("/v1/turns")).body.turns[0];
  expect(
    (
      await f.api("/v1/turns", {
        turns: [
          {
            ...seed,
            event_id: "library-no-text",
            content: null,
            signals: [
              {
                signal: "historic_check",
                rule_version: 1,
                severity: "info",
                summary: "Historical fixture",
                score: null,
                evidence: [],
              },
            ],
          },
        ],
      })
    ).status,
  ).toBe(200);
  await connect(page, f);
  await page.goto(f.base + "/app/manage#" + new URLSearchParams(h));
  await page
    .getByRole("link", { name: /Explore signals and create your own/ })
    .click();
  await expect(
    page.getByRole("heading", { name: "Your signal library", exact: true }),
  ).toBeVisible();
  await expect(card(page, "historic check")).toContainText(
    "Definition unavailable",
  );
  await draft(page, "Deployment phrase");
  await expect(
    page.getByRole("button", { name: "Save and turn on", exact: true }),
  ).toBeDisabled();
  await page
    .getByText("Optional: limit the source activity", { exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Agent", exact: true })
    .selectOption("codex");
  await page
    .getByRole("combobox", { name: "Source project label", exact: true })
    .selectOption("Fixture project");
  await page
    .getByRole("button", { name: "Try on recent activity", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Preview results" }),
  ).toContainText("1 match in 1 checked turn. 1 could not be checked");
  await accessible(page);
  await proof(page, info, "member-signal-preview");
  await page
    .getByLabel("Signal name", { exact: true })
    .fill("Deployment phrase reviewed");
  await expect(
    page.getByRole("button", { name: "Save and turn on", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Try on recent activity", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Save and turn on", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Save and turn on", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  const name = "Deployment phrase reviewed",
    created = card(page, name);
  await expect(created).toContainText("0 recorded matches");
  const settings = (await rpc(f, h.project, "/v1/signals/settings")).body
      .settings,
    rule = settings.custom.find((r) => r.name === name);
  expect(rule).toBeTruthy();
  expect(
    (
      await f.api("/v1/turns", {
        turns: [{ ...seed, event_id: "library-match-arrival" }],
      })
    ).status,
  ).toBe(200);
  await page
    .getByRole("button", { name: "Refresh signals", exact: true })
    .click();
  await expect(created).toContainText("1 recorded match");
  await page
    .getByRole("combobox", { name: "Signal type", exact: true })
    .selectOption("custom");
  await page
    .getByRole("combobox", { name: "Group", exact: true })
    .selectOption("Release checks");
  await page.getByLabel("Find a signal").fill("Deployment");
  await expect(page.locator("tr.signal-row")).toHaveCount(1);
  await accessible(page);
  await proof(page, info, "member-signal-library");
  const href = await created
    .getByRole("link", { name: /Open signal dashboard/ })
    .getAttribute("href");
  expect(href).toContain("workspace=" + encodeURIComponent(h.project));
  expect(href).toContain("signal=" + encodeURIComponent(rule.id));
  await created.getByRole("link", { name: /Open signal dashboard/ }).click();
  await expect(page.locator("#signal-results")).toContainText(name);
  await page
    .locator("#signal-results")
    .getByRole("button", { name: /Inspect evidence/ })
    .click();
  await expect(page.locator("#pane-signals")).toContainText(
    "PRIVATE_DATASET_FIXTURE_RESPONSE",
  );
  await page.goto(url(f, h));
  await card(page, name)
    .getByRole("button", { name: "Pause " + name, exact: true })
    .click();
  await expect(card(page, name)).toContainText("Paused");
  expect(
    (
      await f.api("/v1/turns", {
        turns: [{ ...seed, event_id: "library-after-pause" }],
      })
    ).status,
  ).toBe(200);
  expect(
    (await rpc(f, h.project, "/v1/turns?signal=" + rule.id)).body.turns,
  ).toHaveLength(1);
  await page.reload();
  await expect(card(page, name)).toContainText("Paused");
  await page.emulateMedia({ colorScheme: "dark" });
  await accessible(page);
});

test("conflicts preserve another editors choices and built-in thresholds remain adjustable", async ({
  page,
  member: f,
}) => {
  const h = await home(f);
  await connect(page, f);
  await page.goto(url(f, h));
  await draft(page);
  await page
    .getByRole("button", { name: "Try on recent activity", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Save and turn on", exact: true }),
  ).toBeEnabled();
  const cfg = (await rpc(f, h.project, "/v1/signals/settings")).body.settings;
  expect(
    (
      await rpc(f, h.project, "/v1/signals/settings", "PUT", {
        ...cfg,
        disabled: ["false_done"],
      })
    ).status,
  ).toBe(200);
  await page
    .getByRole("button", { name: "Save and turn on", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toContainText(
    "Someone changed the signal settings",
  );
  expect(
    (await rpc(f, h.project, "/v1/signals/settings")).body.settings.custom,
  ).toHaveLength(0);
  await page
    .getByRole("button", { name: "Reload current settings", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Save and turn on", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Save and turn on", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    (await rpc(f, h.project, "/v1/signals/settings")).body.settings.disabled,
  ).toEqual(["false_done"]);
  await expect(card(page, "Possible false completion")).toContainText("Paused");
  await card(page, "Repeated tool failures")
    .getByRole("button", { name: "Adjust check", exact: true })
    .click();
  await page.getByRole("spinbutton").fill("5");
  await page.getByRole("button", { name: "Save choices", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    (await rpc(f, h.project, "/v1/signals/settings")).body.settings.options
      .tool_error_streak.min_streak,
  ).toBe(5);
  await card(page, "Repeated tool failures")
    .getByRole("button", { name: "Pause Repeated tool failures", exact: true })
    .click();
  await expect(card(page, "Repeated tool failures")).toContainText("Paused");
  await page.reload();
  await expect(card(page, "Repeated tool failures")).toContainText("Paused");
  await accessible(page);
});

test("late or foreign previews cannot authorize saving and expired sessions clear private signals", async ({
  page,
  member: f,
}) => {
  const h = await home(f);
  await connect(page, f);
  await page.goto(url(f, h));
  await draft(page);
  let release, entered;
  const held = new Promise((r) => (release = r)),
    seen = new Promise((r) => (entered = r));
  await page.route("**/v1/signals/preview", async (route) => {
    const response = await route.fetch();
    entered();
    await held;
    await route.fulfill({ response }).catch(() => {});
  });
  try {
    await page
      .getByRole("button", { name: "Try on recent activity", exact: true })
      .click();
    await seen;
    await page.getByLabel("Phrase to match").fill("A DIFFERENT PHRASE");
    release();
    await expect(
      page.getByRole("button", { name: "Try on recent activity", exact: true }),
    ).toBeEnabled();
    await expect(
      page.getByRole("region", { name: "Preview results" }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Save and turn on", exact: true }),
    ).toBeDisabled();
  } finally {
    release();
  }
  await page.unroute("**/v1/signals/preview");
  await page.route("**/v1/signals/preview", async (r) => {
    const response = await r.fetch(),
      body = await response.json();
    body.devspace = "foreign-project";
    await r.fulfill({ response, json: body });
  });
  await page
    .getByRole("button", { name: "Try on recent activity", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toContainText(
    "belongs to another project",
  );
  await expect(
    page.getByRole("button", { name: "Save and turn on", exact: true }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.route("**/v1/summary", (r) =>
    r.fulfill({ status: 401, json: { error: "Expired fixture session" } }),
  );
  await page
    .getByRole("button", { name: "Refresh signals", exact: true })
    .click();
  await expect(
    page.getByRole("heading", {
      name: "Your workspace starts here",
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Your signal library", exact: true }),
  ).toHaveCount(0);
  await accessible(page);
});

test("viewer can inspect signal evidence but cannot change checks", async ({
  page,
  member: f,
}) => {
  const org = (
      await rpc(f, null, "/v1/organizations", "POST", {
        name: "Signal review team",
      })
    ).body.id,
    project = (
      await rpc(f, null, `/v1/organizations/${org}/projects`, "POST", {
        name: "Shared signals",
      })
    ).body.id;
  const invite = await rpc(
    f,
    null,
    `/v1/organizations/${org}/invitations`,
    "POST",
    { role: "viewer" },
  );
  expect(invite.status).toBe(200);
  expect(
    (
      await rpc(
        f,
        null,
        "/v1/invitations/accept",
        "POST",
        { token: invite.body.token },
        f.otherToken,
      )
    ).status,
  ).toBe(200);
  await connect(page, f, f.otherToken);
  await page.goto(url(f, { org, project }));
  await expect(
    page.getByText(/You can explore signals and evidence/),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Create a signal", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /Pause |Adjust check/ }),
  ).toHaveCount(0);
  await expect(
    card(page, "Blocked handoff").getByRole("link", {
      name: /Open signal dashboard/,
    }),
  ).toBeVisible();
  const cfg = (await rpc(f, project, "/v1/signals/settings")).body.settings;
  expect(
    (
      await rpc(
        f,
        project,
        "/v1/signals/settings",
        "PUT",
        { ...cfg, disabled: ["mood"] },
        f.otherToken,
      )
    ).status,
  ).toBe(403);
  await accessible(page);
});
