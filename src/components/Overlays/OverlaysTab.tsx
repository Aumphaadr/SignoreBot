import Icon from "../Icon";
import TestButton from "../Common/TestButton";
import { copyText } from "../../api/clipboard";
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText, type ObsSource, type Overlay, type Response } from "../../api";
import Modal, { ModalActions } from "../Common/Modal";
import { VariableBadges } from "../Common/VariableBadge";
import { Hint, hintFallback } from "../Common/hints";
import { newId } from "../../api/defaults";
import { useAppState } from "../../state/AppState";
import ResponseEditor from "../Common/ResponseEditor";
import { defaultResponse } from "../../api/defaults";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";
import "./OverlaysTab.css";

const FB_VARS = ["user", "reaction", "overlay"];
const FB_DESCR: Record<string, string> = { user: "кто вызвал реакцию", reaction: "какая реакция не показана — команда или награда", overlay: "имя этого оверлея" };

/** Редактор резервной реакции оверлея: состав + включение отдельной вкладкой. */
function FallbackEditor({ overlay, overlays, onSave }: { overlay: Overlay; overlays: Overlay[]; onSave: (fb: Response, enabled: boolean) => void }) {
  const [fb, setFb] = useState<Response>(overlay.fallback ?? defaultResponse());
  const [enabled, setEnabled] = useState(overlay.fallbackEnabled);
  return (
    <div className="reward-editor">
      <ModalActions>
        <TestButton response={fb} vars={{ user: "TestUser", reaction: "!тест", overlay: overlay.name }} />
        <button onClick={() => onSave(fb, enabled)} className="primary"><Icon name="save" /> Сохранить</button>
      </ModalActions>
      <div className="reward-refund-block">
        <label className="toggle-label">
          <span className="toggle-switch"><input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} /><span className="toggle-slider"></span></span>
          <span className="toggle-text">Резервная реакция включена</span>
          <Tooltip text={`Пока включена: если медиа пришло, а оверлей «${overlay.name}» не подключён, бот выполняет эту реакцию, а медиа в очередь оверлея не ставит. Выключенная реакция никуда не девается — состав сохраняется.`} />
        </label>
        <div className="form-hint">
          <Icon name="lightbulb" /> Чтобы бот замечал, что оверлей выключен при смене сцены, в OBS у Browser Source включите <b>«Выключать источник, когда он не виден»</b> (Shutdown source when not visible). Без этого страница живёт на всех сценах, и бот считает оверлей подключённым, даже если зритель его не видит. Заодно полезно включить <b>«Обновлять браузер при активации сцены»</b>.
        </div>
      </div>
      <div className="reward-editor-header">
        <p className="reward-vars-hint"><Icon name="lightbulb" /> Доступные переменные: <VariableBadges className="inline-variable-list" variables={FB_VARS} descriptions={FB_DESCR} /></p>
      </div>
      <ResponseEditor value={fb} onChange={setFb} overlays={overlays.filter((x) => x.id !== overlay.id)} variables={FB_VARS} />
    </div>
  );
}

const sanitize = (s: string) => s.toLowerCase().replace(/\s+/g, "-").replace(/[^a-z0-9\-_]/g, "").replace(/^-+|-+$/g, "");

export default function OverlaysTab() {
  const { config, setSection, status } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const overlays = config.overlays;
  const obs = config.obs;
  const [newName, setNewName] = useState("");
  const [newPath, setNewPath] = useState("");
  const [pathEdited, setPathEdited] = useState(false);
  const [sources, setSources] = useState<ObsSource[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [fbEdit, setFbEdit] = useState<Overlay | null>(null);
  const [pathDrafts, setPathDrafts] = useState<Record<string, string>>({});

  const setObs = (patch: Partial<typeof obs>) => setSection("obs", { ...obs, ...patch });
  const statusOf = (o: Overlay) => status?.overlays.find((s) => s.id === o.id);

  const add = () => {
    if (!newName.trim()) return void showNotification("Введите название оверлея!", NOTIFICATION_TYPES.WARNING, 2000);
    const path = sanitize(newPath || newName);
    if (!path) return void showNotification("Адрес оверлея не может быть пустым!", NOTIFICATION_TYPES.WARNING, 2000);
    if (overlays.some((o) => o.path === path)) return void showNotification("Оверлей с таким адресом уже существует!", NOTIFICATION_TYPES.ERROR, 3000);
    const o: Overlay = { id: newId("overlay"), name: newName.trim(), path, fallback: null, fallbackEnabled: false };
    setSection("overlays", [...overlays, o]);
    if (!obs.browserSources.some((b) => b.overlayPath === path)) setObs({ browserSources: [...obs.browserSources, { overlayPath: path, inputName: `Overlay ${o.name}` }] });
    setNewName(""); setNewPath(""); setPathEdited(false);
    showNotification(`Оверлей «${o.name}» создан`, NOTIFICATION_TYPES.SUCCESS, 2000);
  };
  const remove = (o: Overlay) => showConfirm(`Удалить оверлей «${o.name}»?\n\nРеакции, привязанные к нему, начнут играть на всех оверлеях.`, () => {
    setSection("overlays", overlays.filter((x) => x.id !== o.id));
    setObs({ browserSources: obs.browserSources.filter((b) => b.overlayPath !== o.path) });
    showNotification(`Оверлей «${o.name}» удалён`, NOTIFICATION_TYPES.WARNING, 2000);
  });
  const commitPath = (o: Overlay) => {
    const draft = pathDrafts[o.id];
    if (draft === undefined) return;
    const p = sanitize(draft);
    setPathDrafts((d) => { const n = { ...d }; delete n[o.id]; return n; });
    if (!p) return void showNotification("Адрес оверлея не может быть пустым", NOTIFICATION_TYPES.WARNING, 2000);
    if (p === o.path) return;
    if (overlays.some((x) => x.id !== o.id && x.path === p)) return void showNotification("Такой адрес уже занят", NOTIFICATION_TYPES.ERROR, 2000);
    setSection("overlays", overlays.map((x) => (x.id === o.id ? { ...x, path: p } : x)));
    setObs({ browserSources: obs.browserSources.map((b) => (b.overlayPath === o.path ? { ...b, overlayPath: p } : b)) });
    showNotification("Адрес изменён — обновите URL в OBS", NOTIFICATION_TYPES.WARNING, 3000);
  };
  const copy = (url: string) => copyText(url).then(() => showNotification("URL скопирован", NOTIFICATION_TYPES.SUCCESS, 2000)).catch(() => showNotification(`${url}`, NOTIFICATION_TYPES.INFO, 6000));
  const testObs = async () => {
    setBusy(true);
    try { const s = await api.obsTest(); setSources(s); showNotification(`OBS подключён, источников: ${s.length}`, NOTIFICATION_TYPES.SUCCESS, 2500); }
    catch (e) { setSources(null); showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 5000); }
    finally { setBusy(false); }
  };
  const refreshObs = async () => {
    setBusy(true);
    try { const r = await api.obsRefresh(); showNotification(r.length ? `Перезагружено источников: ${r.length}` : "Ни один источник не обновлён (см. логи)", r.length ? NOTIFICATION_TYPES.SUCCESS : NOTIFICATION_TYPES.WARNING, 3000); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 5000); }
    finally { setBusy(false); }
  };
  const setUrl = async (inputName: string, path: string) => {
    try { const msg = await api.obsSetUrl(inputName, path); showNotification(msg, NOTIFICATION_TYPES.SUCCESS, 3000); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 5000); }
  };
  const browserSources = sources?.filter((s) => s.inputKind.includes("browser")) ?? [];
  // список источников OBS — для подсказки в поле «Browser Source в OBS»
  useEffect(() => { if (obs.enabled && sources === null) api.obsTest().then(setSources).catch(() => {}); }, [obs.enabled, sources]);
  const matchSources = async () => {
    try {
      const found = await api.obsMatchSources();
      if (found.length === 0) showNotification("В OBS не нашлось Browser Source с адресами этих оверлеев. Пропишите адрес кнопкой «В OBS» у оверлея или укажите имя источника вручную.", NOTIFICATION_TYPES.WARNING, 6000);
      else showNotification(`Подобрано: ${found.map((b) => `${b.overlayPath} → «${b.inputName}»`).join(", ")}`, NOTIFICATION_TYPES.SUCCESS, 5000);
    } catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 5000); }
  };

  return (
    <div className="overlays-tab">
      <div className="overlays-header">
        <h2><Icon name="overlay-screen" /> Оверлеи</h2>
        <p className="overlays-description">Оверлеи — веб-страницы, которые вы добавляете как Browser Source в OBS. У каждого свой URL с ключом доступа; медиа можно направлять на конкретный оверлей.</p>
        {status?.server.error && <div className="error-message"><Icon name="error-badge" /> Сервер оверлеев не запущен: {status.server.error}. Измените порт в «Настройках».</div>}
        {!config.network.allowLan && (
          <p className="form-hint" style={{ marginTop: 10 }}><Icon name="lock" /> Сервер оверлеев слушает только этот компьютер. Если OBS работает на другом компьютере, включите «Доступ из локальной сети» в «Настройки → Сеть».</p>
        )}
        {config.network.allowLan && (
          <label className="toggle-label" style={{ marginTop: 12 }}>
            <span className="toggle-switch"><input type="checkbox" checked={config.network.preferLocalhostUrls} onChange={(e) => setSection("network", { ...config.network, preferLocalhostUrls: e.target.checked })} /><span className="toggle-slider"></span></span>
            <span className="toggle-text">OBS на этом же компьютере — адреса с <code>127.0.0.1</code></span>
            <Tooltip text="Для 127.0.0.1 в OBS работает офлайн-кэш страницы (Service Worker): порядок запуска OBS и бота не важен — оверлей сам подключится, когда бот появится. Для адресов по локальной сети (другой компьютер) кэш недоступен: если OBS запущен раньше бота, включите интеграцию OBS WebSocket ниже — бот перезагрузит Browser Source сам." />
          </label>
        )}
      </div>

      <div className="overlays-list">
        {overlays.length === 0 && <div className="empty-overlays"><p><Icon name="inbox-empty" /> Оверлеи не созданы</p><p className="hint">Создайте первый оверлей, чтобы начать</p></div>}
        {overlays.map((o) => {
          const st = statusOf(o);
          const bs = obs.browserSources.find((b) => b.overlayPath === o.path);
          return (
            <div key={o.id} className="overlay-card">
              <div className="overlay-card-fields">
                <div className="overlay-field">
                  <label>Название</label>
                  <input type="text" value={o.name} onChange={(e) => setSection("overlays", overlays.map((x) => (x.id === o.id ? { ...x, name: e.target.value } : x)))} placeholder="Название оверлея" className="overlay-name-input" />
                </div>
                <div className="overlay-field">
                  <label>Адрес (путь)</label>
                  <div className="overlay-path-group">
                    <span className="path-prefix">/overlay/</span>
                    <input type="text" value={pathDrafts[o.id] ?? o.path} onChange={(e) => setPathDrafts((d) => ({ ...d, [o.id]: e.target.value }))} onBlur={() => commitPath(o)} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} placeholder="my-overlay" className="overlay-path-input" />
                  </div>
                </div>
                <div className="overlay-field">
                  <label>Browser Source в OBS <Tooltip text="Имя источника в OBS точно как в списке источников OBS — по нему бот перезагружает страницу и прописывает адрес. При включённой интеграции список подставляется из OBS; кнопка «Подобрать по адресам» в блоке OBS заполняет поле сама." /></label>
                  <input type="text" list="obs-browser-sources" value={bs?.inputName ?? ""} placeholder={`Overlay ${o.name}`} onChange={(e) => setObs({ browserSources: bs ? obs.browserSources.map((b) => (b.overlayPath === o.path ? { ...b, inputName: e.target.value } : b)) : [...obs.browserSources, { overlayPath: o.path, inputName: e.target.value }] })} className="overlay-name-input" />
                  {browserSources.length > 0 && <datalist id="obs-browser-sources">{browserSources.map((x) => <option key={x.inputName} value={x.inputName} />)}</datalist>}
                </div>
              </div>
              <div className="overlay-card-url">
                {st && !st.connected && st.pageRequestOk !== false && st.pageRequestAgeSec !== null ? <Hint text={st.hint ?? ""}><span className={`badge ${st?.connected ? "badge-success" : "badge-warning"}`}>{st?.connected ? <><Icon name="status-connected" /> подключён{(st.connections ?? 0) > 1 ? ` ×${st.connections}` : ""}</> : <><Icon name="status-disconnected" /> не подключён</>}</span></Hint> : <Hint text={st?.connected ? <>подключений: {st.connections}</> : "оверлей не подключён"}><span className={`badge ${st?.connected ? "badge-success" : "badge-warning"}`}>{st?.connected ? <><Icon name="status-connected" /> подключён{(st.connections ?? 0) > 1 ? ` ×${st.connections}` : ""}</> : <><Icon name="status-disconnected" /> не подключён</>}</span></Hint>}
                {st && st.pending > 0 && <span className="badge badge-info">в очереди: {st.pending}</span>}
                {st && !st.connected && st.pageRequestOk === false && <Hint text={st.hint ?? ""}><span className="badge badge-warning">адрес в OBS без ключа</span></Hint>}
                {st && !st.connected && st.pageRequestAgeSec === null && <Hint text={st.hint ?? ""}><span className="badge badge-warning">страницу не запрашивали</span></Hint>}
                <span className="overlay-url">{st?.url ?? `…/overlay/${o.path}`}</span>
              </div>
              <div className="overlay-card-actions">
                <button onClick={() => st && void copy(st.url)} className="overlay-action-btn copy" disabled={!st}><Icon name="copy"  /> Копировать URL</button>
                <button onClick={() => st && void openUrl(st.url)} className="overlay-action-btn open" disabled={!st}><Icon name="external-link"  /> Открыть</button>
                {obs.enabled && bs?.inputName && <button onClick={() => void setUrl(bs.inputName, o.path)} className="overlay-action-btn open" title="Записать этот URL в Browser Source OBS"><Icon name="plug"  /> В OBS</button>}
                <button onClick={() => void api.overlayClear(o.path, true).then(() => showNotification("Оверлей остановлен", NOTIFICATION_TYPES.INFO, 1500))} className="overlay-action-btn copy" title="Остановить всё, что сейчас играет"><Icon name="stop"  /> Стоп</button>
                <Hint text={hintFallback(o, overlays)}><button onClick={() => setFbEdit(o)} className={`overlay-action-btn ${o.fallbackEnabled && o.fallback ? "fallback-on" : ""}`}><Icon name="warning" /> Если недоступен</button></Hint>
                <button onClick={() => remove(o)} className="overlay-action-btn delete"><Icon name="delete"  /> Удалить</button>
              </div>
            </div>
          );
        })}
      </div>

      <Modal isOpen={!!fbEdit} onClose={() => setFbEdit(null)} size="xlarge" title={fbEdit ? `Если оверлей «${fbEdit.name}» недоступен` : ""}>
        {fbEdit && <FallbackEditor key={fbEdit.id} overlay={fbEdit} overlays={overlays} onSave={(fb, enabled) => {
          setSection("overlays", overlays.map((x) => (x.id === fbEdit.id ? { ...x, fallback: fb, fallbackEnabled: enabled } : x)));
          setFbEdit(null);
          showNotification(`Резервная реакция «${fbEdit.name}» ${enabled ? "сохранена и включена" : "сохранена (выключена)"}`, NOTIFICATION_TYPES.SUCCESS, 2000);
        }} />}
      </Modal>

      <div className="add-overlay-form">
        <h3><Icon name="add" /> Новый оверлей</h3>
        <div className="add-overlay-fields">
          <div className="add-overlay-field"><label>Название</label><input type="text" value={newName} onChange={(e) => { setNewName(e.target.value); if (!pathEdited) setNewPath(sanitize(e.target.value)); }} placeholder="Например: Алерты" className="add-overlay-input" /></div>
          <div className="add-overlay-field">
            <label>Адрес <Tooltip text="Только латинские буквы, цифры, дефис и подчёркивание" /></label>
            <div className="overlay-path-group"><span className="path-prefix">/overlay/</span><input type="text" value={newPath} onChange={(e) => { setPathEdited(true); setNewPath(sanitize(e.target.value)); }} placeholder="alerts" className="add-overlay-path-input" /></div>
          </div>
          <button onClick={add} className="add-overlay-btn"><Icon name="add"  /> Создать</button>
        </div>
      </div>

      <div className="obs-integration-card" style={{ marginTop: 24 }}>
        <div className="obs-integration-header">
          <div className="obs-integration-title">
            <h3><Icon name="plug"  /> OBS / Streamlabs WebSocket</h3>
            <p className="obs-integration-description">Если OBS запущен раньше бота, Browser Source не переподключаются сами — бот может перезагрузить их через OBS WebSocket (OBS 28+, «Инструменты → Настройки WebSocket-сервера»).</p>
          </div>
          <label className="toggle-label obs-toggle-row">
            <span className="toggle-text">{obs.enabled ? "Включено" : "Выключено"}</span>
            <span className="toggle-switch"><input type="checkbox" checked={obs.enabled} onChange={(e) => setObs({ enabled: e.target.checked })} /><span className="toggle-slider"></span></span>
          </label>
        </div>
        {obs.enabled && (
          <div className="obs-integration-body">
            <div className="form-row">
              <div className="form-group"><label>Адрес WebSocket</label><input type="text" value={obs.url} onChange={(e) => setObs({ url: e.target.value })} placeholder="ws://127.0.0.1:4455" /></div>
              <div className="form-group"><label>Пароль</label><input type="password" value={obs.password} onChange={(e) => setObs({ password: e.target.value })} placeholder="если задан в OBS" /></div>
            </div>
            <label className="toggle-label" style={{ marginBottom: 12 }}>
              <span className="toggle-switch"><input type="checkbox" checked={obs.autoRefresh} onChange={(e) => setObs({ autoRefresh: e.target.checked })} /><span className="toggle-slider"></span></span>
              <span>Автоматически перезагружать Browser Source, если оверлеи не подключились после запуска</span>
            </label>
            <div className="flex gap-2">
              <button onClick={() => void testObs()} disabled={busy} className="outline"><Icon name="plug"  /> Проверить подключение</button>
              <button onClick={() => void refreshObs()} disabled={busy}><Icon name="refresh"  /> Перезагрузить Browser Source</button>
            </div>
            {sources && (
              <div className="obs-meta-list" style={{ marginTop: 12 }}>
                {status?.obsProblem && <div className="form-hint text-warning"><Icon name="warning" /> {status.obsProblem}</div>}
                <div className="obs-meta-item"><span className="obs-meta-label">Browser Source в OBS:</span>{browserSources.length === 0 && <span>нет</span>}<button className="small" style={{ marginLeft: "auto" }} onClick={() => void matchSources()} title="Найти в OBS источники, чьи адреса ведут на оверлеи бота, и записать их имена в привязки"><Icon name="lightning" /> Подобрать по адресам</button></div>
                {browserSources.map((s) => (
                  <div key={s.inputName} className="obs-meta-item"><code>{s.inputName}</code><span className="text-muted" style={{ fontSize: 12 }}>{s.url ?? "—"}</span></div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
