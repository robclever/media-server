'use strict';
(() => {
  let currentAlbum = null;
  let photos = [];
  let selected = 0;
  let photoFocus;
  let busy = false;
  let revision = 0;

  function rememberAlbum(id) {
    try { localStorage.setItem('photo-last-album', String(id)); } catch (_) {}
  }

  function lastAlbum() {
    try { return Number(localStorage.getItem('photo-last-album')) || 0; } catch (_) { return 0; }
  }

  async function renameAlbum(album) {
    const name = await askName('Rename album', album.name);
    if (name === null || name.trim() === album.name) return;
    await api(`/api/albums/${album.id}/name`, {name});
    album.name = name.trim();
    await loadAlbums();
    $('status').textContent = `Album renamed to ${album.name}.`;
  }

  async function deleteAlbum(album) {
    const contents = album.count === 1 ? '1 photo' : `${album.count} photos`;
    if (!await askDelete(`Delete “${album.name}” and ${contents}? This permanently removes the original photos and cannot be undone.`)) return;
    await api(`/api/albums/${album.id}`, undefined, 'DELETE');
    await loadAlbums();
    $('status').textContent = `Deleted ${album.name}.`;
  }

  async function loadAlbums() {
    const version = ++revision;
    show('albums');
    const albums = await api('/api/albums');
    if (version !== revision || $('albums').hidden) return;
    $('album-list').replaceChildren();
    $('no-albums').hidden = albums.length > 0;
    for (const album of albums) {
      const card = document.createElement('article'); card.className = 'movie album-card';
      const open = document.createElement('button'); open.className = 'album-open'; open.setAttribute('aria-label', `Open ${album.name}`);
      if (album.cover_id) {
        const image = document.createElement('img'); image.src = `/api/photos/${album.cover_id}/thumbnail`; image.alt = ''; image.loading = 'lazy'; open.append(image);
      } else {
        const placeholder = document.createElement('span'); placeholder.className = 'album-placeholder'; placeholder.textContent = '▧'; open.append(placeholder);
      }
      const title = document.createElement('h2'); title.textContent = album.name;
      const count = document.createElement('p'); count.textContent = `${album.count} photo${album.count === 1 ? '' : 's'}`;
      open.append(title, count); open.onclick = () => openAlbum(album).catch(report);
      const actions = document.createElement('div'); actions.className = 'icon-actions compact';
      const rename = document.createElement('button'); rename.title = 'Rename album'; rename.setAttribute('aria-label', `Rename ${album.name}`); rename.innerHTML = '✎<span>Rename</span>';
      rename.onclick = () => renameAlbum(album).catch(report);
      const remove = document.createElement('button'); remove.className = 'danger'; remove.title = 'Delete album'; remove.setAttribute('aria-label', `Delete ${album.name}`); remove.innerHTML = '🗑<span>Delete</span>';
      remove.onclick = () => deleteAlbum(album).catch(report);
      actions.append(rename, remove); card.append(open, actions); $('album-list').append(card);
    }
  }

  async function openAlbum(album) {
    const version = ++revision;
    currentAlbum = album; photos = []; rememberAlbum(album.id);
    $('album-title').textContent = album.name; $('photo-list').replaceChildren();
    $('photo-files').value = ''; $('upload-progress').textContent = ''; show('album-detail');
    const result = await api(`/api/albums/${album.id}/photos`);
    if (version !== revision || $('album-detail').hidden) return;
    photos = result; renderPhotos();
  }

  function renderPhotos() {
    $('photo-list').replaceChildren(); $('no-photos').hidden = photos.length > 0;
    photos.forEach((photo, index) => {
      const button = document.createElement('button'); button.className = 'photo-tile';
      const image = document.createElement('img'); image.src = `/api/photos/${photo.id}/thumbnail`; image.alt = ''; image.loading = 'lazy';
      const label = document.createElement('span'); label.textContent = photo.name; button.append(image, label);
      button.onclick = () => { selected = index; photoFocus = button; displayPhoto(); $('photo-viewer').showModal(); $('close-photo').focus(); };
      $('photo-list').append(button);
    });
  }

  function displayPhoto() {
    const photo = photos[selected];
    if (!photo) { $('photo-viewer').close(); return; }
    $('photo-title').textContent = photo.name; $('full-photo').alt = photo.name;
    $('full-photo').src = `/api/photos/${photo.id}/preview`;
    $('download-photo').href = `/api/photos/${photo.id}/original`;
    $('download-photo').download = photo.name;
    $('previous-photo').disabled = selected === 0; $('next-photo').disabled = selected === photos.length - 1;
  }

  async function renamePhoto() {
    const photo = photos[selected];
    $('photo-viewer').close();
    const name = await askName('Rename photo', photo.name);
    if (name === null || name.trim() === photo.name) { displayPhoto(); $('photo-viewer').showModal(); return; }
    await api(`/api/photos/${photo.id}/name`, {name});
    photo.name = name.trim(); displayPhoto(); renderPhotos();
    $('status').textContent = `Photo renamed to ${photo.name}.`;
    $('photo-viewer').showModal();
  }

  async function openMoveMenu() {
    const photo = photos[selected];
    const albums = await api('/api/albums');
    const recent = lastAlbum();
    albums.sort((a, b) => Number(b.id === recent) - Number(a.id === recent));
    $('album-destinations').replaceChildren();
    for (const album of albums.filter(album => album.id !== currentAlbum.id)) {
      const option = document.createElement('button'); option.textContent = album.name;
      option.onclick = async () => {
        option.disabled = true;
        try {
          await api(`/api/photos/${photo.id}/album`, {album_id: album.id});
          rememberAlbum(album.id); photos.splice(selected, 1); renderPhotos(); $('move-dialog').close();
          $('status').textContent = `Moved ${photo.name} to ${album.name}.`;
        } catch (error) { option.disabled = false; report(error); }
      };
      $('album-destinations').append(option);
    }
    if (!$('album-destinations').children.length) {
      const message = document.createElement('p'); message.textContent = 'Create another album before moving this photo.'; $('album-destinations').append(message);
    }
    $('photo-viewer').close(); $('move-dialog').showModal();
  }

  async function deletePhoto() {
    const photo = photos[selected];
    $('photo-viewer').close();
    if (!await askDelete(`Delete “${photo.name}”? This permanently removes the original photo and cannot be undone.`)) { displayPhoto(); $('photo-viewer').showModal(); return; }
    await api(`/api/photos/${photo.id}`, undefined, 'DELETE');
    photos.splice(selected, 1);
    $('photo-viewer').close(); renderPhotos();
    $('status').textContent = `Deleted ${photo.name}.`;
  }

  $('photo-profile').onclick = async () => { try { await leaveParents(); await loadAlbums(); } catch (error) { report(error); } };
  $('back-albums').onclick = () => loadAlbums().catch(report);
  $('album-form').onsubmit = async event => {
    event.preventDefault(); $('create-album').disabled = true;
    try { const album = await api('/api/albums', {name: $('album-name').value}); $('album-name').value = ''; rememberAlbum(album.id); await openAlbum(album); }
    catch (error) { report(error); } finally { $('create-album').disabled = false; }
  };
  $('upload-photos').onclick = async () => {
    if (busy || !currentAlbum) return;
    const files = [...$('photo-files').files];
    if (!files.length) { $('upload-progress').textContent = 'Choose photos to upload first.'; return; }
    busy = true;
    const album = currentAlbum; const failures = []; let saved = 0;
    for (const id of ['upload-photos','photo-files','back-albums','home']) $(id).disabled = true;
    try {
      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        $('upload-progress').textContent = `Uploading ${i + 1} of ${files.length}: ${file.name}`;
        try {
          if (file.size > 24 * 1024 * 1024) throw new Error('larger than 24 MiB');
          const response = await fetch(`/api/albums/${album.id}/photos?name=${encodeURIComponent(file.name)}`, {method:'POST', headers:{'Content-Type':file.type || 'application/octet-stream','X-Requested-With':'custom-plex'},body:file});
          if (!response.ok) throw new Error(response.status === 415 ? 'unsupported image; export as JPEG, PNG, or WebP, at most 8192 pixels per side' : response.status === 413 ? 'larger than 24 MiB' : `upload failed (${response.status}); check photo storage space and permissions`);
          photos.push(await response.json()); saved++; rememberAlbum(album.id); renderPhotos();
        } catch (error) { failures.push(`${file.name}: ${error.message}`); }
      }
      $('photo-files').value = '';
      $('upload-progress').textContent = `${saved} of ${files.length} photos saved.${failures.length ? '\n' + failures.join('\n') : ''}`;
    } finally {
      busy = false; for (const id of ['upload-photos','photo-files','back-albums','home']) $(id).disabled = false;
    }
  };
  $('close-photo').onclick = () => $('photo-viewer').close();
  $('photo-viewer').addEventListener('close', () => { $('full-photo').removeAttribute('src'); photoFocus?.focus(); });
  $('previous-photo').onclick = () => { if (selected > 0) { selected--; displayPhoto(); } };
  $('next-photo').onclick = () => { if (selected + 1 < photos.length) { selected++; displayPhoto(); } };
  $('rename-photo').onclick = () => renamePhoto().catch(report);
  $('move-photo').onclick = () => openMoveMenu().catch(report);
  $('delete-photo').onclick = () => deletePhoto().catch(report);
  $('cancel-move').onclick = () => { $('move-dialog').close(); if (photos[selected]) { displayPhoto(); $('photo-viewer').showModal(); } };
  window.addEventListener('beforeunload', event => { if (busy) { event.preventDefault(); event.returnValue = ''; } });
})();
