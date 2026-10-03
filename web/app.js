'use strict';
/**
 * Shared browser controller for profiles, authentication, movies, covers,
 * playback progress, storage reporting, and reusable dialogs.
 *
 * This file loads before photos.js. The photo controller intentionally reuses
 * `$`, `api`, `show`, `askName`, `askDescription`, `askDelete`, and
 * `leaveParents` from this script.
 */
const $ = id => document.getElementById(id);
let parent = false;
let collection = [];
let selectedLetter = 'All';
const titleOrder = new Intl.Collator('en', {sensitivity: 'base', numeric: true});
/** Groups accented Latin initials with their base letter; other initials use #. */
function movieLetter(title) {
  const initial = title.trim().normalize('NFD').replace(/[\u0300-\u036f]/g, '').charAt(0).toUpperCase();
  return /^[A-Z]$/.test(initial) ? initial : '#';
}
for (const letter of ['All', ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ', '#']) {
  const button = document.createElement('button');
  button.textContent = letter; button.type = 'button';
  button.setAttribute('aria-controls', 'movies');
  button.setAttribute('aria-pressed', String(letter === selectedLetter));
  if (letter === '#') button.setAttribute('aria-label', 'Numbers and other titles');
  button.onclick = () => { selectedLetter = letter; render(); };
  button.onfocus = () => button.scrollIntoView({block: 'nearest', inline: 'nearest'});
  $('movie-alphabet').append(button);
}
let returnFocus;
let currentMovie = null;
let lastSavedAt = 0;
/** Sends a same-origin API request and converts known status codes to useful UI errors. */
async function api(path, data, method) {
  const response = await fetch(path, {method: method || (data === undefined ? 'GET' : 'POST'), headers: {'Content-Type': 'application/json', 'X-Requested-With': 'custom-plex'}, body: data === undefined ? undefined : JSON.stringify(data)});
  if (!response.ok) {
    if (response.status === 401) throw new Error('Please sign in with the parents password.');
    if (response.status === 429) throw new Error('Too many attempts. Please wait a minute.');
    if (response.status === 503) throw new Error('Set the parents password using the local set-password command first.');
    throw new Error(`Request failed (${response.status}). Please try again.`);
  }
  return response.status === 204 ? null : response.json();
}
/** Announces an operation failure in the page-level live region. */
function report(error) { $('status').textContent = error.message; }
/** Shows one primary screen, hides the others, and clears stale status text. */
function show(id) { for (const section of ['profiles','login','library','albums','album-detail']) $(section).hidden = section !== id; $('status').textContent = ''; }
let editResolution;
/** Opens the reusable single-line editor and resolves with text or null. */
function askName(heading, value) {
  return new Promise(resolve => {
    editResolution = resolve; $('edit-heading').textContent = heading; $('edit-name').value = value;
    $('edit-dialog').showModal(); $('edit-name').select();
  });
}
/** Closes and resolves the active name editor. */
function finishEdit(value) {
  const resolve = editResolution; editResolution = null; $('edit-dialog').close(); resolve?.(value);
}
$('edit-form').onsubmit = event => { event.preventDefault(); finishEdit($('edit-name').value); };
$('cancel-edit').onclick = () => finishEdit(null);
$('edit-dialog').addEventListener('cancel', event => { event.preventDefault(); finishEdit(null); });
let descriptionResolution;
/** Opens the reusable multiline album-description editor. */
function askDescription(heading, value) {
  return new Promise(resolve => {
    descriptionResolution = resolve; $('description-heading').textContent = heading; $('description-text').value = value || '';
    $('description-dialog').showModal(); $('description-text').focus();
  });
}
/** Closes and resolves the active description editor. */
function finishDescription(value) {
  const resolve = descriptionResolution; descriptionResolution = null; $('description-dialog').close(); resolve?.(value);
}
$('description-form').onsubmit = event => { event.preventDefault(); finishDescription($('description-text').value); };
$('cancel-description').onclick = () => finishDescription(null);
$('description-dialog').addEventListener('cancel', event => { event.preventDefault(); finishDescription(null); });
let confirmResolution;
/** Opens the destructive-action confirmation dialog. */
function askDelete(message) {
  return new Promise(resolve => {
    confirmResolution = resolve; $('confirm-message').textContent = message;
    $('confirm-dialog').showModal(); $('cancel-confirm').focus();
  });
}
/** Closes and resolves the active deletion confirmation. */
function finishConfirm(value) {
  const resolve = confirmResolution; confirmResolution = null; $('confirm-dialog').close(); resolve?.(value);
}
$('cancel-confirm').onclick = () => finishConfirm(false);
$('accept-confirm').onclick = () => finishConfirm(true);
$('confirm-dialog').addEventListener('cancel', event => { event.preventDefault(); finishConfirm(false); });
/** Formats a nonnegative byte count for the storage meter. */
function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes < 0) return 'Unavailable';
  const units = ['B','KiB','MiB','GiB','TiB']; let unit = 0;
  while (bytes >= 1024 && unit < units.length - 1) { bytes /= 1024; unit++; }
  return `${bytes.toFixed(unit < 2 ? 0 : 1)} ${units[unit]}`;
}
/** Loads managed-file sizes and whole-filesystem capacity without blocking the home screen. */
async function loadStorage() {
  try {
    const storage = await api('/api/storage');
    const used = storage.total - storage.available;
    const percent = storage.total ? Math.round(used / storage.total * 100) : 0;
    const heading = document.createElement('strong'); heading.textContent = `Custom Plex files · ${formatBytes(storage.managed)}`;
    const managed = document.createElement('p'); managed.textContent = storage.locations.map(location => `${location.name}: ${formatBytes(location.bytes)}`).join(' · ');
    const meter = document.createElement('progress'); meter.max = storage.total || 1; meter.value = used; meter.setAttribute('aria-label', 'Configured storage devices used by all files');
    const capacity = document.createElement('p'); capacity.textContent = `Configured devices · ${percent}% full · ${formatBytes(storage.available)} available`;
    const details = document.createElement('small'); details.textContent = `Device usage includes files outside Custom Plex. ${storage.volumes.map(volume => `${volume.name}: ${formatBytes(volume.available)} free of ${formatBytes(volume.total)}`).join(' · ')}`;
    $('storage').replaceChildren(heading, managed, meter, capacity, details);
  } catch (_) { $('storage').replaceChildren(Object.assign(document.createElement('p'), {textContent:'Storage usage is unavailable.'})); }
}
/** Loads the movie list for the selected profile and opens the library screen. */
async function load() {
  collection = await api('/api/movies');
  $('profile-label').textContent = parent ? 'PARENTS · THE FULL COLLECTION' : 'BABY · LITTLE FAVORITES';
  $('scan').hidden = !parent;
  show('library'); render(); $('search').focus();
}
/** Rebuilds filtered movie cards and binds their profile-dependent actions. */
function render() {
  const query = $('search').value.trim().toLowerCase();
  const movies = collection.filter(movie =>
    (selectedLetter === 'All' || movieLetter(movie.title) === selectedLetter) &&
    movie.title.toLowerCase().includes(query)
  ).sort((a, b) => titleOrder.compare(a.title.trim(), b.title.trim()));
  for (const button of $('movie-alphabet').children) {
    button.setAttribute('aria-pressed', String(button.textContent === selectedLetter));
  }
  $('movie-count').textContent = `${movies.length} ${movies.length === 1 ? 'movie' : 'movies'} · ${selectedLetter === 'All' ? 'All titles · A–Z' : selectedLetter === '#' ? 'Numbers & other titles' : `Starting with ${selectedLetter}`}`;
  $('movies').replaceChildren(); $('empty').hidden = movies.length > 0;
  $('empty').textContent = query || selectedLetter !== 'All' ? 'No movies match these filters. Choose All or clear your search.' : parent ? 'Add video files to your media folder, then scan the library.' : 'Your little movie shelf is waiting. Ask a parent to approve some favorites.';
  for (const movie of movies) {
    const card = document.createElement('article'); card.className = 'movie';
    const play = document.createElement('button'); play.className = 'play';
    const cover = document.createElement('span'); cover.className = 'cover'; cover.textContent = '▶'; cover.setAttribute('aria-hidden','true');
    function refreshCover() {
      const picture = document.createElement('img'); picture.alt = ''; picture.loading = 'lazy';
      picture.onload = () => cover.replaceChildren(picture);
      picture.onerror = () => { cover.textContent = '▶'; };
      picture.src = `/api/movies/${movie.id}/cover?v=${Date.now()}`;
      // Attach immediately so native lazy loading can observe the image's position.
      cover.replaceChildren(picture);
    }
    refreshCover();
    const title = document.createElement('h2'); title.textContent = movie.title;
    play.append(cover, title); play.setAttribute('aria-label', `Play ${movie.title}`);
    const resumeAt = Number(movie.position) || 0;
    if (resumeAt > 1) {
      const resume = document.createElement('p'); resume.className = 'resume'; resume.textContent = `Resume at ${formatTime(resumeAt)}`; play.append(resume);
    }
    play.onclick = () => {
      returnFocus = play; currentMovie = movie; lastSavedAt = resumeAt;
      $('playing-title').textContent = movie.title; $('resume-message').textContent = resumeAt > 1 ? `Resuming from ${formatTime(resumeAt)}` : '';
      $('playback-error').hidden = true; $('video').src = `/media/${movie.id}`; $('player').showModal(); $('video').focus(); $('video').play().catch(() => {});
    };
    card.append(play);
    if (parent) {
      const location = document.createElement('p'); location.className = 'source';
      location.textContent = `Location: ${movie.source}`; card.append(location);
    }
    const imageActions = document.createElement('div'); imageActions.className = 'icon-actions compact movie-actions';
    if (parent) {
      const approval = document.createElement('button'); approval.className = 'icon-button'; approval.textContent = movie.approved ? '★' : '☆'; approval.setAttribute('aria-pressed', String(movie.approved));
      const updateApprovalLabel = () => { const label = movie.approved ? 'Remove from Baby profile' : 'Add to Baby profile'; approval.dataset.tooltip = label; approval.setAttribute('aria-label', label); };
      updateApprovalLabel();
      approval.onclick = async () => { try { await api(`/api/movies/${movie.id}/approval`, {approved: !movie.approved}); movie.approved = !movie.approved; approval.textContent = movie.approved ? '★' : '☆'; approval.setAttribute('aria-pressed', String(movie.approved)); updateApprovalLabel(); } catch(error) { report(error); } };
      imageActions.append(approval);
      const rename = document.createElement('button'); rename.className = 'icon-button'; rename.textContent = '✎'; rename.dataset.tooltip = 'Rename movie'; rename.setAttribute('aria-label', `Rename ${movie.title}`);
      rename.onclick = async () => {
        const name = await askName('Rename movie', movie.title);
        if (name === null || name.trim() === movie.title) return;
        try {
          await api(`/api/movies/${movie.id}/title`, {name});
          movie.title = name.trim(); render(); $('status').textContent = `Renamed to ${movie.title}.`;
        } catch (error) { report(error); }
      };
      imageActions.append(rename);
    }
    const choose = document.createElement('button'); choose.className = 'icon-button'; choose.textContent = '▧'; choose.dataset.tooltip = 'Add or change image';
    choose.setAttribute('aria-label', `Add or change image for ${movie.title}`);
    const automatic = document.createElement('button'); automatic.className = 'icon-button'; automatic.textContent = '↻'; automatic.dataset.tooltip = 'Use automatic image';
    automatic.setAttribute('aria-label', `Use automatic image for ${movie.title}`);
    const picker = document.createElement('input'); picker.type = 'file'; picker.accept = 'image/jpeg,image/png,image/webp'; picker.hidden = true;
    choose.onclick = () => picker.click();
    async function changeCover(file) {
      if (file && file.size > 8 * 1024 * 1024) { report(new Error('Choose an image smaller than 8 MiB.')); return; }
      choose.disabled = automatic.disabled = true;
      $('status').textContent = file ? `Saving image for ${movie.title}…` : `Finding an automatic image for ${movie.title}…`;
      try {
        const result = await fetch(`/api/movies/${movie.id}/cover`, {
          method: file ? 'POST' : 'DELETE', headers: {'X-Requested-With': 'custom-plex', ...(file ? {'Content-Type': file.type || 'application/octet-stream'} : {})},
          body: file || undefined
        });
        if (!result.ok) {
          if (result.status === 415) throw new Error('Use a valid JPEG, PNG, or WebP image, at most 8192 pixels per side.');
          if (result.status === 413) throw new Error('Choose an image smaller than 8 MiB.');
          throw new Error(`Could not change the image (${result.status}). Refresh the library and try again.`);
        }
        refreshCover();
        $('status').textContent = file ? `Image saved for ${movie.title}.` : `Automatic image selected for ${movie.title}. A placeholder remains if no frame is available.`;
      } catch (error) { report(error); }
      finally { choose.disabled = automatic.disabled = false; picker.value = ''; }
    }
    picker.onchange = () => { if (picker.files[0]) changeCover(picker.files[0]); };
    automatic.onclick = () => changeCover(null);
    imageActions.append(choose, automatic, picker); card.append(imageActions);
    $('movies').append(card);
  }
}
/** Formats a playback position as M:SS or H:MM:SS. */
function formatTime(seconds) {
  const total = Math.max(0, Math.floor(seconds)); const hours = Math.floor(total / 3600); const minutes = Math.floor(total % 3600 / 60); const rest = total % 60;
  return hours ? `${hours}:${String(minutes).padStart(2,'0')}:${String(rest).padStart(2,'0')}` : `${minutes}:${String(rest).padStart(2,'0')}`;
}
/**
 * Saves the active video's shared position.
 * Routine timeupdate events are throttled to ten-second movement; pause, close,
 * end, and background events force a save.
 */
async function saveProgress(force = false) {
  const video = $('video'); const movie = currentMovie;
  if (!movie || !Number.isFinite(video.duration) || video.duration <= 0 || !Number.isFinite(video.currentTime)) return;
  if (!force && Math.abs(video.currentTime - lastSavedAt) < 10) return;
  lastSavedAt = video.currentTime; movie.position = video.currentTime; movie.duration = video.duration;
  try { await api(`/api/movies/${movie.id}/progress`, {position: video.ended ? 0 : video.currentTime, duration: video.duration}); if (video.ended) movie.position = null; }
  catch (error) { if (force) report(error); }
}
/** Logs out Parents and clears profile-specific movie state from the page. */
async function leaveParents() { await api('/api/logout', {}); parent = false; collection = []; $('movies').replaceChildren(); $('search').value = ''; selectedLetter = 'All'; $('movie-alphabet').scrollLeft = 0; }
$('baby').onclick = async () => { try { await leaveParents(); await load(); } catch(error) { report(error); } };
$('parents').onclick = async () => { try { const session = await api('/api/session'); if (session.parent) { parent = true; await load(); } else { show('login'); $('password').focus(); } } catch(error) { report(error); } };
$('login-form').onsubmit = async event => { event.preventDefault(); const password = $('password').value; $('password').value = ''; try { await api('/api/login', {password}); parent = true; await load(); } catch(error) { report(error); } };
$('cancel').onclick = () => { show('profiles'); $('parents').focus(); };
$('home').onclick = async () => { try { await leaveParents(); show('profiles'); loadStorage(); $('baby').focus(); } catch(error) { report(error); } };
$('scan').onclick = async () => { $('scan').disabled = true; try { const count = await api('/api/scan', {}); await load(); $('status').textContent = `Library updated: ${count} movies.`; } catch(error) { report(error); } finally { $('scan').disabled = false; } };
$('search').oninput = render;
$('close-player').onclick = async () => { await saveProgress(true); $('player').close(); };
$('player').addEventListener('close', () => { const video = $('video'); video.pause(); video.removeAttribute('src'); video.load(); currentMovie = null; returnFocus?.focus(); render(); });
$('video').addEventListener('loadedmetadata', () => { const position = Number(currentMovie?.position) || 0; if (position > 1 && position < $('video').duration - 15) $('video').currentTime = position; });
$('video').addEventListener('timeupdate', () => saveProgress());
$('video').addEventListener('pause', () => saveProgress(true));
$('video').addEventListener('ended', () => saveProgress(true));
document.addEventListener('visibilitychange', () => { if (document.hidden) saveProgress(true); });
$('video').onerror = () => { $('playback-error').hidden = false; };
// Arrow keys move between controls for TV remotes; native video and inputs retain their keys.
document.addEventListener('keydown', event => {
  if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key) || ['INPUT','VIDEO'].includes(document.activeElement.tagName)) return;
  const scope = $('player').open ? $('player') : $('photo-viewer').open ? $('photo-viewer') : document;
  const controls = [...scope.querySelectorAll('button,a,input,video')].filter(el => el.getClientRects().length && !el.disabled);
  const index = controls.indexOf(document.activeElement);
  const step = ['ArrowLeft','ArrowUp'].includes(event.key) ? -1 : 1;
  if (controls.length) { event.preventDefault(); controls[(index + step + controls.length) % controls.length].focus(); }
});
$('baby').focus();
loadStorage();
