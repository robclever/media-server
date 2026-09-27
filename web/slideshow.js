'use strict';
/** Homepage slideshow: fresh playlist per opening, one timer, and native fullscreen. */
(() => {
  const dialog = $('slideshow-player');
  const screen = $('slideshow-screen');
  let slides = [], index = 0, timer, playing = true, generation = 0;
  const preferences = { interval: '5', effect: 'none', duration: '600', shuffle: 'off', captions: 'off' };
  let transition, animateNext = false, captionTimer;
  let originalSlides = [], albums = [], selectedAlbums = [], presets = [], playlistVersion = 0;
  /** Validates stored choices against the controls and tolerates blocked storage. */
  function restorePreferences() {
    let saved;
    try { saved = JSON.parse(localStorage.getItem('slideshow-options')); } catch (_) {}
    for (const key of Object.keys(preferences)) {
      const control = $(`slide-${key}`);
      if ([...control.options].some(option => option.value === saved?.[key])) preferences[key] = saved[key];
      control.value = preferences[key];
      control.onchange = () => {
        preferences[key] = control.value;
        try { localStorage.setItem('slideshow-options', JSON.stringify(preferences)); } catch (_) {}
        if (key === 'shuffle') { reorder(); render(); }
        else if (key === 'captions') showCaption();
        else if (key === 'interval') schedule();
        else { transition?.cancel(); animateNext = false; }
        $('slide-duration').disabled = preferences.effect === 'none';
      };
    }
    $('slide-duration').disabled = preferences.effect === 'none';
  }
  restorePreferences();
  /** Fisher–Yates produces one complete pass, avoiding a repeat at its boundary. */
  function shuffled(items, previous) {
    const result = [...items];
    for (let i = result.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [result[i], result[j]] = [result[j], result[i]];
    }
    if (result.length > 1 && result[0].id === previous) [result[0], result[1]] = [result[1], result[0]];
    return result;
  }
  /** Rebuilds the order when album choices or shuffle mode change. */
  function reorder() {
    slides = preferences.shuffle === 'on' ? shuffled(originalSlides) : [...originalSlides];
    index = 0;
  }
  /** Shows optional captions safely and expires brief captions independently of playback. */
  function showCaption() {
    clearTimeout(captionTimer);
    const visible = preferences.captions !== 'off' && !!slides[index];
    screen.classList.toggle('show-captions', visible);
    if (visible && preferences.captions === 'brief') captionTimer = setTimeout(() => screen.classList.remove('show-captions'), 3000);
  }
  /** Animates only a loaded replacement image, respecting reduced-motion preferences. */
  $('slide-image').onload = () => {
    transition?.cancel();
    if (animateNext && dialog.open && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
      const effects = {
        fade: [{ opacity: 0 }, { opacity: 1 }],
        slide: [{ opacity: 0, transform: 'translateX(6%)' }, { opacity: 1, transform: 'translateX(0)' }],
        zoom: [{ opacity: 0, transform: 'scale(.94)' }, { opacity: 1, transform: 'scale(1)' }],
      };
      if (effects[preferences.effect]) transition = $('slide-image').animate(effects[preferences.effect], {
        duration: Number(preferences.duration), easing: 'ease-in-out',
      });
    }
    animateNext = false;
  };
  /** Schedules only while open, playing, and visible; closing releases the timer. */
  function schedule() {
    clearTimeout(timer);
    if (dialog.open && playing && slides.length > 1 && !document.hidden) {
      timer = setTimeout(() => advance(1), Number(preferences.interval) * 1000);
    }
  }
  /** Renders text safely and loads the bounded preview instead of the original. */
  function render() {
    const slide = slides[index];
    $('slide-image').hidden = !slide;
    if (!slide) { clearTimeout(captionTimer); screen.classList.remove('show-captions'); $('slide-caption').textContent = ''; }
    for (const id of ['slide-previous', 'slide-next', 'slide-pause']) $(id).disabled = !slide;
    $('slide-pause').textContent = playing ? 'Pause slideshow' : 'Resume slideshow';
    if (slide) {
      $('slide-image').alt = slide.name;
      const source = `/api/photos/${slide.id}/preview`;
      if ($('slide-image').getAttribute('src') !== source) {
        transition?.cancel();
        animateNext = $('slide-image').hasAttribute('src');
        showCaption();
        $('slide-image').src = source;
      }
      $('slide-caption').textContent = [slide.name, slide.description].filter(Boolean).join(' — ');
      $('slide-status').textContent = `${index + 1} of ${slides.length}`;
    }
    schedule();
  }
  /** Wraps navigation at either end and resets the automatic advance interval. */
  function advance(direction) {
    if (!slides.length) return;
    if (direction > 0 && index === slides.length - 1 && preferences.shuffle === 'on') {
      slides = shuffled(originalSlides, slides[index].id); index = 0;
    } else index = (index + direction + slides.length) % slides.length;
    render();
  }
  $('start-slideshow').onclick = async () => {
    const version = ++generation;
    const playlistRequest = ++playlistVersion;
    slides = []; originalSlides = []; index = 0; playing = true;
    screen.classList.remove('show-captions');
    loadSetup(version);
    $('slide-caption').textContent = '';
    $('slide-status').textContent = 'Loading slideshow…';
    document.body.classList.add('slideshow-open');
    dialog.showModal(); render();
    try {
      const result = await api('/api/slideshow');
      if (version !== generation || playlistRequest !== playlistVersion || !dialog.open) return;
      originalSlides = result; reorder();
      if (!slides.length) $('slide-status').textContent = 'No photos selected. Open Photo Album and choose “Use in slideshow” on an album with photos.';
      render();
    } catch (error) {
      if (version === generation && dialog.open) $('slide-status').textContent = `Unable to load slideshow: ${error.message}`;
    }
  };
  /** Fetches metadata without letting late responses reopen a closed player. */
  async function loadSetup(version) {
    $('slide-setup-status').textContent = 'Loading albums and presets…';
    try {
      const [available, saved] = await Promise.all([api('/api/albums'), api('/api/slideshow/presets')]);
      if (version !== generation || !dialog.open) return;
      albums = available; presets = saved; selectedAlbums = albums.filter(a => a.slideshow).map(a => a.id);
      renderAlbums(); renderPresets(); $('slide-setup-status').textContent = '';
    } catch (error) { if (version === generation) $('slide-setup-status').textContent = error.message; }
  }
  /** Album choices are local to this playback and never change the homepage default. */
  function renderAlbums() {
    $('slide-albums').replaceChildren();
    for (const album of albums) {
      const label = document.createElement('label'), box = document.createElement('input');
      box.type = 'checkbox'; box.checked = selectedAlbums.includes(album.id);
      box.onchange = () => {
        selectedAlbums = box.checked ? [...selectedAlbums, album.id] : selectedAlbums.filter(id => id !== album.id);
        loadPlaylist();
      };
      label.append(box, document.createTextNode(album.name)); $('slide-albums').append(label);
    }
  }
  /** Refreshes the explicit selection and ignores overlapping/closed requests. */
  async function loadPlaylist() {
    const version = ++playlistVersion;
    clearTimeout(timer); slides = []; originalSlides = []; $('slide-image').removeAttribute('src'); render();
    $('slide-status').textContent = 'Loading slideshow…';
    try {
      const result = await api(`/api/slideshow?albums=${selectedAlbums.join(',')}`);
      if (version !== playlistVersion || !dialog.open) return;
      originalSlides = result; reorder(); render();
      if (!slides.length) $('slide-status').textContent = 'No photos in the selected albums.';
    } catch (error) { if (version === playlistVersion && dialog.open) $('slide-status').textContent = error.message; }
  }
  /** Rebuilds preset choices using text nodes rather than user-provided markup. */
  function renderPresets(id = '') {
    $('slide-preset').replaceChildren(new Option('Choose a preset', ''));
    for (const preset of presets) $('slide-preset').append(new Option(preset.name, preset.id));
    $('slide-preset').value = String(id); presetSelection();
  }
  function presetSelection() {
    const preset = presets.find(p => String(p.id) === $('slide-preset').value);
    for (const id of ['slide-update-preset', 'slide-delete-preset', 'slide-load-preset']) $(id).disabled = !preset;
  }
  /** Accepts only canonical Spotify content URLs, stripping share tracking parameters. */
  function spotifyLink(value) {
    if (!value.trim()) return '';
    try {
      const url = new URL(value);
      if (url.protocol === 'https:' && url.hostname === 'open.spotify.com' && !url.port && !url.username && !url.password && /^\/(playlist|album|track)\/[a-zA-Z0-9]{22}$/.test(url.pathname)) return url.origin + url.pathname;
    } catch (_) {}
    throw new Error('Use a Spotify playlist, album, or track link from open.spotify.com.');
  }
  function updateSpotify() {
    const link = $('slide-open-spotify'); link.hidden = true; link.removeAttribute('href');
    try {
      const value = spotifyLink($('slide-spotify').value);
      if (value) { link.href = value; link.hidden = false; }
      return value;
    } catch (error) { $('slide-setup-status').textContent = error.message; throw error; }
  }
  $('slide-spotify').oninput = () => { try { updateSpotify(); } catch (_) {} };
  $('slide-preset').onchange = presetSelection;
  $('slide-load-preset').onclick = async () => {
    const preset = presets.find(p => String(p.id) === $('slide-preset').value);
    if (!preset) return;
    for (const key of Object.keys(preferences)) {
      preferences[key] = preset.options[key]; $(`slide-${key}`).value = preferences[key];
    }
    try { localStorage.setItem('slideshow-options', JSON.stringify(preferences)); } catch (_) {}
    $('slide-duration').disabled = preferences.effect === 'none';
    $('slide-spotify').value = preset.options.spotify || ''; updateSpotify();
    $('slide-preset-name').value = preset.name;
    selectedAlbums = preset.album_ids.filter(id => albums.some(a => a.id === id));
    $('slide-setup-status').textContent = selectedAlbums.length < preset.album_ids.length ? 'Some saved albums were deleted and have been skipped.' : `Loaded ${preset.name}.`;
    renderAlbums(); await loadPlaylist(); showCaption();
  };
  /** Saves explicit create/update actions; failures retain the edited form. */
  async function savePreset(update) {
    const name = $('slide-preset-name').value.trim();
    if (!name) { $('slide-setup-status').textContent = 'Enter a preset name first.'; return; }
    const id = $('slide-preset').value;
    const payload = { name, album_ids: selectedAlbums, options: { ...preferences, spotify: updateSpotify() } };
    const saved = await api(update ? `/api/slideshow/presets/${id}` : '/api/slideshow/presets', payload);
    presets = await api('/api/slideshow/presets'); renderPresets(update ? id : saved.id);
    $('slide-setup-status').textContent = `Saved ${name}.`;
  }
  for (const [id, update] of [['slide-save-preset', false], ['slide-update-preset', true]]) {
    $(id).onclick = async () => {
      $(id).disabled = true;
      try { await savePreset(update); } catch (error) { $('slide-setup-status').textContent = error.message; }
      finally { $(id).disabled = false; presetSelection(); }
    };
  }
  $('slide-delete-preset').onclick = async () => {
    const id = $('slide-preset').value;
    if (!id) return;
    try {
      await api(`/api/slideshow/presets/${id}`, undefined, 'DELETE');
      presets = presets.filter(p => String(p.id) !== id); renderPresets(); $('slide-setup-status').textContent = 'Preset deleted. Albums and photos are unchanged.';
    } catch (error) { $('slide-setup-status').textContent = error.message; }
  };
  $('slide-image').onerror = () => {
    if (dialog.open && slides.length) $('slide-status').textContent = 'This photo is unavailable. Use Next slide to continue.';
  };
  $('slide-previous').onclick = () => advance(-1);
  $('slide-next').onclick = () => advance(1);
  $('slide-pause').onclick = () => { playing = !playing; render(); };
  $('close-slideshow').onclick = () => dialog.close();
  $('slide-fullscreen').onclick = async () => {
    try {
      if (document.fullscreenElement === screen) await document.exitFullscreen();
      else if (screen.requestFullscreen) await screen.requestFullscreen();
      else throw new Error('Fullscreen is unavailable in this browser.');
    } catch (_) { $('slide-status').textContent = 'Fullscreen is unavailable. You can continue playing in this window.'; }
  };
  $('slide-exit-fullscreen').onclick = () => document.exitFullscreen().catch(report);
  document.addEventListener('fullscreenchange', () => {
    if (document.fullscreenElement === screen) $('slide-exit-fullscreen').focus();
    else if (dialog.open) $('slide-fullscreen').focus();
    if (!dialog.open && !document.fullscreenElement) $('start-slideshow').focus();
  });
  dialog.addEventListener('keydown', event => {
    if (event.target.closest('select, input, textarea')) return;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault(); advance(event.key === 'ArrowLeft' ? -1 : 1);
    } else if (event.code === 'Space' && (event.target === dialog || document.fullscreenElement === screen)) {
      event.preventDefault(); $('slide-pause').click();
    }
  });
  dialog.addEventListener('close', () => {
    ++generation; clearTimeout(timer); slides = [];
    transition?.cancel(); animateNext = false; clearTimeout(captionTimer); ++playlistVersion;
    $('slide-image').removeAttribute('src');
    if (document.fullscreenElement === screen) document.exitFullscreen().catch(() => {});
    document.body.classList.remove('slideshow-open');
    $('start-slideshow').focus();
  });
  document.addEventListener('visibilitychange', schedule);
})();
