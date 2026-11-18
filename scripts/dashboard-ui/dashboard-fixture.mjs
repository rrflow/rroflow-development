import AxeBuilder from '@axe-core/playwright';
import { test as base, expect } from '@playwright/test';
import { startFixture } from './service.mjs';

// Each spec calls this once, so each spec file gets its own worker and a fresh fixture service:
// no spec depends on what another one wrote.
export const fixtureTest = () =>
  base.extend({
    service: [
      // biome-ignore lint/correctness/noEmptyPattern: Playwright requires a fixture to destructure its first argument, even when it uses no other fixture.
      async ({}, use) => {
        const service = await startFixture();
        try {
          await use(service);
        } finally {
          await service.close();
        }
      },
      { scope: 'worker' },
    ],
  });
export async function signIn(page, service) {
  await page.goto(`${service.base}/app`);
  await page.getByText('Advanced: use a devspace key', { exact: true }).click();
  await page.getByLabel('Devspace ingest key', { exact: true }).fill(service.key);
  await page.getByRole('button', { name: 'Open devspace', exact: true }).click();
  await expect(page.locator('#app')).toBeVisible();
  await settled(page);
  expect(await page.evaluate(() => window.scrollY)).toBe(0);
}
export async function settled(page) {
  await expect(page.locator('#loading')).toBeHidden();
  await expect(page.locator('#app-error')).toBeHidden();
}
export async function navigate(page, route) {
  await page.evaluate((route) => {
    location.hash = `#${route}`;
  }, route);
  await expect(page.locator(`[data-nav="${route}"]`)).toHaveAttribute('aria-current', 'page');
  await settled(page);
}
export async function reflow(page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(
    true,
  );
}
export async function accessible(page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  expect(
    results.violations.map((v) => ({
      id: v.id,
      impact: v.impact,
      nodes: v.nodes.map((n) => ({ target: n.target, summary: n.failureSummary })),
    })),
  ).toEqual([]);
}
export async function screenshot(page, info, name) {
  // Contrast inspection can temporarily hide artwork. Let restored styles paint
  // before collecting visual evidence; DOM visibility alone misses this frame.
  await page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );
  const path = info.outputPath(`${name}-test-fixture.png`);
  await page.screenshot({ path, fullPage: false });
  await info.attach(`${name} - disposable test records`, { path, contentType: 'image/png' });
}

// Requests outside the fixture service are refused. Each spec registers this with test.beforeEach.
export async function sameOriginOnly({ page, service }) {
  await page.route('**/*', (route) =>
    new URL(route.request().url()).origin === service.base ? route.continue() : route.abort(),
  );
}

export { expect };
