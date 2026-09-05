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
        // В кэш — только настоящая страница оверлея. Ответ «неверный ключ» или
        // «не найден» (в том числе от другого экземпляра бота на этом порту)
        // кэшировать нельзя: иначе он будет показываться вместо оверлея.
        if (fresh.ok) cache.put(SHELL_CACHE_KEY, fresh.clone());
        return fresh;
      } catch (err) {
        const cached = await cache.match(SHELL_CACHE_KEY);
        if (cached) return cached;

        // Кэша ещё нет (бот ни разу не запускался с этим адресом): отдаём
        // прозрачную страницу, которая сама перезагрузится, как только бот ответит.
        return new Response(
          `<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <title>Оверлей SignoreBot — ждём бота</title>
  <style>html,body{margin:0;background:transparent}</style>
</head>
<body>
  <!-- Кэш оверлея ещё не готов: запустите SignoreBot, страница обновится сама. -->
  <script>
    (function () {
      function poll() {
        fetch("/api/health", { cache: "no-store" })
          .then(function (r) { if (r.ok) location.reload(); })
          .catch(function () {});
      }
      setInterval(poll, 3000);
      window.addEventListener("online", poll);
    })();
  </script>
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