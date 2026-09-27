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

test('homepage slideshow selects albums, plays, pauses, wraps, and enters native fullscreen', async ({ page, request }) => {
  const ids = [];
  try {
    await page.goto('/');
    await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
    await expect(page.locator('#slide-status')).toContainText('No photos selected');
    await expect(page.locator('#slide-pause')).toBeDisabled();
    await page.getByRole('button', { name: 'Close slideshow' }).click();
    for (const name of ['Slideshow memories', 'Excluded memories']) {
      const response = await request.post('/api/albums', { headers: requestHeaders, data: { name } });
      const album = await response.json(); ids.push(album.id);
      // Real image bytes, served by the application's preview endpoint.
      await page.evaluate(async id => {
        const canvas = document.createElement('canvas'); canvas.width = 20; canvas.height = 10;
        for (const name of ['first.png', 'second.png']) {
          const blob = await new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
          const response = await fetch(`/api/albums/${id}/photos?name=${name}`, {
            method: 'POST', headers: { 'X-Requested-With': 'custom-plex' }, body: blob,
          });
          if (!response.ok) throw new Error('Fixture upload failed');
        }
      }, album.id);
    }
    await page.getByRole('button', { name: /Photo Album/ }).click();
    const choice = page.getByRole('checkbox', { name: 'Use Slideshow memories in slideshow', exact: true });
    await choice.check();
    await expect(choice).toBeEnabled();
    await page.reload();
    await page.getByRole('button', { name: /Photo Album/ }).click();
    await expect(choice).toBeChecked();
    await expect(page.getByRole('checkbox', { name: 'Use Excluded memories in slideshow' })).not.toBeChecked();
    await page.getByRole('button', { name: 'Switch profile' }).click();
    await page.clock.install();
    await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
    await expect(page.locator('#slide-status')).toHaveText('1 of 2');
    await expect.poll(() => page.locator('#slide-image').evaluate(img => img.complete && img.naturalWidth > 0)).toBe(true);
    await page.clock.fastForward(5000);
    await expect(page.locator('#slide-status')).toHaveText('2 of 2');
    await page.getByRole('button', { name: 'Pause slideshow', exact: true }).click();
    await page.clock.fastForward(15000);
    await expect(page.locator('#slide-status')).toHaveText('2 of 2');
    await page.getByRole('button', { name: 'Next slide', exact: true }).click();
    await expect(page.locator('#slide-status')).toHaveText('1 of 2');
    await page.keyboard.press('ArrowLeft');
    await expect(page.locator('#slide-status')).toHaveText('2 of 2');
    await page.getByRole('button', { name: 'Resume slideshow', exact: true }).click();
    await page.clock.fastForward(5000);
    await expect(page.locator('#slide-status')).toHaveText('1 of 2');
    await page.getByRole('button', { name: 'Enter fullscreen', exact: true }).click();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement?.id)).toBe('slideshow-screen');
    await expect(page.locator('#slide-image')).toHaveJSProperty('naturalWidth', 20);
    await expect(page.locator('#slide-caption')).not.toBeVisible();
    await expect(page.locator('.slideshow-controls')).not.toBeVisible();
    await expect(page.getByRole('button', { name: 'Close slideshow' })).not.toBeVisible();
    const bounds = await page.locator('#slide-image').boundingBox();
    const viewport = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }));
    expect(bounds).toEqual({ x: 0, y: 0, ...viewport });
    await expect(page.getByRole('button', { name: 'Exit fullscreen', exact: true })).toHaveText('×');
    await page.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
    await expect(page.locator('#slideshow-player')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Enter fullscreen', exact: true })).toBeFocused();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement === null)).toBe(true);
    await page.keyboard.press('Escape');
    await expect(page.locator('#slideshow-player')).not.toBeVisible();
    await expect(page.getByRole('button', { name: 'Play slideshow', exact: true })).toBeFocused();
    await page.clock.fastForward(15000);
    await expect(page.locator('#slide-image')).not.toHaveAttribute('src');
    await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
    await expect(page.locator('#slide-status')).toHaveText('1 of 2');
    await page.getByRole('button', { name: 'Enter fullscreen', exact: true }).click();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement?.id)).toBe('slideshow-screen');
    await page.keyboard.press('Space');
    await expect(page.locator('#slide-pause')).toHaveText('Resume slideshow');
    await page.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
    await page.getByRole('button', { name: 'Close slideshow' }).click();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement === null)).toBe(true);
    await expect(page.getByRole('button', { name: 'Play slideshow', exact: true })).toBeFocused();
    await page.getByRole('button', { name: /Photo Album/ }).click();
    await choice.uncheck();
    await expect(choice).toBeEnabled();
    await page.getByRole('button', { name: 'Switch profile' }).click();
    await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
    await expect(page.locator('#slide-status')).toContainText('No photos selected');
  } finally {
    for (const id of ids) await request.delete(`/api/albums/${id}`, { headers: requestHeaders });
  }
});

test('slideshow handles load failures, close during loading, and unavailable photos', async ({ page }) => {
  await page.goto('/');
  await page.route('**/api/slideshow', route => route.fulfill({ status: 500, body: '' }));
  await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
  await expect(page.locator('#slide-status')).toContainText('Unable to load slideshow');
  await page.getByRole('button', { name: 'Close slideshow' }).click();
  let release;
  const blocked = new Promise(resolve => { release = resolve; });
  await page.route('**/api/slideshow', async route => {
    await blocked;
    await route.fulfill({ json: [{ id: 999999, name: 'Missing photo', description: '' }] });
  });
  await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
  await expect(page.locator('#slide-status')).toHaveText('Loading slideshow…');
  await page.getByRole('button', { name: 'Close slideshow' }).click();
  release();
  await expect(page.locator('#slideshow-player')).not.toBeVisible();
  await expect(page.locator('#slide-image')).not.toHaveAttribute('src');
  await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
  await expect(page.locator('#slide-status')).toContainText('This photo is unavailable');
  await page.getByRole('button', { name: 'Next slide', exact: true }).click();
  await expect(page.locator('#slide-caption')).toHaveText('Missing photo');
});

test('slideshow timing and effects persist and respect reduced motion', async ({ page }) => {
  await page.route('**/api/slideshow', route => route.fulfill({ json: [
    { id: 900001, name: 'First', description: '' }, { id: 900002, name: 'Second', description: '' },
  ] }));
  await page.route('**/api/photos/90000*/preview', route => route.fulfill({
    contentType: 'image/svg+xml', body: '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="blue"/></svg>',
  }));
  await page.goto('/');
  await page.clock.install();
  await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
  await expect(page.locator('#slide-status')).toHaveText('1 of 2');
  await expect(page.getByLabel('Transition duration')).toBeDisabled();
  await page.getByLabel('Time per photo').selectOption('10');
  await page.clock.fastForward(5000);
  await expect(page.locator('#slide-status')).toHaveText('1 of 2');
  await page.clock.fastForward(5000);
  await expect(page.locator('#slide-status')).toHaveText('2 of 2');
  await page.getByRole('button', { name: 'Pause slideshow', exact: true }).click();
  for (const effect of ['fade', 'slide', 'zoom']) {
    await page.getByLabel('Transition effect').selectOption(effect);
    await page.getByLabel('Transition duration').selectOption('2000');
    await page.getByRole('button', { name: 'Next slide', exact: true }).click();
    await expect.poll(() => page.locator('#slide-image').evaluate(img => img.getAnimations().length)).toBe(1);
    const animation = await page.locator('#slide-image').evaluate(img => {
      const animation = img.getAnimations()[0]; animation.pause(); animation.currentTime = 1000;
      return { duration: animation.effect.getTiming().duration, frames: animation.effect.getKeyframes(), opacity: getComputedStyle(img).opacity };
    });
    expect(animation.duration).toBe(2000);
    expect(Number(animation.opacity)).toBeGreaterThan(0);
    expect(Number(animation.opacity)).toBeLessThan(1);
    if (effect === 'slide') expect(animation.frames[0].transform).toContain('translateX');
    if (effect === 'zoom') expect(animation.frames[0].transform).toContain('scale');
  }
  await page.reload();
  await page.getByRole('button', { name: 'Play slideshow', exact: true }).click();
  await expect(page.getByLabel('Time per photo')).toHaveValue('10');
  await expect(page.getByLabel('Transition effect')).toHaveValue('zoom');
  await expect(page.getByLabel('Transition duration')).toHaveValue('2000');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(page.locator('#slide-status')).toHaveText('1 of 2');
  await page.getByRole('button', { name: 'Next slide', exact: true }).click();
  await expect(page.locator('#slide-status')).toHaveText('2 of 2');
  await expect.poll(() => page.locator('#slide-image').evaluate(img => img.complete)).toBe(true);
  expect(await page.locator('#slide-image').evaluate(img => img.getAnimations().length)).toBe(0);
  await page.getByRole('button', { name: 'Enter fullscreen', exact: true }).click();
  await expect(page.locator('.slideshow-settings')).not.toBeVisible();
  await page.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
  await expect(page.getByLabel('Transition effect')).toBeVisible();
  await page.getByLabel('Transition effect').selectOption('none');
  await expect(page.getByLabel('Transition duration')).toBeDisabled();
});
