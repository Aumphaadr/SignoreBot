// Редактор медиа-реакции: файл(ы), оверлей, громкость, режим, анимации,
// текст, предпросмотр, тест на оверлее.

import Icon, { type IconName } from "../Icon";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errText, type MediaFile, type MediaResponse, type Overlay, type Response } from "../../api";
import { FONT_FAMILIES, MEDIA_ENTER_ANIMATIONS, MEDIA_EXIT_ANIMATIONS, TEXT_ANIMATIONS, defaultMedia, fileKind, formatSize, substituteSample } from "../../api/defaults";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";
import { VariableBadges } from "./VariableBadge";
import "./MediaEditor.css";
import "../../../src-tauri/overlay/overlay.css";
import "../../styles/overlay-fonts.css";
import FontPicker from "./FontPicker";

type Kind = ReturnType<typeof fileKind>;

function secondaryKindFor(primary: Kind): Kind | null {
  if (primary === "image") return "audio";
  if (primary === "audio") return "image";
  return null;
}

// ------------------------------------------------------------------ файлы

export function useMediaFiles() {
  const { showNotification } = useNotification();
  const [files, setFiles] = useState<MediaFile[]>([]);
  const reload = useCallback(async () => {
    try { setFiles(await api.mediaList()); } catch (e) { showNotification(`Список медиа: ${errText(e)}`, NOTIFICATION_TYPES.ERROR); }
  }, [showNotification]);
  useEffect(() => { void reload(); }, [reload]);
  return { files, reload };
}

export async function pickAndImport(accept: Kind | null, showNotification: ReturnType<typeof useNotification>["showNotification"]): Promise<MediaFile[]> {
  const filters =
    accept === "video" ? [{ name: "Видео", extensions: ["mp4", "webm", "mov", "m4v"] }]
    : accept === "audio" ? [{ name: "Аудио", extensions: ["mp3", "wav", "ogg", "m4a", "flac", "opus"] }]
    : accept === "image" ? [{ name: "Картинки", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"] }]
    : [{ name: "Медиа", extensions: ["mp4", "webm", "mov", "m4v", "mp3", "wav", "ogg", "m4a", "flac", "opus", "png", "jpg", "jpeg", "gif", "webp", "bmp"] }];
  const picked = await open({ multiple: true, filters });
  if (!picked) return [];
  const paths = Array.isArray(picked) ? picked : [picked];
  const r = await api.mediaImport(paths);
  r.errors.forEach((e) => showNotification(`${e}`, NOTIFICATION_TYPES.ERROR, 5000));
  if (r.files.length) showNotification(`Добавлено файлов: ${r.files.length}`, NOTIFICATION_TYPES.SUCCESS, 2000);
  return r.files;
}

function FileBrowser({ files, selected, filter, onSelect, onDelete }: { files: MediaFile[]; selected: string; filter: Kind | null; onSelect: (f: MediaFile) => void; onDelete: (name: string) => void }) {
  const { showConfirm } = useNotification();
  const [q, setQ] = useState("");
  const list = useMemo(() => files.filter((f) => (!filter || f.kind === filter) && f.name.toLowerCase().includes(q.toLowerCase())), [files, filter, q]);
  const icon = (k: string): IconName => (k === "video" ? "clapperboard" : k === "audio" ? "audio-note" : k === "image" ? "image" : "document");
  return (
    <div className="file-browser">
      <div className="file-browser-header">
        <h4><Icon name="folder-open" /> Медиа-файлы {filter && `(${filter})`}</h4>
        <div className="file-search">
          <Icon name="search" className="search-icon" />
          <input type="text" placeholder="Поиск файлов..." value={q} onChange={(e) => setQ(e.target.value)} className="file-search-input" />
          {q && <button className="search-clear-btn" onClick={() => setQ("")}><Icon name="close" /> </button>}
        </div>
      </div>
      {list.length === 0 ? (
        <p className="empty-files">{files.length === 0 ? "Нет файлов" : `Нет файлов, содержащих "${q}"`}</p>
      ) : (
        <div className="files-grid">
          {list.map((f) => (
            <div key={f.name} className={`file-item ${selected === f.name ? "selected" : ""}`} onClick={() => onSelect(f)}>
              <span className="file-icon"><Icon name={icon(f.kind)} /></span>
              <span className="file-name" title={f.name}>{f.name}</span>
              <span className="file-size">{formatSize(f.size)}</span>
              <button className="delete-file-btn" title="Удалить" onClick={(e) => { e.stopPropagation(); showConfirm(`Удалить файл "${f.name}"?`, () => onDelete(f.name)); }}><Icon name="delete"  /></button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function FileSelector({ label, selected, accept, files, onSelect, onClear, onDelete, onImported }: {
  label: React.ReactNode; selected: string; accept: Kind | null; files: MediaFile[];
  onSelect: (name: string) => void; onClear: () => void; onDelete: (name: string) => void; onImported: () => void;
}) {
  const { showNotification } = useNotification();
  const [browse, setBrowse] = useState(false);
  const [busy, setBusy] = useState(false);
  const doImport = async () => {
    setBusy(true);
    try {
      const added = await pickAndImport(accept, showNotification);
      if (added.length) { onImported(); onSelect(added[0].name); }
    } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR); }
    finally { setBusy(false); }
  };
  return (
    <div className="media-file-selector">
      <label>{label}</label>
      <div className="file-input-group">
        <input type="text" value={selected || "Файл не выбран"} readOnly className="file-name-display" />
        <div className="file-button-group">
          <button onClick={doImport} className="browse-btn" disabled={busy}><Icon name="upload"  /> {busy ? "…" : "Добавить"}</button>
          <button onClick={() => setBrowse((b) => !b)} className="browse-btn browse-existing"><Icon name="folder-open"  /> Из папки</button>
          {selected && <button onClick={onClear} className="browse-btn clear-file-btn" title="Убрать файл"><Icon name="close" /> </button>}
        </div>
      </div>
      {browse && <FileBrowser files={files} selected={selected} filter={accept} onSelect={(f) => { onSelect(f.name); setBrowse(false); }} onDelete={onDelete} />}
    </div>
  );
}

// ------------------------------------------------------------------ предпросмотр
// Строит ТОТ ЖЕ DOM и использует ТОТ ЖЕ CSS, что и overlay.html (createMediaWithText):
// холст 1920×1080 масштабируется под ширину панели.

const CANVAS_W = 1920, CANVAS_H = 1080;

function MediaPreview({ media }: { media: MediaResponse }) {
  const [show, setShow] = useState(false);
  const [urls, setUrls] = useState<{ primary?: string; secondary?: string }>({});
  const [playing, setPlaying] = useState(false);
  const [scale, setScale] = useState(0.4);
  const videoRef = useRef<HTMLVideoElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!show) return;
    let alive = true;
    (async () => {
      const p = media.file ? await api.mediaUrl(media.file).catch(() => undefined) : undefined;
      const s = media.secondaryFile ? await api.mediaUrl(media.secondaryFile).catch(() => undefined) : undefined;
      if (alive) setUrls({ primary: p, secondary: s });
    })();
    return () => { alive = false; };
  }, [show, media.file, media.secondaryFile]);

  useEffect(() => {
    if (!show || !stageRef.current) return;
    const el = stageRef.current;
    const ro = new ResizeObserver(() => setScale(el.clientWidth / CANVAS_W));
    ro.observe(el);
    setScale(el.clientWidth / CANVAS_W);
    return () => ro.disconnect();
  }, [show]);

  const kind = fileKind(media.file);
  const image = kind === "image" ? urls.primary : kind === "audio" && media.secondaryFile ? urls.secondary : undefined;
  const audio = kind === "audio" ? urls.primary : kind === "image" && media.secondaryFile ? urls.secondary : undefined;
  const video = kind === "video" ? urls.primary : undefined;

  const stop = () => {
    videoRef.current?.pause(); audioRef.current?.pause();
    if (videoRef.current) videoRef.current.currentTime = 0;
    if (audioRef.current) audioRef.current.currentTime = 0;
    setPlaying(false);
  };
  const play = () => {
    const w = wrapRef.current;
    if (w) {
      w.classList.remove(...Array.from(w.classList).filter((c) => c.startsWith("media-enter-")));
      if (media.animation.enter !== "none") {
        w.style.animationDuration = `${media.animation.enterDuration}s`;
        void w.offsetWidth;
        w.classList.add(`media-enter-${media.animation.enter}`);
      }
    }
    const vol = media.volume / 100;
    if (videoRef.current) { videoRef.current.volume = vol; void videoRef.current.play(); }
    if (audioRef.current) { audioRef.current.volume = vol; void audioRef.current.play(); }
    setPlaying(true);
  };

  // --- как в overlay.html: createAnimatedText / createMediaWithText ---
  const t = media.text;
  const hasText = t.enabled && !!t.content;
  // Как на оверлее: подставляем переменные (тестовые значения).
  const sampleText = substituteSample(t.content);
  const textStyle: React.CSSProperties = {
    ...(t.font.fontFamily ? { fontFamily: t.font.fontFamily } : {}),
    ...(t.font.fontSize ? { fontSize: `${t.font.fontSize}px` } : {}),
    ...(t.font.fontWeight ? { fontWeight: t.font.fontWeight } : {}),
    ...(t.font.fontStyle ? { fontStyle: t.font.fontStyle } : {}),
    ...(t.font.color ? { color: t.font.color } : {}),
  };
  const textEl = hasText && (
    <div className="text-element" style={textStyle}>
      {playing && t.animation !== "none"
        ? sampleText.split("").map((ch, i) => <span key={i} className={`char char-anim-${t.animation}`} style={{ animationDelay: `${i * 0.05}s`, ["--amp" as string]: t.animationAmplitude }}>{ch}</span>)
        : sampleText}
    </div>
  );
  const mediaEl = video
    ? <video ref={videoRef} src={video} className="media-element video" playsInline onEnded={stop} style={media.chromakey !== "none" ? { mixBlendMode: media.chromakey as React.CSSProperties["mixBlendMode"] } : undefined} />
    : image
      ? <img src={image} alt="" className="media-element image" style={media.chromakey !== "none" ? { mixBlendMode: media.chromakey as React.CSSProperties["mixBlendMode"] } : undefined} />
      : null;
  const pos = hasText ? t.position : "overlay";
  const wrapperClass = `media-wrapper ${hasText ? "has-text" : "media-only"} position-${pos}`;
  const textFirst = pos === "above" || pos === "left";

  return (
    <div className="media-preview-section">
      <div className="preview-header">
        <h4><Icon name="eye" /> Предпросмотр <Tooltip text="Та же вёрстка и CSS, что на странице оверлея (холст 1920×1080). Клетчатый фон = прозрачность." /></h4>
        <button onClick={() => setShow((s) => !s)} className={`preview-toggle-btn ${show ? "active" : ""}`}>{show ? "Скрыть" : "Показать"}</button>
      </div>
      {show && (
        <div className="preview-container">
          <div ref={stageRef} className="preview-stage">
            <div style={{ position: "absolute", top: 0, left: 0, width: CANVAS_W, height: CANVAS_H, transform: `scale(${scale})`, transformOrigin: "top left" }}>
              <div ref={wrapRef} className={wrapperClass}>
                {!mediaEl && !hasText && <div className="audio-only-placeholder"><Icon name="audio-note" /> Аудио: {media.file}</div>}
                {textFirst && textEl}
                {mediaEl}
                {!textFirst && textEl}
              </div>
            </div>
            {audio && <audio ref={audioRef} src={audio} onEnded={stop} />}
          </div>
          <div className="preview-controls">
            {playing ? <button onClick={stop} className="preview-stop-btn"><Icon name="stop"  /> Остановить</button> : <button onClick={play} className="preview-play-btn"><Icon name="play"  /> Воспроизвести</button>}
          </div>
        </div>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ редактор

export default function MediaEditor({ value, onChange, overlays, fullResponse }: { value: MediaResponse; onChange: (m: MediaResponse) => void; overlays: Overlay[]; fullResponse?: Response }) {
  const { showNotification } = useNotification();
  const { files, reload } = useMediaFiles();
  const m = { ...defaultMedia(), ...value };
  const set = (patch: Partial<MediaResponse>) => onChange({ ...m, ...patch });

  const primaryKind = fileKind(m.file);
  const secondaryKind = secondaryKindFor(primaryKind);
  const showAnimations = primaryKind === "video" || primaryKind === "image" || (primaryKind === "audio" && !!m.secondaryFile);
  const showDuration = primaryKind === "image" || (primaryKind === "audio" && !!m.secondaryFile);

  const probe = async (name: string) => {
    try {
      const r = await api.mediaProbe(name);
      if (r.warnings.length) showNotification(`${r.warnings[0]}`, NOTIFICATION_TYPES.WARNING, 6000);
    } catch { /* не критично */ }
  };
  const selectPrimary = (name: string) => {
    const k = fileKind(name);
    const patch: Partial<MediaResponse> = { file: name, enabled: true };
    if (k === "video" || (m.secondaryFile && fileKind(m.secondaryFile) !== secondaryKindFor(k))) patch.secondaryFile = "";
    set(patch);
    void probe(name);
  };
  const deleteFile = async (name: string) => {
    try {
      await api.mediaDelete(name);
      showNotification(`Файл "${name}" удалён`, NOTIFICATION_TYPES.WARNING, 2000);
      const patch: Partial<MediaResponse> = {};
      if (m.file === name) { patch.file = ""; patch.secondaryFile = ""; }
      if (m.secondaryFile === name) patch.secondaryFile = "";
      if (Object.keys(patch).length) set(patch);
      await reload();
    } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR); }
  };
  const testOnOverlay = async () => {
    try {
      const sent = await api.mediaTest({ chat: { enabled: false, components: [] }, media: { ...m, enabled: true }, ...(fullResponse ? {} : {}) });
      showNotification(sent ? "Отправлено на оверлей" : "Оверлей не подключён — медиа в очереди", sent ? NOTIFICATION_TYPES.SUCCESS : NOTIFICATION_TYPES.WARNING, 2500);
    } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR); }
  };

  const [fontMode, setFontMode] = useState<"preset" | "custom">(() =>
    m.text.font.fontFamily && !FONT_FAMILIES.some((f) => f.value === m.text.font.fontFamily) ? "custom" : "preset");
  const setText = (patch: Partial<MediaResponse["text"]>) => set({ text: { ...m.text, ...patch } });
  const setFont = (patch: Partial<MediaResponse["text"]["font"]>) => setText({ font: { ...m.text.font, ...patch } });
  const setAnim = (patch: Partial<MediaResponse["animation"]>) => set({ animation: { ...m.animation, ...patch } });

  return (
    <div className="media-editor">
      <FileSelector
        label={<span style={{ display: "flex", alignItems: "center", gap: 8 }}><Icon name="clapperboard" /> Медиа файл <Tooltip text="Видео, аудио или картинка. Тип определяется по содержимому файла при добавлении." /></span>}
        selected={m.file} accept={null} files={files}
        onSelect={selectPrimary} onClear={() => set({ file: "", secondaryFile: "" })} onDelete={deleteFile} onImported={reload}
      />
      {m.file && secondaryKind && (
        <div className="secondary-file-section">
          <FileSelector
            label={<span style={{ display: "flex", alignItems: "center", gap: 8 }}>{secondaryKind === "audio" ? <><Icon name="audio-note" /> Звук (дополнительно)</> : <><Icon name="image" /> Картинка (дополнительно)</>}
              <Tooltip text={secondaryKind === "audio" ? "Звуковое сопровождение к картинке" : "Картинка, показываемая вместе со звуком"} /></span>}
            selected={m.secondaryFile} accept={secondaryKind} files={files}
            onSelect={(n) => { set({ secondaryFile: n }); void probe(n); }} onClear={() => set({ secondaryFile: "" })} onDelete={deleteFile} onImported={reload}
          />
        </div>
      )}

      <div className="overlay-selector">
        <label><Icon name="overlay-screen" /> Целевой оверлей <Tooltip text="Конкретный оверлей или все сразу (тогда медиа сыграет на каждом подключённом оверлее)." /></label>
        <select value={m.overlay ?? ""} onChange={(e) => set({ overlay: e.target.value || null })} className="overlay-select">
          <option value="">Все оверлеи</option>
          {overlays.map((o) => <option key={o.id} value={o.id}>{o.name} (/overlay/{o.path})</option>)}
        </select>
        {!m.overlay && overlays.length > 1 && (
          <div className="form-hint text-warning">Оверлей не выбран: копия медиа уйдёт на каждый из {overlays.length}. Обычно нужен один — выберите его в списке.</div>
        )}
      </div>

      <div className="media-settings">
        <label><Icon name="volume-on" /> Громкость <Tooltip text="Громкость воспроизведения (0–100%)" /></label>
        <div className="volume-control">
          <input type="range" min={0} max={100} value={m.volume} onChange={(e) => set({ volume: parseInt(e.target.value) })} />
          <span className="volume-value">{m.volume}%</span>
          <Icon name="volume-on" className="volume-icon" />
        </div>
      </div>

      <div className="media-settings">
        <label><Icon name="clapperboard" /> Режим воспроизведения <Tooltip text="«В очереди» — ждёт окончания других медиа. «Вне очереди» — играет сразу поверх всего." /></label>
        <div className="position-buttons">
          <button type="button" className={`position-btn ${m.queueMode === "queue" ? "active" : ""}`} onClick={() => set({ queueMode: "queue" })}><Icon name="clipboard" /> В очереди</button>
          <button type="button" className={`position-btn ${m.queueMode === "immediate" ? "active" : ""}`} onClick={() => set({ queueMode: "immediate" })}><Icon name="lightning" /> Вне очереди</button>
        </div>
      </div>

      {(primaryKind === "video" || primaryKind === "image") && (
        <div className="media-settings">
          <label><Icon name="chroma-key" /> Наложение (chromakey) <Tooltip text="CSS mix-blend-mode для видео/картинки: «screen» убирает чёрный фон, «multiply» — белый. Настоящий хромакей лучше делать фильтром в OBS." /></label>
          <select value={m.chromakey} onChange={(e) => set({ chromakey: e.target.value })} className="overlay-select">
            {[["none", "— Нет —"], ["screen", "screen (прозрачный чёрный)"], ["lighten", "lighten"], ["multiply", "multiply (прозрачный белый)"], ["darken", "darken"], ["difference", "difference"]].map(([v, l]) => <option key={v} value={v}>{l}</option>)}
          </select>
        </div>
      )}

      {showDuration && (
        <div className="media-settings">
          <label><Icon name="stopwatch" /> Длительность показа картинки <Tooltip text="Секунды. Пусто — из общих настроек оверлея. Картинка со звуком показывается не меньше длины звука." /></label>
          <div className="volume-control">
            <input type="number" min={1} max={600} step={0.5} value={m.imageDurationSec ?? ""} placeholder="по умолчанию" onChange={(e) => set({ imageDurationSec: e.target.value === "" ? null : Math.max(0.5, parseFloat(e.target.value) || 0) })} style={{ width: 140 }} />
            <span className="volume-value">сек</span>
          </div>
        </div>
      )}

      {showAnimations && (
        <div className="media-animation-section">
          <h4><Icon name="animation-masks" /> Анимации медиа</h4>
          <div className="animation-row">
            <div className="animation-select-group">
              <label>Появление</label>
              <select value={m.animation.enter} onChange={(e) => setAnim({ enter: e.target.value })} className="animation-select">
                {MEDIA_ENTER_ANIMATIONS.map((a) => <option key={a.value} value={a.value}>{a.label}</option>)}
              </select>
            </div>
            <div className="animation-select-group">
              <label>Скрытие</label>
              <select value={m.animation.exit} onChange={(e) => setAnim({ exit: e.target.value })} className="animation-select">
                {MEDIA_EXIT_ANIMATIONS.map((a) => <option key={a.value} value={a.value}>{a.label}</option>)}
              </select>
            </div>
          </div>
          <div className="animation-row duration-row">
            <div className="animation-speed-group">
              <label>Длительность появления: {m.animation.enterDuration.toFixed(1)}с</label>
              <input type="range" min={0.1} max={2} step={0.1} value={m.animation.enterDuration} disabled={m.animation.enter === "none"} onChange={(e) => setAnim({ enterDuration: parseFloat(e.target.value) })} className="speed-slider" />
            </div>
            <div className="animation-speed-group">
              <label>Длительность скрытия: {m.animation.exitDuration.toFixed(1)}с</label>
              <input type="range" min={0.1} max={2} step={0.1} value={m.animation.exitDuration} disabled={m.animation.exit === "none"} onChange={(e) => setAnim({ exitDuration: parseFloat(e.target.value) })} className="speed-slider" />
            </div>
          </div>
        </div>
      )}

      <div className="media-text-section">
        <div className="section-header">
          <label className="toggle-label">
            <input type="checkbox" checked={m.text.enabled} onChange={(e) => setText({ enabled: e.target.checked })} className="toggle-checkbox" />
            <span className="toggle-text"><Icon name="edit" /> Показывать текст на оверлее</span>
            <Tooltip text="Текст поверх медиа или рядом. Переменные подставляются." />
          </label>
        </div>
        {m.text.enabled && (
          <div className="text-settings">
            <div className="text-vars-block">
              <div className="text-vars-label"><span><Icon name="pin" /> Доступные переменные:</span></div>
              <div className="text-vars-badges"><VariableBadges variables={["user", "target", "message"]} /></div>
            </div>
            <div className="text-input-group">
              <label>Текст</label>
              <textarea value={m.text.content} onChange={(e) => setText({ content: e.target.value })} placeholder="Текст для отображения... {user} — имя пользователя" rows={3} className="text-content-input" />
            </div>
            <div className="position-selector">
              <label>Позиция текста</label>
              <div className="position-buttons">
                {([["above", "arrow-up", "Сверху"], ["below", "arrow-down", "Снизу"], ["left", "arrow-left", "Слева"], ["right", "arrow-right", "Справа"], ["overlay", "target", "Поверх"]] as const).map(([p, ic, l]) => (
                  <button key={p} type="button" className={`position-btn ${m.text.position === p ? "active" : ""}`} onClick={() => setText({ position: p })}><Icon name={ic} /> {l}</button>
                ))}
              </div>
            </div>
            <div className="text-animation-selector">
              <label><Icon name="animation-masks" /> Анимация текста</label>
              <div className="animation-select-row">
                <select value={m.text.animation} onChange={(e) => setText({ animation: e.target.value })} className="animation-select">
                  {TEXT_ANIMATIONS.map((a) => <option key={a.value} value={a.value}>{a.label}</option>)}
                </select>
                {m.text.animation !== "none" && (
                  <div className="amplitude-control">
                    <label>Сила анимации</label>
                    <input type="range" min={0.3} max={2} step={0.1} value={m.text.animationAmplitude} onChange={(e) => setText({ animationAmplitude: parseFloat(e.target.value) })} className="amplitude-slider" />
                    <span className="amplitude-value">{m.text.animationAmplitude.toFixed(1)}x</span>
                  </div>
                )}
              </div>
            </div>
            <div className="font-settings">
              <h4><Icon name="typography" /> Настройки шрифта</h4>
              <div className="font-settings-grid">
                <div className="font-setting-item">
                  <label>Режим выбора шрифта</label>
                  <div className="font-mode-buttons">
                    <button type="button" className={`font-mode-btn ${fontMode === "preset" ? "active" : ""}`} onClick={() => setFontMode("preset")}><Icon name="clipboard" /> Из списка</button>
                    <button type="button" className={`font-mode-btn ${fontMode === "custom" ? "active" : ""}`} onClick={() => setFontMode("custom")}><Icon name="edit" /> Свой шрифт</button>
                  </div>
                </div>
                {fontMode === "preset" ? (
                  <div className="font-setting-item">
                    <label>Семейство</label>
<FontPicker value={m.text.font.fontFamily ?? FONT_FAMILIES[0].value} onChange={(v) => setFont({ fontFamily: v })} />
                  </div>
                ) : (
                  <div className="font-setting-item">
                    <label>Название шрифта</label>
                    <input type="text" value={(m.text.font.fontFamily ?? "").replace(/'/g, "").replace(/, sans-serif$/, "")} onChange={(e) => setFont({ fontFamily: e.target.value ? `'${e.target.value}', sans-serif` : undefined })} placeholder="например: Roboto, Montserrat" className="custom-font-input" />
                  </div>
                )}
                <div className="font-setting-item">
                  <label>Размер: {m.text.font.fontSize ?? 32}px</label>
                  <input type="range" min={12} max={120} value={m.text.font.fontSize ?? 32} onChange={(e) => setFont({ fontSize: parseInt(e.target.value) })} />
                </div>
                <div className="font-setting-item">
                  <label>Цвет текста</label>
                  <div className="color-input-group">
                    <input type="color" value={m.text.font.color ?? "#ffffff"} onChange={(e) => setFont({ color: e.target.value })} className="color-picker" />
                    <input type="text" value={m.text.font.color ?? "#ffffff"} onChange={(e) => setFont({ color: e.target.value })} className="color-text-input" placeholder="#ffffff" />
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      {!m.file && <p className="media-no-file-hint">Выберите медиафайл — появятся предпросмотр и «Тест на оверлее». Без файла реакция на оверлей не отправляется.</p>}
      {m.file && (
        <>
          <MediaPreview media={m} />
          <div className="preview-controls" style={{ marginTop: 8 }}>
            <button onClick={testOnOverlay} className="preview-play-btn" title="Отправить это медиа на оверлей прямо сейчас"><Icon name="play"  /> Тест на оверлее</button>
          </div>
        </>
      )}
    </div>
  );
}
