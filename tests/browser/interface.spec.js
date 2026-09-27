import { expect, test } from '@playwright/test';

const requestHeaders = { 'X-Requested-With': 'custom-plex' };

async function expectCompact(button) {
  const box = await button.boundingBox();
  expect(box).not.toBeNull();
  expect(box.width).toBeLessThanOrEqual(36);
  expect(box.height).toBeLessThanOrEqual(36);
}

async function expectTooltip(button, text) {
  await button.hover();
  await expect.poll(() => button.evaluate(element => ({
    content: getComputedStyle(element, '::after').content,
    opacity: getComputedStyle(element, '::after').opacity,
  }))).toEqual({ content: `"${text}"`, opacity: '1' });
}

test('compact controls, descriptions, and movie resume work in a browser', async ({ page, request }) => {
  let albumId;
  try {
    await page.goto('/');
    await page.getByRole('button', { name: /Parents/ }).click();
    await page.getByLabel('Parents password').fill('browser-test-password');
    await page.getByRole('button', { name: 'Unlock collection' }).click();

    const movie = page.locator('article.movie').filter({ hasText: 'Browser-Test' });
    await expect(movie).toBeVisible();
    const renameMovie = movie.getByRole('button', { name: 'Rename Browser-Test' });
    const imageMovie = movie.getByRole('button', { name: 'Add or change image for Browser-Test' });
    await expectCompact(renameMovie);
    await expectCompact(imageMovie);
    await expectTooltip(renameMovie, 'Rename movie');

    await movie.getByRole('button', { name: 'Play Browser-Test' }).click();
    const video = page.locator('#video');
    await expect.poll(() => video.evaluate(element => element.readyState)).toBeGreaterThanOrEqual(1);
    const saved = page.waitForResponse(response =>
      response.url().includes('/progress') && response.request().method() === 'POST' && response.status() === 204
    );
    await video.evaluate(element => {
      element.currentTime = 12;
      element.dispatchEvent(new Event('pause'));
    });
    await saved;
    await page.getByRole('button', { name: 'Close player' }).click();
    await expect(movie.getByText('Resume at 0:12')).toBeVisible();

    await movie.getByRole('button', { name: 'Play Browser-Test' }).click();
    await expect.poll(() => video.evaluate(element => element.currentTime)).toBeGreaterThan(11);
    await page.getByRole('button', { name: 'Close player' }).click();

    await page.getByRole('button', { name: 'Switch profile' }).click();
    await page.getByRole('button', { name: /Photo Album/ }).click();
    await page.getByLabel('New album name').fill('Browser verification album');
    await page.getByRole('button', { name: 'Create album' }).click();
    await page.getByRole('button', { name: 'All albums' }).click();

    const albums = await request.get('/api/albums');
    expect(albums.ok()).toBeTruthy();
    albumId = (await albums.json()).find(album => album.name === 'Browser verification album').id;
    const album = page.locator('article.album-card').filter({ hasText: 'Browser verification album' });
    const renameAlbum = album.getByRole('button', { name: 'Rename Browser verification album' });
    const describeAlbum = album.getByRole('button', { name: 'Edit description for Browser verification album' });
    await expectCompact(renameAlbum);
    await expectCompact(describeAlbum);
    await expectTooltip(describeAlbum, 'Edit description');
    await describeAlbum.click();
    await page.getByRole('textbox', { name: 'Description', exact: true }).fill('A description saved through the browser.');
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(album.getByText('A description saved through the browser.')).toBeVisible();

    const uploadStatus = await page.evaluate(async id => {
      const canvas = document.createElement('canvas'); canvas.width = 8; canvas.height = 8;
      const context = canvas.getContext('2d'); context.fillStyle = '#286496'; context.fillRect(0, 0, 8, 8);
      const image = await new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
      return (await fetch(`/api/albums/${id}/photos?name=browser.png`, {
        method: 'POST',
        headers: { 'Content-Type': 'image/png', 'X-Requested-With': 'custom-plex' },
        body: image,
      })).status;
    }, albumId);
    expect(uploadStatus).toBe(201);
    await album.getByRole('button', { name: 'Open Browser verification album' }).click();
    const photoTile = page.getByRole('button', { name: 'browser.png' });
    await expect(photoTile).toBeVisible();
    await photoTile.click();
    const renamePhoto = page.getByRole('button', { name: 'Rename photo' });
    const movePhoto = page.getByRole('button', { name: 'Move photo to another album' });
    const deletePhoto = page.getByRole('button', { name: 'Delete photo' });
    for (const control of [renamePhoto, movePhoto, deletePhoto]) await expectCompact(control);
    await expectTooltip(movePhoto, 'Move to album');
    const photoDescription = page.getByRole('textbox', { name: 'Photo description' });
    await expect(photoDescription).toBeVisible();
    await photoDescription.fill('A photo description saved through the browser.');
    await page.getByRole('button', { name: 'Save description' }).click();
    await expect(page.getByText('Description saved.', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Close photo' }).click();
    await photoTile.click();
    await expect(photoDescription).toHaveValue('A photo description saved through the browser.');
  } finally {
    if (albumId) {
      await request.delete(`/api/albums/${albumId}`, { headers: requestHeaders });
    }
  }
});
