'use strict';
const $ = id => document.getElementById(id);
let parent = false;
let collection = [];
let returnFocus;
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
function report(error) { $('status').textContent = error.message; }
function show(id) { for (const section of ['profiles','login','library','albums','album-detail']) $(section).hidden = section !== id; $('status').textContent = ''; }
let editResolution;
function askName(heading, value) {
  return new Promise(resolve => {
    editResolution = resolve; $('edit-heading').textContent = heading; $('edit-name').value = value;
    $('edit-dialog').showModal(); $('edit-name').select();
  });
}
function finishEdit(value) {
  const resolve = editResolution; editResolution = null; $('edit-dialog').close(); resolve?.(value);
}
$('edit-form').onsubmit = event => { event.preventDefault(); finishEdit($('edit-name').value); };
$('cancel-edit').onclick = () => finishEdit(null);
$('edit-dialog').addEventListener('cancel', event => { event.preventDefault(); finishEdit(null); });
let confirmResolution;
function askDelete(message) {
  return new Promise(resolve => {
    confirmResolution = resolve; $('confirm-message').textContent = message;
    $('confirm-dialog').showModal(); $('cancel-confirm').focus();
  });
}
function finishConfirm(value) {
  const resolve = confirmResolution; confirmResolution = null; $('confirm-dialog').close(); resolve?.(value);
}
$('cancel-confirm').onclick = () => finishConfirm(false);
$('accept-confirm').onclick = () => finishConfirm(true);
$('confirm-dialog').addEventListener('cancel', event => { event.preventDefault(); finishConfirm(false); });
function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes < 0) return 'Unavailable';
  const units = ['B','KiB','MiB','GiB','TiB']; let unit = 0;
  while (bytes >= 1024 && unit < units.length - 1) { bytes /= 1024; unit++; }
  return `${bytes.toFixed(unit < 2 ? 0 : 1)} ${units[unit]}`;
}
async function loadStorage() {
  try {
    const storage = await api('/api/storage');
    const used = storage.total - storage.available;
    const percent = storage.total ? Math.round(used / storage.total * 100) : 0;
    const heading = document.createElement('strong'); heading.textContent = `Media storage · ${percent}% used`;
    const meter = document.createElement('progress'); meter.max = storage.total || 1; meter.value = used; meter.setAttribute('aria-label', 'Media storage used');
    const summary = document.createElement('p'); summary.textContent = `${formatBytes(used)} used · ${formatBytes(storage.available)} available · ${formatBytes(storage.total)} total`;
    const details = document.createElement('small'); details.textContent = storage.volumes.map(volume => `${volume.name}: ${formatBytes(volume.total - volume.available)} of ${formatBytes(volume.total)}`).join(' · ');
    $('storage').replaceChildren(heading, meter, summary, details);
  } catch (_) { $('storage').replaceChildren(Object.assign(document.createElement('p'), {textContent:'Storage usage is unavailable.'})); }
}
async function load() {
  collection = await api('/api/movies');
  $('profile-label').textContent = parent ? 'PARENTS · THE FULL COLLECTION' : 'BABY · LITTLE FAVORITES';
  $('scan').hidden = !parent;
  show('library'); render(); $('search').focus();
}
function render() {
  const movies = collection.filter(movie => movie.title.toLowerCase().includes($('search').value.toLowerCase()));
  $('movies').replaceChildren(); $('empty').hidden = movies.length > 0;
  $('empty').textContent = $('search').value ? 'No movies match your search.' : parent ? 'Add video files to your media folder, then scan the library.' : 'Your little movie shelf is waiting. Ask a parent to approve some favorites.';
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
    play.onclick = () => { returnFocus = play; $('playing-title').textContent = movie.title; $('playback-error').hidden = true; $('video').src = `/media/${movie.id}`; $('player').showModal(); $('video').focus(); $('video').play().catch(() => {}); };
    card.append(play);
    if (parent) {
      const location = document.createElement('p'); location.className = 'source';
      location.textContent = `Location: ${movie.source}`; card.append(location);
    }
    if (parent) {
      const approval = document.createElement('button'); approval.className = 'approval'; approval.textContent = movie.approved ? '✓ Available to Baby' : '+ Add to Baby'; approval.setAttribute('aria-pressed', String(movie.approved));
      approval.onclick = async () => { try { await api(`/api/movies/${movie.id}/approval`, {approved: !movie.approved}); movie.approved = !movie.approved; approval.textContent = movie.approved ? '✓ Available to Baby' : '+ Add to Baby'; approval.setAttribute('aria-pressed', String(movie.approved)); } catch(error) { report(error); } };
      card.append(approval);
      const rename = document.createElement('button'); rename.className = 'approval'; rename.textContent = '✎ Rename title';
      rename.onclick = async () => {
        const name = await askName('Rename movie', movie.title);
        if (name === null || name.trim() === movie.title) return;
        try {
          await api(`/api/movies/${movie.id}/title`, {name});
          movie.title = name.trim(); render(); $('status').textContent = `Renamed to ${movie.title}.`;
        } catch (error) { report(error); }
      };
      card.append(rename);
    }
    const imageActions = document.createElement('div'); imageActions.className = 'image-actions';
    const choose = document.createElement('button'); choose.textContent = 'Add / change image';
    choose.setAttribute('aria-label', `Add or change image for ${movie.title}`);
    const automatic = document.createElement('button'); automatic.textContent = 'Use automatic image';
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
async function leaveParents() { await api('/api/logout', {}); parent = false; collection = []; $('movies').replaceChildren(); $('search').value = ''; }
$('baby').onclick = async () => { try { await leaveParents(); await load(); } catch(error) { report(error); } };
$('parents').onclick = async () => { try { const session = await api('/api/session'); if (session.parent) { parent = true; await load(); } else { show('login'); $('password').focus(); } } catch(error) { report(error); } };
$('login-form').onsubmit = async event => { event.preventDefault(); const password = $('password').value; $('password').value = ''; try { await api('/api/login', {password}); parent = true; await load(); } catch(error) { report(error); } };
$('cancel').onclick = () => { show('profiles'); $('parents').focus(); };
$('home').onclick = async () => { try { await leaveParents(); show('profiles'); loadStorage(); $('baby').focus(); } catch(error) { report(error); } };
$('scan').onclick = async () => { $('scan').disabled = true; try { const count = await api('/api/scan', {}); await load(); $('status').textContent = `Library updated: ${count} movies.`; } catch(error) { report(error); } finally { $('scan').disabled = false; } };
$('search').oninput = render;
$('close-player').onclick = () => $('player').close();
$('player').addEventListener('close', () => { $('video').pause(); $('video').removeAttribute('src'); $('video').load(); returnFocus?.focus(); });
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
