const CACHE = 'chart-v1';
const PRECACHE = ['/fdc3-plugin.js'];

self.addEventListener('install', (e) =>
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(PRECACHE)))
);

self.addEventListener('activate', (e) =>
  e.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
    )
  )
);

self.addEventListener('fetch', (e) => {
  const { url } = e.request;
  // Never intercept WebSocket upgrades, cross-origin requests, or HTML (always network for HTML)
  if (
    url.startsWith('ws') ||
    !url.startsWith(self.location.origin) ||
    e.request.destination === 'document'
  ) return;

  e.respondWith(caches.match(e.request).then((hit) => hit ?? fetch(e.request)));
});
