import { expect, test } from '@playwright/test';

test('movie alphabet sorts titles, combines search, and scrolls on mobile', async ({ page }) => {
  await page.route('**/api/movies', route => route.fulfill({ json: [
    'Zulu', 'apple', 'Arrival', 'Éclair', '2001', 'Bravo'
  ].map((title, id) => ({ id, title, approved: true })) }));
  await page.goto('/');
  await page.getByRole('button', { name: /Baby/ }).click();
  const titles = page.locator('#movies h2');
  const alphabet = page.getByRole('group', { name: 'Filter movies by first letter' });
  await expect(titles).toHaveText(['2001', 'apple', 'Arrival', 'Bravo', 'Éclair', 'Zulu']);
  await alphabet.getByRole('button', { name: 'A', exact: true }).click();
  await expect(titles).toHaveText(['apple', 'Arrival']);
  await expect(alphabet.getByRole('button', { name: 'A', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByLabel('Find a movie').fill('arrival');
  await expect(titles).toHaveText(['Arrival']);
  await page.getByLabel('Find a movie').fill('missing');
  await expect(page.locator('#empty')).toBeVisible();
  await page.getByLabel('Find a movie').fill('');
  await alphabet.getByRole('button', { name: 'E', exact: true }).click();
  await expect(titles).toHaveText(['Éclair']);
  await alphabet.getByRole('button', { name: 'Numbers and other titles' }).click();
  await expect(titles).toHaveText(['2001']);
  await page.setViewportSize({ width: 390, height: 844 });
  await alphabet.getByRole('button', { name: 'Z', exact: true }).focus();
  await page.keyboard.press('Enter');
  await expect(titles).toHaveText(['Zulu']);
  expect(await alphabet.evaluate(el => el.scrollWidth > el.clientWidth && el.scrollLeft > 0)).toBe(true);
  const barBounds = await alphabet.boundingBox();
  expect(barBounds.x + barBounds.width).toBeLessThanOrEqual(390);
  await page.getByRole('button', { name: 'Switch profile' }).click();
  await page.getByRole('button', { name: /Baby/ }).click();
  await expect(titles).toHaveCount(6);
  await expect(alphabet.getByRole('button', { name: 'All', exact: true })).toHaveAttribute('aria-pressed', 'true');
});
