const CACHE = 'ticker-v4';
const PLUGIN_URL = new URL('./fdc3-plugin.js', self.location).href;

// `cache: 'reload'` skips the HTTP cache, which may still hold a copy that an
// older server marked immutable.
const fetchPlugin = () => fetch(PLUGIN_URL, { cache: 'reload' });

self.addEventListener('install', (e) => {
  self.skipWaiting();
  e.waitUntil(
    caches.open(CACHE).then((c) => fetchPlugin().then((res) => c.put(PLUGIN_URL, res)))
  );
});

self.addEventListener('activate', (e) =>
  e.waitUntil(
    caches.keys()
      .then((keys) =>
        Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
      )
      .then(() => self.clients.claim())
  )
);

// The plugin's name has no content hash, so the network wins and the cache is
// only the offline fallback. Everything else goes straight to the network.
self.addEventListener('fetch', (e) => {
  if (e.request.url !== PLUGIN_URL) return;

  e.respondWith(
    fetchPlugin()
      .then(async (res) => {
        if (res.ok) await (await caches.open(CACHE)).put(PLUGIN_URL, res.clone());
        return res;
      })
      .catch(() => caches.match(PLUGIN_URL))
  );
});
