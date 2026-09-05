// Локальный просмотр собранного сайта: раздаёт docs/ так же, как GitHub Pages.
//
// Запуск: npm run site:serve            (http://127.0.0.1:8765/)
//         npm run site:serve -- 4000    (другой порт)
// Сайт перед этим нужно собрать: npm run site.
//
// Зачем сервер, если это статические файлы: открытые через file:// адреса
// вида /start/ не работают, а маршрутизатор из docs/app.js должен получать
// настоящие ответы сервера — как на GitHub Pages. Каталог без слэша на конце
// отдаётся редиректом, иначе относительные ссылки внутри страницы разъедутся.
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { dirname, extname, join, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "docs");
const PORT = Number(process.argv[2] || process.env.PORT || 8765);

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".ico": "image/x-icon",
  ".ttf": "font/ttf",
  ".woff2": "font/woff2",
  ".txt": "text/plain; charset=utf-8",
};

function send(res, code, body, type = "text/plain; charset=utf-8") {
  // no-store: правки видны сразу после пересборки, без Ctrl+F5
  res.writeHead(code, { "Content-Type": type, "Cache-Control": "no-store" });
  res.end(body);
}

const server = createServer(async (req, res) => {
  let pathname;
  try {
    pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
  } catch {
    return send(res, 400, "400 — некорректный адрес");
  }
  // никуда за пределы docs/
  const target = resolve(ROOT, "." + normalize(pathname));
  if (target !== ROOT && !target.startsWith(ROOT + sep)) {
    return send(res, 403, "403 — за пределами docs/");
  }

  let file = target;
  const info = await stat(file).catch(() => null);
  if (info?.isDirectory()) {
    if (!pathname.endsWith("/")) {
      res.writeHead(301, { Location: pathname + "/", "Cache-Control": "no-store" });
      return res.end();
    }
    file = join(file, "index.html");
  }

  const body = await readFile(file).catch(() => null);
  if (!body) return send(res, 404, `404 — нет «${pathname}»\n\nСайт собирается командой: npm run site`);
  send(res, 200, body, TYPES[extname(file).toLowerCase()] || "application/octet-stream");
});

server.on("error", (e) => {
  if (e.code === "EADDRINUSE") console.error(`Порт ${PORT} занят. Запустите с другим: npm run site:serve -- ${PORT + 1}`);
  else console.error(e.message);
  process.exit(1);
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`Сайт из docs/ — http://127.0.0.1:${PORT}/   (Ctrl+C — остановить)`);
});
