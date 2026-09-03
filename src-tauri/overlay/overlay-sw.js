const CACHE_NAME = "overlay-shell-v1";
const SHELL_CACHE_KEY = "/__overlay_shell__";

self.addEventListener("install", (event) => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  const url = new URL(req.url);

  const isOverlayNavigation =
    req.mode === "navigate" &&
    url.origin === self.location.origin &&
    (url.pathname === "/overlay" || url.pathname.startsWith("/overlay/"));

  if (!isOverlayNavigation) return;

  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE_NAME);

      try {
        const fresh = await fetch(req);
        cache.put(SHELL_CACHE_KEY, fresh.clone());
        return fresh;
      } catch (err) {
        const cached = await cache.match(SHELL_CACHE_KEY);
        if (cached) return cached;

        return new Response(
          `<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Overlay</title>
  <style>
    html,body{
      margin:0;
      background:transparent;
      color:white;
      font-family:sans-serif;
      display:flex;
      align-items:center;
      justify-content:center;
      height:100%;
    }
  </style>
</head>
<body>
  Overlay cache is not ready yet. Start the bot once while OBS is open.
</body>
</html>`,
          {
            status: 503,
            headers: { "Content-Type": "text/html; charset=utf-8" },
          },
        );
      }
    })(),
  );
});