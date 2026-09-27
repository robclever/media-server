'use strict';
/** Homepage slideshow: fresh playlist per opening, one timer, and native fullscreen. */
(() => {
  const dialog = $('slideshow-player');
  const screen = $('slideshow-screen');
  let slides = [], index = 0, timer, playing = true, generation = 0;
  const preferences = { interval: '5', effect: 'none', duration: '600' };
  let transition, animateNext = false;
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
        if (key === 'interval') schedule();
        else { transition?.cancel(); animateNext = false; }
        $('slide-duration').disabled = preferences.effect === 'none';
      };
    }
    $('slide-duration').disabled = preferences.effect === 'none';
  }
  restorePreferences();
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
    for (const id of ['slide-previous', 'slide-next', 'slide-pause']) $(id).disabled = !slide;
    $('slide-pause').textContent = playing ? 'Pause slideshow' : 'Resume slideshow';
    if (slide) {
      $('slide-image').alt = slide.name;
      const source = `/api/photos/${slide.id}/preview`;
      if ($('slide-image').getAttribute('src') !== source) {
        transition?.cancel();
        animateNext = $('slide-image').hasAttribute('src');
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
    index = (index + direction + slides.length) % slides.length;
    render();
  }
  $('start-slideshow').onclick = async () => {
    const version = ++generation;
    slides = []; index = 0; playing = true;
    $('slide-caption').textContent = '';
    $('slide-status').textContent = 'Loading slideshow…';
    dialog.showModal(); render();
    try {
      const result = await api('/api/slideshow');
      if (version !== generation || !dialog.open) return;
      slides = result;
      if (!slides.length) $('slide-status').textContent = 'No photos selected. Open Photo Album and choose “Use in slideshow” on an album with photos.';
      render();
    } catch (error) {
      if (version === generation && dialog.open) $('slide-status').textContent = `Unable to load slideshow: ${error.message}`;
    }
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
    if (event.target.closest('select')) return;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault(); advance(event.key === 'ArrowLeft' ? -1 : 1);
    } else if (event.code === 'Space' && (event.target === dialog || document.fullscreenElement === screen)) {
      event.preventDefault(); $('slide-pause').click();
    }
  });
  dialog.addEventListener('close', () => {
    ++generation; clearTimeout(timer); slides = [];
    transition?.cancel(); animateNext = false;
    $('slide-image').removeAttribute('src');
    if (document.fullscreenElement === screen) document.exitFullscreen().catch(() => {});
    $('start-slideshow').focus();
  });
  document.addEventListener('visibilitychange', schedule);
})();
