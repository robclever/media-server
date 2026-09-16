'use strict';
const $ = id => document.getElementById(id);
let parent = false;
let collection = [];
let returnFocus;
async function api(path, data) {
  const response = await fetch(path, {method: data === undefined ? 'GET' : 'POST', headers: {'Content-Type': 'application/json', 'X-Requested-With': 'custom-plex'}, body: data === undefined ? undefined : JSON.stringify(data)});
  if (!response.ok) {
    if (response.status === 401) throw new Error('Please sign in with the parents password.');
    if (response.status === 429) throw new Error('Too many attempts. Please wait a minute.');
    if (response.status === 503) throw new Error('Set the parents password using the local set-password command first.');
    throw new Error(`Request failed (${response.status}). Please try again.`);
  }
  return response.status === 204 ? null : response.json();
}
function report(error) { $('status').textContent = error.message; }
function show(id) { for (const section of ['profiles','login','library']) $(section).hidden = section !== id; $('status').textContent = ''; }
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
    }
    $('movies').append(card);
  }
}
async function leaveParents() { await api('/api/logout', {}); parent = false; collection = []; $('movies').replaceChildren(); $('search').value = ''; }
$('baby').onclick = async () => { try { await leaveParents(); await load(); } catch(error) { report(error); } };
$('parents').onclick = async () => { try { const session = await api('/api/session'); if (session.parent) { parent = true; await load(); } else { show('login'); $('password').focus(); } } catch(error) { report(error); } };
$('login-form').onsubmit = async event => { event.preventDefault(); const password = $('password').value; $('password').value = ''; try { await api('/api/login', {password}); parent = true; await load(); } catch(error) { report(error); } };
$('cancel').onclick = () => { show('profiles'); $('parents').focus(); };
$('home').onclick = async () => { try { await leaveParents(); show('profiles'); $('baby').focus(); } catch(error) { report(error); } };
$('scan').onclick = async () => { $('scan').disabled = true; try { const count = await api('/api/scan', {}); await load(); $('status').textContent = `Library updated: ${count} movies.`; } catch(error) { report(error); } finally { $('scan').disabled = false; } };
$('search').oninput = render;
$('close-player').onclick = () => $('player').close();
$('player').addEventListener('close', () => { $('video').pause(); $('video').removeAttribute('src'); $('video').load(); returnFocus?.focus(); });
$('video').onerror = () => { $('playback-error').hidden = false; };
// Arrow keys move between controls for TV remotes; native video and inputs retain their keys.
document.addEventListener('keydown', event => {
  if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key) || ['INPUT','VIDEO'].includes(document.activeElement.tagName)) return;
  const scope = $('player').open ? $('player') : document;
  const controls = [...scope.querySelectorAll('button,a,input,video')].filter(el => el.getClientRects().length && !el.disabled);
  const index = controls.indexOf(document.activeElement);
  const step = ['ArrowLeft','ArrowUp'].includes(event.key) ? -1 : 1;
  if (controls.length) { event.preventDefault(); controls[(index + step + controls.length) % controls.length].focus(); }
});
$('baby').focus();
