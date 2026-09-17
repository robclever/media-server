'use strict';
(() => {
  let currentAlbum = null;
  let photos = [];
  let selected = 0;
  let photoFocus;
  let busy = false;
  let revision = 0;
  async function loadAlbums() {
    const version = ++revision;
    show('albums');
    const albums = await api('/api/albums');
    if (version !== revision || $('albums').hidden) return;
    $('album-list').replaceChildren();
    $('no-albums').hidden = albums.length > 0;
    for (const album of albums) {
      const button = document.createElement('button'); button.className = 'album-card';
      if (album.cover_id) {
        const image = document.createElement('img'); image.src = `/api/photos/${album.cover_id}/thumbnail`; image.alt = ''; image.loading = 'lazy'; button.append(image);
      } else {
        const placeholder = document.createElement('span'); placeholder.className = 'album-placeholder'; placeholder.textContent = '▧'; button.append(placeholder);
      }
      const title = document.createElement('h2'); title.textContent = album.name;
      const count = document.createElement('p'); count.textContent = `${album.count} photo${album.count === 1 ? '' : 's'}`;
      button.append(title, count); button.onclick = () => openAlbum(album).catch(report); $('album-list').append(button);
    }
  }
  async function openAlbum(album) {
    const version = ++revision;
    currentAlbum = album; photos = [];
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
    $('photo-title').textContent = photo.name; $('full-photo').alt = photo.name;
    $('full-photo').src = `/api/photos/${photo.id}/preview`;
    $('download-photo').href = `/api/photos/${photo.id}/original`;
    $('download-photo').download = photo.name;
    $('previous-photo').disabled = selected === 0; $('next-photo').disabled = selected === photos.length - 1;
  }
  $('photo-profile').onclick = async () => { try { await leaveParents(); await loadAlbums(); } catch (error) { report(error); } };
  $('back-albums').onclick = () => loadAlbums().catch(report);
  $('album-form').onsubmit = async event => {
    event.preventDefault(); $('create-album').disabled = true;
    try { const album = await api('/api/albums', {name: $('album-name').value}); $('album-name').value = ''; await openAlbum(album); }
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
          photos.push(await response.json()); saved++; renderPhotos();
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
  window.addEventListener('beforeunload', event => { if (busy) { event.preventDefault(); event.returnValue = ''; } });
})();
