import { test as base, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { randomUUID } from "node:crypto";
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
async function connect(page, f) {
  await page.setExtraHTTPHeaders({ Authorization: "Bearer " + f.token });
  await page.route("**/*", (r) =>
    new URL(r.request().url()).origin === f.base ? r.continue() : r.abort(),
  );
}
const pageUrl = (f, org, project) =>
  f.base + "/app/manage#" + new URLSearchParams({ org, project });
async function membership(f) {
  const r = await f.api("/v1/me");
  expect(r.status).toBe(200);
  const project = r.body.default_project;
  return {
    project,
    org: r.body.organizations.find((o) =>
      o.projects.some((p) => p.id === project),
    ).id,
  };
}
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

test("project overview shows received activity, signal evidence and delivery without reading content", async ({
  page,
  member: f,
}, info) => {
  const { org, project } = await membership(f);
  const seed = (await f.api("/v1/turns")).body.turns[0];
  expect(
    (
      await f.api("/v1/turns", {
        turns: [
          {
            ...seed,
            event_id: "overview-claude",
            agent: "claude-code",
            tools: [0, 1, 2].map((n) => ({
              id: `fixture-tool-${n}`,
              name: "fixture.command",
              started_at: seed.started_at,
              ended_at: seed.ended_at,
              status: "error",
              input: null,
              output: null,
            })),
            signals: [
              {
                signal: "tool_error_streak",
                rule_version: 1,
                severity: "warn",
                summary: "Fixture repeated failures",
                score: null,
                evidence: [],
              },
            ],
          },
        ],
      })
    ).status,
  ).toBe(200);
  const capture = randomUUID(),
    events = ["call.started", "call.failed"].map((kind, sequence) => ({
      schema: "rroflow.capture/v1",
      event_id: randomUUID(),
      capture_id: capture,
      sequence,
      agent: "grok",
      session_id: "overview-sdk-fixture",
      operation_id: "call",
      observed_at: "2026-10-30T12:00:00Z",
      kind,
      payload: { input: "PRIVATE_OVERVIEW_SDK_CONTENT" },
    }));
  expect((await f.api("/v1/events", { events })).status).toBe(200);
  await expect
    .poll(async () => (await f.api("/v1/sdk/summary")).body.activities)
    .toBe(1);
  expect(
    (
      await f.api("/v1/sources/health", {
        schema: "rroflow.source-health/v1",
        source_id: randomUUID(),
        sequence: 1,
        observed_at: new Date().toISOString(),
        collector_version: "fixture",
        pending_revisions: 2,
        pending_bytes: 200,
        journal_bytes: 400,
        oldest_pending_at: "2026-10-30T12:00:00Z",
        capture_errors: 0,
        delivery_ok: false,
        signal_settings_revision: 1,
        session_capture: true,
        sdk: {
          linked_queues: 1,
          pending_events: 3,
          pending_bytes: 100,
          retained_bytes: 200,
          retrying_queues: 1,
          queue_errors: 0,
        },
      })
    ).status,
  ).toBe(200);
  const requested = [];
  page.on("request", (r) => {
    if (new URL(r.url()).pathname.startsWith("/v1/"))
      requested.push(new URL(r.url()).pathname);
  });
  await connect(page, f);
  await page.goto(pageUrl(f, org, project));
  await expect(page.getByTestId("overview-turn-count")).toHaveText("2");
  await expect(page.getByTestId("overview-sdk-count")).toHaveText("1");
  await expect(page.getByTestId("overview-source-count")).toHaveText("1");
  await expect(
    page.getByText("Reported a problem", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("2 turn revisions waiting. 3 SDK events waiting.", {
      exact: true,
    }),
  ).toBeVisible();
  expect(requested).not.toContain("/v1/turns");
  expect(requested).not.toContain("/v1/sdk/activity");
  expect(requested).not.toContain("/v1/events");
  expect(await page.locator("body").innerText()).not.toContain(
    "PRIVATE_DATASET_FIXTURE_PROMPT",
  );
  expect(await page.locator("body").innerText()).not.toContain(
    "PRIVATE_OVERVIEW_SDK_CONTENT",
  );
  await accessible(page);
  const proof = info.outputPath("project-overview-test-fixture.png");
  await page.screenshot({ path: proof, fullPage: true });
  await info.attach("Project overview (fixture captures)", {
    path: proof,
    contentType: "image/png",
  });
  await page.getByText("What do these numbers mean?", { exact: true }).click();
  await expect(page.getByText(/same work can appear in both/)).toBeVisible();
  const finding = page.getByRole("link", {
    name: /Repeated tool failures.*1 match/,
  });
  expect(await finding.getAttribute("href")).toContain(
    "workspace=" + encodeURIComponent(project),
  );
  await finding.click();
  await expect(page).toHaveURL(
    new RegExp("workspace=" + project + ".*signal=tool_error_streak"),
  );
  await page.goto(pageUrl(f, org, project));
  await expect(page.getByTestId("overview-sdk-count")).toHaveText("1");
  const sdk = page.getByRole("link", { name: /Call failed.*1 activity/ });
  expect(await sdk.getAttribute("href")).toContain("kind=sdk");
  await page.emulateMedia({ colorScheme: "dark" });
  await accessible(page);
});

test("changing projects discards late metrics and expired sessions clear the overview", async ({
  page,
  member: f,
}) => {
  const { org, project } = await membership(f);
  const created = await f.api(
    `/v1/organizations/${encodeURIComponent(org)}/projects`,
    { name: "Empty overview project" },
  );
  expect(created.status).toBe(200);
  await connect(page, f);
  let release, entered, delivered;
  const hold = new Promise((r) => (release = r)),
    seen = new Promise((r) => (entered = r)),
    finished = new Promise((r) => (delivered = r));
  await page.route("**/v1/summary", async (route) => {
    try {
      const response = await route.fetch();
      if (route.request().headers()["x-rroflow-project"] === project) {
        entered();
        await hold;
        await route.fulfill({ response }).catch(() => {});
        delivered();
      } else await route.fulfill({ response });
    } catch {
      await route.abort().catch(() => {});
    }
  });
  try {
    await page.goto(pageUrl(f, org, project));
    await seen;
    await expect(page.getByTestId("overview-turn-count")).toHaveText(
      "Loading...",
    );
    await page.getByRole("link", { name: /All projects/ }).click();
    await page
      .getByRole("link", { name: "Empty overview project", exact: true })
      .click();
    await expect(page.getByTestId("overview-turn-count")).toHaveText("0");
    await expect(page.getByTestId("overview-sdk-count")).toHaveText("0");
    release();
    await finished;
    await expect(page.getByTestId("overview-turn-count")).toHaveText("0");
    await expect(
      page.getByText(/Connect a source to start seeing turns here/),
    ).toBeVisible();
    await page.unroute("**/v1/summary");
    await page.route("**/v1/summary", (r) =>
      r.fulfill({ status: 401, json: { error: "Fixture session expired" } }),
    );
    await page
      .getByRole("button", { name: "Refresh overview", exact: true })
      .click();
    await expect(
      page.getByRole("heading", {
        name: "Your workspace starts here",
        exact: true,
      }),
    ).toBeVisible();
    await expect(page.getByTestId("overview-turn-count")).toHaveCount(0);
    await accessible(page);
  } finally {
    release();
  }
});

test("overview does not replace unavailable, stale or foreign responses with healthy zeroes", async ({
  page,
  member: f,
}) => {
  const { org, project } = await membership(f);
  await connect(page, f);
  let wrong = true;
  await page.route("**/v1/summary", async (r) => {
    const response = await r.fetch();
    const body = await response.json();
    if (wrong) body.devspace = "another-project";
    await r.fulfill({ response, json: body });
  });
  await page.route("**/v1/sdk/summary", (r) =>
    r.fulfill({ status: 503, json: { error: "Fixture indexing unavailable" } }),
  );
  await page.goto(pageUrl(f, org, project));
  await expect(page.getByTestId("overview-turn-count")).toHaveText(
    "Unavailable",
  );
  await expect(
    page.getByText(/Monitoring returned a different project/),
  ).toBeVisible();
  await expect(page.getByTestId("overview-sdk-count")).toHaveText(
    "Unavailable",
  );
  await expect(page.getByText("Fixture indexing unavailable")).toBeVisible();
  wrong = false;
  await page
    .getByRole("button", { name: "Refresh overview", exact: true })
    .click();
  await expect(page.getByTestId("overview-turn-count")).toHaveText("1");
  await page.unroute("**/v1/sdk/summary");
  await page.route("**/v1/sources/health", (r) =>
    r.fulfill({
      json: {
        devspace: project,
        as_of: "2026-10-30T12:00:00Z",
        recent_for_seconds: 1,
        sources: [
          {
            received_at: "2026-10-30T11:00:00Z",
            report: {
              source_id: "00000000-0000-4000-8000-000000000001",
              pending_revisions: 4,
              capture_errors: 0,
              delivery_ok: true,
            },
          },
        ],
      },
    }),
  );
  await page
    .getByRole("button", { name: "Refresh overview", exact: true })
    .click();
  await expect(
    page.getByText("No recent report loaded", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText(/SDK queue counts were not reported/),
  ).toBeVisible();
  await accessible(page);
});
