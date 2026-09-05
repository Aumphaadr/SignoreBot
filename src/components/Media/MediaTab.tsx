// Каталог медиа: список, добавление, удаление, уборка неиспользуемых.

import Icon, { type IconName } from "../Icon";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errText, type MediaFile, type MediaSet } from "../../api";
import { useAppState } from "../../state/AppState";
import { newId, setKindLabel } from "../../api/defaults";
import Tooltip from "../Tooltip";
import Modal from "../Common/Modal";
import { formatSize } from "../../api/defaults";
import { pickAndImport, useMediaFiles } from "../Common/MediaEditor";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import "../Common/MediaEditor.css";

export default function MediaTab() {
  const { files, reload } = useMediaFiles();
  const { showNotification, showConfirm } = useNotification();
  const { config, setSection } = useAppState();
  const sets = config.mediaSets;
  const saveSets = (next: MediaSet[]) => setSection("mediaSets", next);
  const [setFilter, setSetFilter] = useState<string>("all");
  const [newSetName, setNewSetName] = useState("");
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  // Черновик наборов для открытого файла: галочки применяются по «Сохранить»
  const [draftSets, setDraftSets] = useState<string[]>([]);
  const createSet = (name: string): MediaSet | null => {
    const n = name.trim();
    if (!n) { showNotification("Введите название набора", NOTIFICATION_TYPES.WARNING, 2000); return null; }
    if (sets.some((s) => s.name.toLowerCase() === n.toLowerCase())) { showNotification(`Набор «${n}» уже есть`, NOTIFICATION_TYPES.WARNING, 2500); return null; }
    const ms: MediaSet = { id: newId("set"), name: n, files: [] };
    saveSets([...sets, ms]);
    showNotification(`Набор «${n}» создан`, NOTIFICATION_TYPES.SUCCESS, 2000);
    return ms;
  };
  const deleteSet = (s: MediaSet) => showConfirm(`Удалить набор «${s.name}»?\n\nФайлы останутся на месте. Реакции, которые показывали случайный файл из этого набора, перестанут отправлять медиа, пока им не выберут другой набор.`, () => {
    saveSets(sets.filter((x) => x.id !== s.id));
    if (setFilter === s.id) setSetFilter("all");
    showNotification(`Набор «${s.name}» удалён`, NOTIFICATION_TYPES.WARNING, 2500);
  });
  const applyDraftSets = (name: string) => {
    saveSets(sets.map((s) => {
      const want = draftSets.includes(s.id); const has = s.files.includes(name);
      if (want === has) return s;
      return { ...s, files: want ? [...s.files, name] : s.files.filter((f) => f !== name) };
    }));
    showNotification(draftSets.length ? `«${name}» — в наборах: ${sets.filter((s) => draftSets.includes(s.id)).map((s) => s.name).join(", ")}` : `«${name}» не входит ни в один набор`, NOTIFICATION_TYPES.SUCCESS, 2500);
    setPreview(null);
  };
  const openFile = (f: MediaFile) => { setDraftSets(setsOf(f.name).map((s) => s.id)); setPreview(f); };
  const setsOf = (name: string) => sets.filter((s) => s.files.includes(name));
  const missingOf = (s: MediaSet) => s.files.filter((n) => !files.some((f) => f.name === n));
  // «используется/сирота» считает ядро с учётом наборов — после правки наборов перечитываем список
  useEffect(() => { void reload(); }, [sets, reload]);
  const [q, setQ] = useState("");
  const [kind, setKind] = useState<"all" | "video" | "audio" | "image">("all");
  const [onlyUnused, setOnlyUnused] = useState(false);
  const [preview, setPreview] = useState<MediaFile | null>(null);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  // Громкость плеера: у нативных контролов WebKit нет ползунка, только mute.
  const [volume, setVolume] = useState(() => { try { const v = parseFloat(localStorage.getItem("sb.previewVolume") ?? ""); return Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 1; } catch { return 1; } });
  const playerRef = useRef<HTMLMediaElement | null>(null);
  useEffect(() => {
    if (playerRef.current) playerRef.current.volume = volume;
    try { localStorage.setItem("sb.previewVolume", String(volume)); } catch { /* приватный режим */ }
  }, [volume, previewUrl]);
  useEffect(() => {
    if (!preview) { setPreviewUrl(null); return; }
    let alive = true;
    api.mediaUrl(preview.name).then((u) => { if (alive) setPreviewUrl(u); }).catch(() => setPreviewUrl(null));
    return () => { alive = false; };
  }, [preview]);
  const list = useMemo(() => files.filter((f) => (kind === "all" || f.kind === kind) && (!onlyUnused || !f.used) && (setFilter === "all" || !!sets.find((s) => s.id === setFilter)?.files.includes(f.name)) && f.name.toLowerCase().includes(q.toLowerCase())), [files, kind, onlyUnused, q, setFilter, sets]);
  const unused = files.filter((f) => !f.used);
  const unusedSize = unused.reduce((a, f) => a + f.size, 0);
  const icon = (k: string): IconName => (k === "video" ? "clapperboard" : k === "audio" ? "audio-note" : k === "image" ? "image" : "document");

  const del = (name: string) => showConfirm(`Удалить файл "${name}"?`, async () => {
    try { await api.mediaDelete(name); setPreview((p) => (p?.name === name ? null : p)); showNotification(`Файл "${name}" удалён`, NOTIFICATION_TYPES.WARNING, 2000); await reload(); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  });
  const cleanup = () => showConfirm(`Удалить все неиспользуемые файлы (${unused.length} шт., ${formatSize(unusedSize)})?\n\nЭто файлы, на которые не ссылается ни одна команда, награда, событие или таймер.`, async () => {
    try { const n = await api.mediaDeleteUnused(); showNotification(`Удалено файлов: ${n}`, NOTIFICATION_TYPES.SUCCESS, 3000); await reload(); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  });

  return (
    <div className="media-tab">
      <div className="commands-header">
        <h2><Icon name="filmstrip" /> Медиа</h2>
        <p className="commands-description">Все файлы, доступные для реакций. Всего: {files.length} ({formatSize(files.reduce((a, f) => a + f.size, 0))}), неиспользуемых: {unused.length} ({formatSize(unusedSize)}).</p>
        <div className="flex gap-2 mt-3">
          <button className="primary" onClick={() => void pickAndImport(null, showNotification).then((a) => { if (a.length) void reload(); })}><Icon name="add"  /> Добавить файлы</button>
          <button className="warning" onClick={cleanup} disabled={unused.length === 0}><Icon name="clean"  /> Удалить неиспользуемые</button>
        </div>
      </div>
      <div className="media-sets">
        <div className="media-sets-header">
          <h3><Icon name="layers" /> Наборы <Tooltip text="Набор — список файлов, из которого реакция показывает случайный файл без повторов подряд (выбирается в редакторе медиа: «Случайный из набора»). Файл может входить в несколько наборов. Файл в наборе считается используемым." /></h3>
          <div className="flex gap-2 items-center">
            <input type="text" value={newSetName} onChange={(e) => setNewSetName(e.target.value)} placeholder="Название нового набора" onKeyDown={(e) => { if (e.key === "Enter" && createSet(newSetName)) setNewSetName(""); }} style={{ width: 240 }} />
            <button className="small" onClick={() => { if (createSet(newSetName)) setNewSetName(""); }}><Icon name="add" /> Создать набор</button>
          </div>
        </div>
        {sets.length === 0 ? <p className="form-hint">Наборов пока нет. Создайте набор, затем у нужных файлов нажмите «В набор…».</p> : (
          <div className="media-sets-list">
            {sets.map((s) => {
              const info = setKindLabel(s.files); const missing = missingOf(s);
              return (
                <div key={s.id} className={`media-set-card ${setFilter === s.id ? "active" : ""}`}>
                  {renaming?.id === s.id ? (
                    <input type="text" autoFocus value={renaming.name} onChange={(e) => setRenaming({ id: s.id, name: e.target.value })}
                      onKeyDown={(e) => { if (e.key === "Enter") { const n = renaming.name.trim(); if (n) saveSets(sets.map((x) => (x.id === s.id ? { ...x, name: n } : x))); setRenaming(null); } if (e.key === "Escape") setRenaming(null); }}
                      onBlur={() => { const n = renaming.name.trim(); if (n) saveSets(sets.map((x) => (x.id === s.id ? { ...x, name: n } : x))); setRenaming(null); }} />
                  ) : <button className="media-set-name" onClick={() => setSetFilter(setFilter === s.id ? "all" : s.id)} title="Показать файлы набора">{s.name}</button>}
                  <span className={`badge ${info.empty || info.mixed ? "badge-warning" : "badge-info"}`} title={info.mixed ? `В наборе файлы разных типов (${info.kinds.join(", ")}): настройки длительности и анимаций подходят не всем, а предпросмотр показывает один файл. Работать будет, но лучше держать наборы однотипными.` : info.empty ? "В наборе нет файлов — реакция с ним ничего не покажет" : "Все файлы одного типа"}>{info.label}</span>
                  {missing.length > 0 && <span className="badge badge-warning" title={`Нет на диске: ${missing.join(", ")}`}>нет на диске: {missing.length}</span>}
                  <span className="media-set-actions">
                    <button className="small" onClick={() => setRenaming({ id: s.id, name: s.name })} title="Переименовать"><Icon name="edit" /></button>
                    <button className="small danger" onClick={() => deleteSet(s)} title="Удалить набор (файлы остаются)"><Icon name="delete" /></button>
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
      <div className="file-browser media-tab-browser">
        <div className="file-browser-header">
          <div className="flex gap-2 items-center">
            <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)} style={{ width: "auto" }}>
              <option value="all">Все типы</option><option value="video">Видео</option><option value="audio">Аудио</option><option value="image">Картинки</option>
            </select>
            {sets.length > 0 && (
              <select value={setFilter} onChange={(e) => setSetFilter(e.target.value)} style={{ width: "auto" }}>
                <option value="all">Все наборы</option>{sets.map((s) => <option key={s.id} value={s.id}>Набор: {s.name}</option>)}
              </select>
            )}
            <label className="toggle-label"><input type="checkbox" checked={onlyUnused} onChange={(e) => setOnlyUnused(e.target.checked)} style={{ width: "auto" }} /> только неиспользуемые</label>
          </div>
          <div className="file-search">
            <Icon name="search" className="search-icon" />
            <input type="text" placeholder="Поиск файлов..." value={q} onChange={(e) => setQ(e.target.value)} className="file-search-input" />
            {q && <button className="search-clear-btn" onClick={() => setQ("")}><Icon name="close" /> </button>}
          </div>
        </div>
        {list.length === 0 ? <p className="empty-files">{files.length === 0 ? "Нет файлов" : "Ничего не найдено"}</p> : (
          <div className="files-grid">
            {list.map((f) => (
              <div key={f.name} className="file-item" onClick={() => openFile(f)} title="Открыть: предпросмотр, наборы, удаление">
                <span className="file-icon"><Icon name={icon(f.kind)} /></span>
                <span className="file-name" title={f.name}>{f.name}</span>
                {setsOf(f.name).map((s) => <span key={s.id} className="badge badge-info" title={`В наборе «${s.name}»`}>{s.name}</span>)}
                {!f.used && <span className="badge badge-warning" title="Не используется ни одной реакцией и не входит ни в один набор">сирота</span>}
                <span className="file-size">{formatSize(f.size)}</span>
                <button className="delete-file-btn" title="Удалить" onClick={(e) => { e.stopPropagation(); del(f.name); }}><Icon name="delete"  /></button>
              </div>
            ))}
          </div>
        )}
      </div>
      <Modal isOpen={!!preview} onClose={() => setPreview(null)} title={preview ? <><Icon name={icon(preview.kind)} /> {preview.name}</> : ""} size="large">
        {preview && (
          <div className="media-preview-modal">
            {previewUrl && preview.kind === "image" && <img src={previewUrl} alt="" />}
            {previewUrl && preview.kind === "video" && <video ref={(el) => { playerRef.current = el; }} src={previewUrl} controls preload="auto" onLoadedMetadata={(e) => { e.currentTarget.volume = volume; }} />}
            {previewUrl && preview.kind === "audio" && <audio ref={(el) => { playerRef.current = el; }} src={previewUrl} controls preload="auto" onLoadedMetadata={(e) => { e.currentTarget.volume = volume; }} />}
            {previewUrl && (preview.kind === "video" || preview.kind === "audio") && (
              <div className="media-preview-volume">
                <button className="small" onClick={() => setVolume(volume > 0 ? 0 : 1)} title={volume > 0 ? "Выключить звук" : "Включить звук"}>{volume > 0 ? <Icon name="volume-on"  /> : <Icon name="volume-muted"  />}</button>
                <input type="range" min={0} max={100} value={Math.round(volume * 100)} onChange={(e) => setVolume(parseInt(e.target.value) / 100)} />
                <span className="media-preview-volume-value">{Math.round(volume * 100)}%</span>
              </div>
            )}
            {preview.kind === "unknown" && <p className="text-muted">Неизвестный тип файла — предпросмотр недоступен.</p>}
            <div className="form-hint" style={{ marginTop: 10 }}>{formatSize(preview.size)} · {new Date(preview.modified).toLocaleString("ru-RU")} · {preview.used ? "используется в реакциях" : "не используется (сирота)"}</div>
            <div className="media-file-settings">
              <h4><Icon name="layers" /> Наборы</h4>
              {sets.length === 0 ? <p className="form-hint">Наборов пока нет — создайте набор в блоке «Наборы» над списком файлов.</p> : (
                <div className="media-file-sets">
                  {sets.map((s) => (
                    <label key={s.id} className="toggle-label media-file-set-row">
                      <input type="checkbox" checked={draftSets.includes(s.id)} onChange={(e) => setDraftSets((d) => (e.target.checked ? [...d, s.id] : d.filter((x) => x !== s.id)))} style={{ width: "auto" }} />
                      <span>{s.name}</span><span className="badge badge-info">{setKindLabel(s.files).label}</span>
                    </label>
                  ))}
                </div>
              )}
              <div className="flex gap-2" style={{ marginTop: 12 }}>
                <button className="primary" onClick={() => applyDraftSets(preview.name)}><Icon name="save" /> Сохранить</button>
                <button onClick={() => setPreview(null)}>Закрыть</button>
                <button className="danger" style={{ marginLeft: "auto" }} onClick={() => del(preview.name)}><Icon name="delete" /> Удалить файл</button>
              </div>
            </div>
          </div>
        )}
      </Modal>
    </div>
  );
}
