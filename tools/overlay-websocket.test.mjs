// Исполняем настоящий сценарий оверлея без OBS, сети и сторонних библиотек.
// Таймеры и WebSocket управляются вручную, чтобы воспроизводить гонки.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createContext, runInContext } from "node:vm";

const html = readFileSync(new URL("../src-tauri/overlay/overlay.html", import.meta.url), "utf8");
const source = html.match(/<script>([\s\S]*?)<\/script>/u)[1];

function page({ constructorFailures = 0 } = {}) {
  const sockets = [], timers = new Map(), windowEvents = new Map(), documentEvents = new Map();
  const errors = [];
  let timerId = 0, attempts = 0;
  class Socket {
    static CONNECTING = 0; static OPEN = 1; static CLOSING = 2; static CLOSED = 3;
    constructor(url) {
      attempts++;
      if (constructorFailures-- > 0) throw new Error("WebSocket unavailable");
      this.url = url;
      this.readyState = Socket.CONNECTING;
      this.sent = [];
      sockets.push(this);
    }
    send(message) { this.sent.push(JSON.parse(message)); }
    open() { this.readyState = Socket.OPEN; this.onopen(); }
    message(data) { this.onmessage({ data: JSON.stringify(data) }); }
    close() { this.readyState = Socket.CLOSED; this.onclose(); }
  }
  const element = { classList: { add() {}, remove() {} }, style: {} };
  const context = createContext({
    window: {
      location: { pathname: "/overlay/ball", search: "?key=test", host: "127.0.0.1:3001", protocol: "http:" },
      isSecureContext: true,
      addEventListener(name, callback) { windowEvents.set(name, callback); },
      AudioContext: class {},
    },
    document: {
      readyState: "loading",
      getElementById() { return element; },
      addEventListener(name, callback) { documentEvents.set(name, callback); },
    },
    navigator: {}, localStorage: { getItem() { return null; } }, URLSearchParams, WebSocket: Socket,
    fetch: async () => ({ ok: true, json: async () => ({ id: "ball", name: "Баллы канала" }) }),
    console: { log() {}, error(...args) { errors.push(args); } },
    setTimeout(callback, delay) { timers.set(++timerId, { callback, delay }); return timerId; },
    clearTimeout(id) { timers.delete(id); }, setInterval() {},
  });
  runInContext(source, context);
  // Проверяем настоящие WS-обработчики и разбор медиа. Вместо браузерного
  // проигрывателя считаем переданные ему элементы; DOM/WebAudio не тестируем.
  runInContext(`
    globalThis.playback = { queue: [], immediate: [], cleared: 0 };
    startServerMonitor = () => {};
    addToQueue = item => playback.queue.push(item);
    playImmediate = item => playback.immediate.push(item);
    clearAll = () => playback.cleared++;
  `, context);
  return {
    sockets, timers, errors, playback: context.playback,
    init: () => documentEvents.get("DOMContentLoaded")(),
    event: name => windowEvents.get(name)(),
    value: expression => runInContext(expression, context),
    attempts: () => attempts,
    retry() {
      assert.equal(timers.size, 1, "должен быть ровно один таймер реконнекта");
      const [id, timer] = [...timers][0];
      assert.equal(timer.delay, 3000);
      timers.delete(id);
      timer.callback();
    },
  };
}

const media = queueMode => ({ command: "playVideo", videoFile: "alert.mp3", queueMode, volume: 75 });

test("focus и online во время CONNECTING оставляют один сокет", async () => {
  const p = page();
  await p.init();
  for (let i = 0; i < 10; i++) { p.event("focus"); p.event("online"); }
  assert.equal(p.sockets.length, 1);
  const socket = p.sockets[0];
  assert.equal(socket.url, "ws://127.0.0.1:3001/ws?path=ball&key=test");
  socket.open();
  socket.message(media("queue"));
  socket.message(media("immediate"));
  assert.equal(p.playback.queue.length, 1);
  assert.equal(p.playback.immediate.length, 1);
});

test("focus до завершения async init не создаёт второй сокет при init", async () => {
  const p = page();
  const initialized = p.init();
  p.event("focus");
  p.event("online");
  await initialized;
  assert.equal(p.sockets.length, 1);
});

test("OPEN сохраняет соединение, настройки и pong уходят через него", async () => {
  const p = page();
  await p.init();
  const socket = p.sockets[0];
  socket.open();
  p.event("focus"); p.event("online");
  socket.message({ command: "config", pauseBetweenMs: 250, imageDurationSec: 2 });
  socket.message({ command: "ping" });
  assert.equal(p.sockets.length, 1);
  assert.equal(p.value("MEDIA_PAUSE_DELAY"), 250);
  assert.equal(p.value("IMAGE_DURATION"), 2);
  assert.deepEqual(socket.sent, [{ command: "pong" }]);
  assert.equal(p.timers.size, 0);
});

test("CLOSING ждёт close; реконнект и события фокуса дают один новый сокет", async () => {
  const p = page();
  await p.init();
  const old = p.sockets[0];
  old.open();
  old.readyState = 2;
  p.event("focus"); p.event("online");
  assert.equal(p.sockets.length, 1);
  old.close();
  p.retry();
  p.event("focus"); p.event("online");
  assert.equal(p.sockets.length, 2);
  assert.equal(p.sockets[1].url, old.url);
  p.sockets[1].open();
  p.sockets[1].message(media("queue"));
  assert.equal(p.playback.queue.length, 1);
  assert.equal(p.timers.size, 0);
});

test("запоздавшие callbacks старого сокета не меняют новое соединение", async () => {
  const p = page();
  await p.init();
  const old = p.sockets[0];
  old.open();
  old.readyState = 3; // close уже произошёл, callback ещё не доставлен
  p.event("focus");
  const current = p.sockets[1];
  current.open();
  old.onopen();
  old.message(media("queue"));
  old.message(media("immediate"));
  old.message({ command: "config", pauseBetweenMs: 99, imageDurationSec: 99 });
  old.message({ command: "clearAll" });
  old.message({ command: "ping" });
  old.onerror();
  old.onclose();
  assert.equal(p.value("ws"), current);
  assert.equal(p.value("MEDIA_PAUSE_DELAY"), 3000);
  assert.equal(p.playback.queue.length, 0);
  assert.equal(p.playback.immediate.length, 0);
  assert.equal(p.playback.cleared, 0);
  assert.equal(current.sent.length, 0);
  assert.equal(p.errors.length, 0);
  assert.equal(p.timers.size, 0);
  current.message(media("queue"));
  assert.equal(p.playback.queue.length, 1);
});

test("error + close ставят один реконнект; повторные обрывы восстанавливаются", async () => {
  const p = page();
  await p.init();
  for (let i = 0; i < 3; i++) {
    const socket = p.sockets[i];
    socket.open();
    socket.onerror();
    assert.equal(p.timers.size, 0);
    socket.close();
    socket.onclose();
    p.retry();
  }
  assert.equal(p.sockets.length, 4);
  p.sockets[3].open();
  p.sockets[3].message(media("immediate"));
  assert.equal(p.playback.immediate.length, 1);
});

test("focus ускоряет ожидающий реконнект и отменяет старый таймер", async () => {
  const p = page();
  await p.init();
  p.sockets[0].close();
  assert.equal(p.timers.size, 1);
  p.event("focus"); p.event("online");
  assert.equal(p.sockets.length, 2);
  assert.equal(p.timers.size, 0);
});

test("ошибки конструктора повторяют попытки через таймер до успеха", async () => {
  const p = page({ constructorFailures: 3 });
  await p.init();
  for (let i = 0; i < 3; i++) p.retry();
  assert.equal(p.attempts(), 4);
  assert.equal(p.sockets.length, 1);
  assert.equal(p.timers.size, 0);
  p.sockets[0].open();
  p.sockets[0].message(media("queue"));
  assert.equal(p.playback.queue.length, 1);
});

test("две отдельные страницы одного path получают по одному алёрту", async () => {
  const a = page(), b = page();
  await Promise.all([a.init(), b.init()]);
  for (const p of [a, b]) {
    p.event("focus"); p.event("online");
    assert.equal(p.sockets.length, 1);
    p.sockets[0].open();
    p.sockets[0].message(media("queue"));
    p.sockets[0].message(media("immediate"));
    assert.equal(p.playback.queue.length, 1);
    assert.equal(p.playback.immediate.length, 1);
  }
});
