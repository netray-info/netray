import { test, expect } from '@playwright/test';

test('homepage loads and shows welcome state', async ({ page }) => {
  await page.goto('/');

  // Logo and tagline are visible
  await expect(page.locator('.logo')).toBeVisible();
  await expect(page.locator('.tagline')).toContainText('TLS, illuminated');

  // Welcome cards with examples are shown
  await expect(page.locator('.welcome')).toBeVisible();
  await expect(page.locator('.welcome-card').first()).toBeVisible();
});

test('theme toggle cycles themes', async ({ page }) => {
  await page.goto('/');

  const toggle = page.locator('.header-btn', { hasText: /[☾☀◐]/ });
  await expect(toggle).toBeVisible();

  // Click cycles through themes
  await toggle.click();
  const theme = await page.locator('html').getAttribute('data-theme');
  expect(['dark', 'light', 'system']).toContain(theme);
});

test('inspect example.com via input', async ({ page }) => {
  await page.goto('/');

  // Type hostname and submit
  const input = page.locator('input[type="text"]');
  await expect(input).toBeVisible();
  await input.fill('example.com');
  await input.press('Enter');

  // Loading indicator should appear
  await expect(page.locator('.loading-indicator')).toBeVisible();

  // Results should appear (generous timeout for real TLS handshake)
  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });

  // Results summary shows IP count, port count, and duration
  const summary = page.locator('.results-summary');
  await expect(summary).toBeVisible();
  await expect(summary).toContainText(/\d+ IP/);
  await expect(summary).toContainText(/\d+ port/);
  await expect(summary).toContainText(/\d+ms/);
});

test('inspect via welcome card example button', async ({ page }) => {
  await page.goto('/');

  // Click the first example button (example.com)
  const exampleBtn = page.locator('.welcome-example', { hasText: 'example.com' }).first();
  await expect(exampleBtn).toBeVisible();
  await exampleBtn.click();

  // Results should appear
  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });
  await expect(page.locator('.results-summary')).toContainText(/\d+ms/);
});

test('results show validation summary', async ({ page }) => {
  await page.goto('/?h=example.com');

  // Wait for results
  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });

  // Validation summary section should be present
  const validation = page.locator('.validation-summary, [class*="validation"]');
  await expect(validation.first()).toBeVisible();
});

test('results show IP card with certificate info', async ({ page }) => {
  await page.goto('/?h=example.com');

  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });

  // At least one IP card should be rendered
  const ipCard = page.locator('.ip-card').first();
  await expect(ipCard).toBeVisible();

  // IP card header should contain an IP address
  const header = ipCard.locator('.ip-card__header');
  await expect(header).toBeVisible();
  const headerText = await header.textContent();
  // Should contain an IPv4 or IPv6 address
  expect(headerText).toMatch(/\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}|[0-9a-f:]+/i);
});

test('URL updates with query parameter after inspect', async ({ page }) => {
  await page.goto('/');

  const input = page.locator('input[type="text"]');
  await input.fill('example.com');
  await input.press('Enter');

  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });

  // URL should contain ?h=example.com
  expect(page.url()).toContain('h=example.com');
});

test('direct URL with ?h= parameter triggers inspection', async ({ page }) => {
  await page.goto('/?h=example.com');

  // Should go straight to results (no welcome state)
  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });
  await expect(page.locator('.welcome')).not.toBeVisible();
});

test('error state for unreachable host', async ({ page }) => {
  await page.goto('/');

  const input = page.locator('input[type="text"]');
  await input.fill('this-domain-does-not-exist-12345.example.com');
  await input.press('Enter');

  // Should show an error banner
  await expect(page.locator('.error-banner')).toBeVisible({ timeout: 30000 });
});

test('explain toggle works', async ({ page }) => {
  await page.goto('/?h=example.com');

  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });

  // Click explain toggle
  const explainBtn = page.locator('.filter-toggle', { hasText: 'explain' });
  await expect(explainBtn).toBeVisible();
  await explainBtn.click();

  // Button should become active
  await expect(explainBtn).toHaveClass(/active/);
});

test('expand/collapse all toggle works', async ({ page }) => {
  await page.goto('/?h=example.com');

  await expect(page.locator('.results')).toBeVisible({ timeout: 30000 });

  const toggleBtn = page.locator('.filter-toggle', { hasText: /expand all|collapse all/ });
  await expect(toggleBtn).toBeVisible();

  // Click to expand all
  await toggleBtn.click();
  await expect(toggleBtn).toContainText('collapse all');

  // Click to collapse all
  await toggleBtn.click();
  await expect(toggleBtn).toContainText('expand all');
});

test('footer contains expected links', async ({ page }) => {
  await page.goto('/');

  // GitHub link
  await expect(page.locator('footer a[href*="github.com"]').first()).toBeVisible();

  // API docs link
  await expect(page.locator('footer a[href="/docs"]')).toBeVisible();
});

test('help modal opens and closes', async ({ page }) => {
  await page.goto('/');

  // Click help button
  const helpBtn = page.locator('.header-btn', { hasText: '?' });
  await expect(helpBtn).toBeVisible();
  await helpBtn.click();

  // Modal should be visible
  const modal = page.locator('.modal');
  await expect(modal).toBeVisible();
  await expect(modal).toContainText('Keyboard shortcuts');

  // Close modal
  const closeBtn = modal.locator('.modal__close');
  await closeBtn.click();
  await expect(modal).not.toBeVisible();
});
