import { test as base, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { startDatasetFixture } from "./dataset-service.mjs";
const database = process.env.RROFLOW_DATASET_UI_DATABASE_URL;
const test = base.extend({
  member: async ({}, use) => {
    test.skip(!database, "Requires disposable PostgreSQL");
    const member = await startDatasetFixture(database);
    try {
      await use(member);
    } finally {
      await member.close();
    }
  },
});
async function connect(page, member, token = member.token) {
  await page.setExtraHTTPHeaders({ Authorization: "Bearer " + token });
  await page.route("**/*", (r) =>
    new URL(r.request().url()).origin === member.base
      ? r.continue()
      : r.abort(),
  );
}
async function rpc(member, path, method = "GET", body, token = member.token) {
  const r = await fetch(member.base + path, {
    method,
    headers: {
      Authorization: "Bearer " + token,
      "Content-Type": "application/json",
    },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  return { status: r.status, body: await r.json() };
}
async function newTeam(member, name) {
  const org = await rpc(member, "/v1/organizations", "POST", { name });
  expect(org.status).toBe(200);
  const p = await rpc(
    member,
    `/v1/organizations/${org.body.id}/projects`,
    "POST",
    { name: name + " project" },
  );
  expect(p.status).toBe(200);
  return { org: org.body.id, project: p.body.id };
}
async function accessible(page) {
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
    .analyze();
  expect(
    result.violations.map((v) => ({
      id: v.id,
      nodes: v.nodes.map((n) => ({
        target: n.target,
        summary: n.failureSummary,
      })),
    })),
  ).toEqual([]);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth + 1,
    ),
  ).toBe(true);
}
async function proof(page, info, name) {
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  const path = info.outputPath(name + "-test-fixture.png");
  await page.screenshot({ path, fullPage: true });
  await info.attach(name + " (disposable test membership)", {
    path,
    contentType: "image/png",
  });
}

test("member creates an organization, project and private key, then opens only that project in monitoring", async ({
  page,
  member,
}, info) => {
  await connect(page, member);
  await page.goto(member.base + "/app/manage");
  const response = await page.request.get(member.base + "/app/manage");
  expect(response.headers()["content-security-policy"]).toContain(
    "script-src 'self'",
  );
  expect(response.headers()["cache-control"]).toBe("no-store");
  await expect(
    page.getByRole("heading", { name: "Your work, organized." }),
  ).toBeVisible();
  await accessible(page);
  await page
    .getByRole("button", { name: "New organization", exact: true })
    .click();
  await page
    .getByLabel("Organization name", { exact: true })
    .fill("Studio North");
  await page
    .getByRole("button", { name: "Create organization", exact: true })
    .click();
  await expect(page.getByLabel("Organization", { exact: true })).toHaveValue(
    /o_/,
  );
  await expect(
    page.getByRole("heading", { name: "A fresh space for your next project" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "New project", exact: true }).click();
  await page.getByLabel("Project name", { exact: true }).fill("Client website");
  await page
    .getByRole("button", { name: "Create project", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Client website", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "No connection keys yet" }),
  ).toBeVisible();
  await page.getByRole("link", { name: "All projects" }).click();
  const organizationUrl = page.url();
  await page.getByRole("link", { name: "Skip to content" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("main")).toBeFocused();
  expect(page.url()).toBe(organizationUrl);
  await proof(page, info, "member-organization");
  await accessible(page);
  await page
    .getByRole("link", { name: "Manage connections", exact: true })
    .click();
  await page.getByRole("button", { name: "Create key", exact: true }).click();
  await page.getByLabel("A name you will recognize").fill("Laptop collector");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Create key", exact: true })
    .click();
  const secret = await page.getByLabel("One-time connection key").inputValue();
  expect(secret).toMatch(/^rrop_[a-f0-9]{64}$/);
  await page.getByRole("button", { name: "I've saved this key" }).click();
  await expect(page.getByLabel("One-time connection key")).toHaveCount(0);
  expect(
    await page.evaluate(
      (key) =>
        JSON.stringify({ ...sessionStorage, ...localStorage }).includes(key),
      secret,
    ),
  ).toBe(false);
  await expect(
    page.getByText("Laptop collector", { exact: true }),
  ).toBeVisible();
  await accessible(page);
  await proof(page, info, "member-project-connections");
  const old = await member.api("/v1/turns");
  const record = {
    ...old.body.turns[0],
    event_id: "console-project-record",
    content: {
      input: "Console linked record",
      output: "Disposable project isolation proof",
    },
  };
  const posted = await rpc(
    member,
    "/v1/turns",
    "POST",
    { turns: [record] },
    secret,
  );
  expect(posted.status).toBe(200);
  const project = new URLSearchParams(new URL(page.url()).hash.slice(1)).get(
    "project",
  );
  await page
    .getByRole("link", { name: "Open monitoring", exact: true })
    .click();
  await expect(page).toHaveURL(new RegExp("workspace=" + project));
  await expect(page.locator("#app")).toBeVisible();
  await page.evaluate(() => {
    location.hash = "#activity";
  });
  await expect(
    page.getByText("Console linked record", { exact: false }).first(),
  ).toBeVisible();
  await expect(
    page.getByText("PRIVATE_DATASET_FIXTURE_PROMPT", { exact: false }),
  ).toHaveCount(0);
  if (await page.locator("#menu-toggle").isVisible())
    await page.locator("#menu-toggle").click();
  await expect(
    page.getByRole("link", { name: "Organizations & connections" }),
  ).toBeVisible();
});

test("key revocation takes effect and a viewer gets monitoring without management controls", async ({
  page,
  member,
}) => {
  const team = await newTeam(member, "Shared team");
  const key = await rpc(member, `/v1/projects/${team.project}/keys`, "POST", {
    label: "Build worker",
  });
  expect(key.status).toBe(200);
  await connect(page, member);
  await page.goto(
    `${member.base}/app/manage#org=${team.org}&project=${team.project}`,
  );
  await page
    .getByRole("button", { name: "Revoke Build worker", exact: true })
    .click();
  await page.getByRole("button", { name: "Revoke key", exact: true }).click();
  await expect(page.getByText("Revoked", { exact: true })).toBeVisible();
  expect(
    (await rpc(member, "/v1/turns", "POST", { turns: [] }, key.body.key))
      .status,
  ).toBe(401);
  const invitation = await rpc(
    member,
    `/v1/organizations/${team.org}/invitations`,
    "POST",
    { role: "viewer" },
  );
  expect(invitation.status).toBe(200);
  expect(
    (
      await rpc(
        member,
        "/v1/invitations/accept",
        "POST",
        { token: invitation.body.token },
        member.otherToken,
      )
    ).status,
  ).toBe(200);
  await page.setExtraHTTPHeaders({
    Authorization: "Bearer " + member.otherToken,
  });
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "An admin manages connections" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Create key", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("Build worker", { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole("link", { name: "Open monitoring", exact: true }),
  ).toBeVisible();
  await accessible(page);
});

test("changing organizations discards an earlier private key-list response", async ({
  page,
  member,
}) => {
  const one = await newTeam(member, "First team"),
    two = await newTeam(member, "Second team");
  await rpc(member, `/v1/projects/${one.project}/keys`, "POST", {
    label: "PRIVATE_OLD_PROJECT_KEY",
  });
  await connect(page, member);
  let release, arrived;
  const received = new Promise((r) => (arrived = r)),
    gate = new Promise((r) => (release = r));
  await page.route(`**/v1/projects/${one.project}/keys`, async (r) => {
    const response = await r.fetch();
    arrived();
    await gate;
    await r.fulfill({ response }).catch(() => {});
  });
  try {
    await page.goto(
      `${member.base}/app/manage#org=${one.org}&project=${one.project}`,
    );
    await received;
    await page
      .getByLabel("Organization", { exact: true })
      .selectOption(two.org);
    await expect(
      page.getByRole("heading", { name: "Your work, organized." }),
    ).toBeVisible();
    release();
    await page.waitForTimeout(100);
    await expect(
      page.getByText("PRIVATE_OLD_PROJECT_KEY", { exact: true }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("link", { name: "Second team project", exact: true }),
    ).toBeVisible();
  } finally {
    release?.();
  }
});

test("membership errors, literal names, dark appearance and signed-out entry remain clear", async ({
  page,
  member,
}, info) => {
  const team = await newTeam(member, "<img src=x onerror=alert(1)>");
  await connect(page, member);
  await page.goto(`${member.base}/app/manage#org=${team.org}`);
  await expect(
    page.getByRole("link", {
      name: "<img src=x onerror=alert(1)> project",
      exact: true,
    }),
  ).toBeVisible();
  expect(await page.locator("img").count()).toBe(0);
  await accessible(page);
  await page.emulateMedia({ colorScheme: "dark" });
  await accessible(page);
  await proof(page, info, "member-dark");
  await page.setExtraHTTPHeaders({
    Authorization: "Bearer " + member.otherToken,
  });
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "This organization is unavailable" }),
  ).toBeVisible();
  await expect(
    page.getByText("<img src=x onerror=alert(1)> project", { exact: true }),
  ).toHaveCount(0);
  await page.setExtraHTTPHeaders({});
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Your workspace starts here" }),
  ).toBeVisible();
  await expect(
    page.getByText("Account sign-in is not configured on this deployment yet."),
  ).toBeVisible();
  await expect(page.getByLabel("Devspace ingest key")).toHaveCount(0);
  await accessible(page);
});
