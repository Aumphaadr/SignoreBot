import { Fragment, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { api, errText, type BadgeImage, type ChannelReward, type ChatEntry, type ChatHistory, type ChatReaderSettings } from "../../api";
import { useAppState } from "../../state/AppState";
import { NOTIFICATION_TYPES, useNotification } from "../Notification";
import Icon from "../Icon";
import Modal from "../Common/Modal";
import TwitchPopout from "./TwitchPopout";
import ReaderRewards from "./ReaderRewards";
import RewardPicture from "./RewardPicture";
import { forUs, nicknameColor, tracked } from "./reader";
import "./ReaderTab.css";

function ReaderToggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return <label className="toggle-label"><span className="toggle-switch"><input type="checkbox" aria-label={label} checked={checked} onChange={(e) => onChange(e.target.checked)} /><span className="toggle-slider" /></span><span className="toggle-text">{label}</span></label>;
}
function Emote({ url, text }: { url: string; text: string }) {
  const [failed, setFailed] = useState(false);
  return failed ? <>{text}</> : <img className="reader-emote" src={url} alt={text} title={text} referrerPolicy="no-referrer" onError={() => setFailed(true)} />;
}
function MessageText({ entry, streamerId, highlight }: { entry: ChatEntry; streamerId: string | null; highlight: boolean }) {
  if (entry.deleted) return <span className="reader-deleted">Сообщение удалено</span>;
  if (!entry.fragments.length) return <>{entry.text}</>;
  return <>{entry.fragments.map((f, i) => <Fragment key={i}>{f.imageUrl ? <Emote url={f.imageUrl} text={f.text} /> : f.userId ? <span className={highlight && f.userId === streamerId ? "reader-mention own" : "reader-mention"}>{f.text}</span> : f.text}</Fragment>)}</>;
}
const empty: ChatHistory = { channelId: "", generation: 0, revision: 0, entries: [] };
const modes = [["read", "eye", "Только чтение"], ["moderation", "sword", "С модерацией"], ["twitch", "globe", "Twitch Popout"]] as const;

export default function ReaderTab({ active, goTo }: { active: boolean; goTo: (tab: string) => void }) {
  const { config, status, updateSection, onChanged } = useAppState();
  const prefs = config.chatReader;
  const { showNotification } = useNotification();
  const [history, setHistory] = useState<ChatHistory>(empty);
  const [loadError, setLoadError] = useState("");
  const [badges, setBadges] = useState(new Map<string, BadgeImage>());
  const [rewardCatalog, setRewardCatalog] = useState(new Map<string, ChannelReward>());
  const [settings, setSettings] = useState(false);
  const [ignoredDraft, setIgnoredDraft] = useState(prefs.ignoredLogins.join(", "));
  const [rewards, setRewards] = useState(false);
  const [onlyUnread, setOnlyUnread] = useState(false);
  const [following, setFollowing] = useState(true);
  const followingRef = useRef(following); followingRef.current = following;
  const [anchor, setAnchor] = useState("");
  const [message, setMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [moderation, setModeration] = useState<{ entry: ChatEntry; generation: number; action: "warn" | "timeout" | "ban" } | null>(null);
  const [duration, setDuration] = useState(600);
  const [reason, setReason] = useState("");
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState("");
  const list = useRef<HTMLDivElement>(null);
  const scroll = useRef(0);
  const scrollAnchor = useRef<{ id: string; offset: number } | null>(null);
  const refreshing = useRef(false);
  const rerun = useRef(false);
  const streamerId = status?.broadcaster.userId ?? null;
  const botId = status?.bot.userId ?? null;
  const login = status?.broadcaster.login ?? null;
  const mode = prefs.mode;
  const connected = !!status?.eventsub.connected;
  const modMode = mode === "moderation";
  const patch = (value: Partial<ChatReaderSettings>) => updateSection("chatReader", (p) => ({ ...p, ...value }));
  const fail = useCallback((e: unknown) => showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 6000), [showNotification]);

  const refresh = useCallback(async () => {
    rerun.current = true; if (refreshing.current) return;
    refreshing.current = true;
    do {
      rerun.current = false;
      try {
        const next = await api.chatHistory();
        setHistory((prev) => prev.generation === next.generation && prev.revision === next.revision ? prev : next);
        setLoadError("");
      } catch (e) { setLoadError(errText(e)); }
    } while (rerun.current);
    refreshing.current = false;
  }, []);
  useEffect(() => {
    void refresh(); let timer: ReturnType<typeof setTimeout> | undefined;
    const stop = onChanged("reader", () => { if (!timer) timer = setTimeout(() => { timer = undefined; void refresh(); }, 100); });
    // A missed event (e.g. a full IPC queue) must not leave stale history forever.
    const recovery = setInterval(() => void refresh(), 15000);
    return () => { stop(); clearTimeout(timer); clearInterval(recovery); };
  }, [onChanged, refresh]);
  useEffect(() => {
    setFollowing(prefs.autoScroll); followingRef.current = prefs.autoScroll; setOnlyUnread(false); scroll.current = 0; scrollAnchor.current = null; setAnchor("");
  }, [history.generation]); // a new channel is a new reading session
  useEffect(() => {
    let canceled = false; setBadges(new Map());
    if (!streamerId) return;
    const load = () => void api.chatBadges().then((sets) => {
      if (canceled) return; const map = new Map<string, BadgeImage>();
      for (const set of sets) for (const version of set.versions) map.set(`${set.setId}/${version.id}`, version);
      setBadges(map);
    }).catch(() => {}); // unavailable badge catalogs never break message rendering
    load(); const timer = setInterval(load, 30 * 60_000);
    return () => { canceled = true; clearInterval(timer); };
  }, [streamerId, status?.broadcaster.state]);
  useEffect(() => {
    let canceled = false, ticket = 0; setRewardCatalog(new Map());
    if (!streamerId) return;
    const load = async () => {
      const current = ++ticket;
      try { const rewards = await api.rewardsChannel(); if (!canceled && current === ticket) setRewardCatalog((old) => new Map([...old, ...rewards.map((r): [string, ChannelReward] => [r.id, r])])); }
      catch { /* The channel-points symbol remains available if the catalog cannot load. */ }
    };
    void load(); const stop = onChanged("rewards", () => void load());
    const timer = setInterval(() => void load(), 15 * 60_000);
    return () => { canceled = true; stop(); clearInterval(timer); };
  }, [streamerId, status?.broadcaster.state, onChanged]);
  const isTracked = (e: ChatEntry) => tracked(e, prefs, botId, streamerId);
  const unread = history.entries.filter((e) => !e.read && isTracked(e));
  const visible = useMemo(() => history.entries.filter((e) => (prefs.showEvents || !e.event) && (!onlyUnread || !prefs.tracking || (!e.read && tracked(e, prefs, botId, streamerId)))), [history, prefs, botId, streamerId, onlyUnread]);
  const tailId = history.entries[history.entries.length - 1]?.id ?? "";
  const rememberPosition = () => {
    const el = list.current; if (!el || !el.clientHeight) return;
    scroll.current = el.scrollTop;
    const top = el.getBoundingClientRect().top;
    const row = Array.from(el.querySelectorAll<HTMLElement>(".reader-row")).find((r) => r.getBoundingClientRect().bottom > top);
    scrollAnchor.current = row?.dataset.messageId ? { id: row.dataset.messageId, offset: row.getBoundingClientRect().top - top } : null;
  };
  const pause = () => {
    if (!followingRef.current) return;
    rememberPosition(); followingRef.current = false; setFollowing(false); setAnchor(tailId);
  };
  const latest = () => { scrollAnchor.current = null; followingRef.current = true; setFollowing(true); setAnchor(tailId); if (list.current) { list.current.scrollTop = list.current.scrollHeight; scroll.current = list.current.scrollTop; } };
  const restorePosition = useCallback(() => {
    const el = list.current;
    if (!active || mode === "twitch" || !el?.clientHeight) return;
    if (followingRef.current && prefs.autoScroll) { el.scrollTop = el.scrollHeight; return; }
    const anchor = scrollAnchor.current;
    const row = anchor && Array.from(el.querySelectorAll<HTMLElement>(".reader-row")).find((r) => r.dataset.messageId === anchor.id);
    if (row && anchor) el.scrollTop += row.getBoundingClientRect().top - el.getBoundingClientRect().top - anchor.offset;
    else el.scrollTop = scroll.current;
  }, [active, mode, prefs.autoScroll]);
  useLayoutEffect(restorePosition, [restorePosition, visible, following, prefs.fontSize, prefs.timestamps, prefs.badges]);
  useEffect(() => {
    const el = list.current?.firstElementChild; if (!el) return;
    const observer = new ResizeObserver(restorePosition);
    observer.observe(el); return () => observer.disconnect();
  }, [restorePosition]);
  const mark = async (ids: string[], read: boolean) => {
    if (!ids.length) return;
    const generation = history.generation;
    setHistory((h) => ({ ...h, entries: h.entries.map((e) => ids.includes(e.id) ? { ...e, read } : e) }));
    try { await api.chatMark(generation, ids, read); await refresh(); } catch (e) { fail(e); void refresh(); }
  };
  const executeModeration = async (entry: ChatEntry, generation: number, action: string, seconds: number | null, why: string) => {
    await api.chatModerate(generation, entry.id, action, seconds, why);
    showNotification(action === "delete" ? `Удалено сообщение ${entry.name}` : action === "warn" ? `Предупреждение отправлено ${entry.name}` : action === "timeout" ? `${entry.name}: мут на ${seconds} сек.` : `${entry.name} забанен`, NOTIFICATION_TYPES.SUCCESS, 2500);
  };
  const send = async () => {
    if (!message.trim() || sending) return;
    setSending(true); try { await api.chatReaderSend(message.trim()); setMessage(""); } catch (e) { fail(e); } finally { setSending(false); }
  };
  const hasDelete = status?.broadcaster.scopes.includes("moderator:manage:chat_messages");
  const streamerReady = status?.broadcaster.state === "authorized" && !!streamerId;
  const hasWrite = !!status?.broadcaster.scopes.includes("user:write:chat");
  const hasWarn = status?.broadcaster.scopes.includes("moderator:manage:warnings");
  const hasBan = status?.broadcaster.scopes.includes("moderator:manage:banned_users");
  const missingActions = [!hasDelete && "удаления сообщений", !hasWarn && "предупреждений", !hasBan && "мута и бана"].filter(Boolean).join(", ");
  const newCount = anchor ? Math.max(0, history.entries.length - 1 - history.entries.findIndex((e) => e.id === anchor)) : history.entries.length;
  const canModerate = (e: ChatEntry) => connected && !e.isReward && !e.rewardId && !!e.messageId && !e.deleted && !!e.userId && e.userId !== streamerId && !e.badges.some((b) => b.setId === "moderator" || b.setId === "broadcaster");
  const readButton = (e: ChatEntry) => prefs.tracking ? <button className={`reader-read ${e.read ? "done" : ""}`} disabled={!isTracked(e)} title={!isTracked(e) ? `${e.name || "Событие"}: не учитывается в непрочитанных` : `${e.read ? "Отметить непрочитанным" : "Прочитано"}: ${e.name || e.event}`} aria-label={`${e.read ? "Отметить непрочитанным" : "Прочитано"}: ${e.name || e.event}`} onClick={() => void mark([e.id], !e.read)}><Icon name={e.read ? "circle-check" : "check"} /></button> : null;

  return <section className="reader-tab">
    <header className="reader-header"><div><h2><Icon name="message-dots" /> Читать чат</h2><span className="reader-channel">{login ? `#${login}` : "Канал не выбран"} <span className={`reader-connection ${connected ? "online" : ""}`}>{connected ? "Подключено" : "Нет подключения к чату"}</span></span></div>{mode !== "twitch" && <button onClick={() => { setIgnoredDraft(prefs.ignoredLogins.join(", ")); setSettings(true); }} title="Настройки читалки" aria-label="Настройки читалки"><Icon name="gear-8" /></button>}</header>
    <div className="reader-modebar"><div className="reader-modes" role="group" aria-label="Режим чтения">{modes.map(([id, icon, label]) => <button key={id} className={mode === id ? "selected" : ""} aria-pressed={mode === id} onClick={() => patch({ mode: id })}><Icon name={icon} />{label}</button>)}</div>{mode !== "twitch" && <ReaderToggle label="Автопрокрутка" checked={prefs.autoScroll} onChange={(on) => { patch({ autoScroll: on }); if (on) latest(); else pause(); }} />}</div>
    <div hidden={mode === "twitch"} className="reader-custom">
      <div className="reader-toolbar"><div>{prefs.tracking && <><strong>{unread.length}</strong> непрочитанных <ReaderToggle label="Только непрочитанные" checked={onlyUnread} onChange={(on) => { pause(); setOnlyUnread(on); }} /><button disabled={!unread.length} onClick={() => void mark(unread.map((e) => e.id), true)}><Icon name="check" /> Прочитано всё</button></>}</div></div>
      {loadError && <div className="reader-warning">Не удалось получить ленту: {loadError} <button onClick={() => void refresh()}>Повторить</button></div>}
      {modMode && missingActions && <div className="reader-warning">Для {missingActions} заново войдите в аккаунт стримера. <button onClick={() => goTo("auth")}>Авторизация</button></div>}
      <div className={`reader-feed-shell ${modMode ? "moderation" : ""}`} style={{ "--reader-font-size": `${prefs.fontSize}px` } as CSSProperties}>
        <div className="reader-feed" ref={list} tabIndex={0} aria-label="Сообщения чата" onWheel={(e) => { if (e.deltaY < 0) pause(); }} onKeyDown={(e) => { if (["ArrowUp", "PageUp", "Home"].includes(e.key)) pause(); }} onScroll={(e) => {
          const el = e.currentTarget;
          if (!active || mode === "twitch" || !el.clientHeight) return;
          if (el.scrollHeight - el.clientHeight - el.scrollTop > 60) pause();
          if (!followingRef.current || !prefs.autoScroll) rememberPosition();
        }} onPointerDown={(e) => { if ((e.target as HTMLElement).closest("button")) pause(); }}>
          <div className="reader-rows">{!visible.length ? <div className="reader-empty"><Icon name="message-dots" size={38} /><h3>{onlyUnread ? "Всё прочитано" : "Здесь появится чат"}</h3><p>{onlyUnread ? "Новые сообщения останутся непрочитанными, пока вы сами их не отметите." : connected ? "Лента собирается с момента подключения SignoreBot к каналу." : "Авторизуйте стримера и бота. Сообщения начнут появляться после подключения."}</p></div> : visible.map((e) => {
            const own = prefs.highlightOwn && forUs(e, streamerId);
            const isReward = e.isReward || !!e.rewardId;
            const reward = rewardCatalog.get(e.rewardId);
            return <article key={e.id} data-message-id={e.id} className={`reader-row ${prefs.tracking && !e.read && isTracked(e) ? "unread" : ""} ${own ? "for-us" : ""} ${e.event ? "event" : ""} ${e.deleted ? "deleted" : ""}`}>
              {modMode && isReward ? <div className="reader-mod-buttons reader-reward-slot"><span className="reader-reward-marker" role="img" aria-label={`Награда: ${reward?.title || e.event}`} title={reward ? `${reward.title} · ${reward.cost} баллов` : e.event} style={reward && /^#[\da-f]{6}$/i.test(reward.backgroundColor) ? { backgroundColor: reward.backgroundColor } : undefined}><RewardPicture url={reward?.image} size={32} /></span></div> : modMode ? <div className="reader-mod-buttons" onPointerEnter={pause}>{(e.messageId && (e.text || e.deleted)) ? ([
                ["delete", "trash", "Удалить сообщение", hasDelete], ["warn", "triangle-alert", "Предупредить", hasWarn], ["timeout", "clock", "Выдать мут", hasBan], ["ban", "ban", "Забанить", hasBan],
              ] as const).map(([action, icon, label, permission]) => <button key={action} disabled={!canModerate(e) || !permission} title={`${label}: ${e.name || "Событие"}${!permission ? ". Заново войдите в аккаунт стримера" : ""}`} aria-label={`${label}: ${e.name || "Событие"}`} onClick={() => {
                if (action === "delete") void executeModeration(e, history.generation, action, null, "").catch(fail);
                else { setModeration({ entry: { ...e }, generation: history.generation, action }); setReason(""); setActionError(""); }
              }}><Icon name={icon} /></button>) : null}</div> : readButton(e)}
              <div className="reader-message-body">
                {e.event && <div className="reader-event-label"><Icon name={e.rewardId ? "channel-points" : "party-popper"} />{e.rewardId && e.event === "Награда за баллы канала" ? `Награда · ${config.rewards.find((r) => r.rewardId === e.rewardId)?.rewardTitle || "Баллы канала"}` : e.event}</div>}
                {e.reply && <div className={`reader-reply ${prefs.highlightOwn && e.reply.userId === streamerId ? "own" : ""}`} title={e.reply.deleted ? "Исходное сообщение удалено" : `Ответ ${e.reply.name}: ${e.reply.text}`}><Icon name="message-reply" /><b>{e.reply.name}</b><span>{e.reply.deleted ? "Сообщение удалено" : e.reply.text}</span></div>}
                <div className="reader-message">{prefs.timestamps && <time className="reader-time" dateTime={new Date(e.timestamp).toISOString()}>{new Date(e.timestamp).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })}</time>}{prefs.badges && e.badges.map((badge) => {
                  const image = badges.get(`${badge.setId}/${badge.id}`);
                  return image ? <img key={`${badge.setId}/${badge.id}`} className="reader-badge" src={image.imageUrl2x} title={image.title} alt={image.title} referrerPolicy="no-referrer" /> : <span className="reader-badge-fallback" key={`${badge.setId}/${badge.id}`} title={`${badge.setId} ${badge.id}`}><Icon name={badge.setId === "moderator" ? "sword" : badge.setId === "broadcaster" ? "star" : "user"} /></span>;
                })}{e.name && <strong className="reader-nickname" style={{ color: nicknameColor(e.color) }} title={e.login ? `@${e.login}` : undefined}>{e.name}{e.text || e.deleted ? ":" : ""}</strong>} <MessageText entry={e} streamerId={streamerId} highlight={prefs.highlightOwn} /></div>
              </div>{modMode && readButton(e)}
            </article>;
          })}</div>
        </div>
        {(!following || !prefs.autoScroll) && <button className="reader-jump" onClick={latest}><Icon name="arrow-down" /> К последним сообщениям{newCount > 0 ? ` · +${newCount}` : ""}</button>}
      </div>
      <footer className="reader-footer"><span>{!prefs.autoScroll ? "Ручная прокрутка" : following ? "Показываются последние сообщения" : "Прокрутка приостановлена"} <span className="reader-info" title="Последние 500 записей текущего сеанса хранятся в памяти. Прокрутка не отмечает сообщения прочитанными."><Icon name="circle-question" /></span></span>{modMode && <button onClick={() => setRewards(true)}><Icon name="channel-points" /> Награды за баллы</button>}</footer>
      {streamerReady && !hasWrite && <div className="reader-warning">Для отправки от своего имени заново войдите в аккаунт стримера. <button onClick={() => goTo("auth")}>Авторизация</button></div>}
      <form className="reader-compose" onSubmit={(e) => { e.preventDefault(); void send(); }}><input aria-label="Сообщение в чат" placeholder={login ? `Написать от ${login}` : "Сообщение в чат"} value={message} maxLength={500} onChange={(e) => setMessage(e.target.value)} disabled={!streamerReady || !hasWrite || sending} /><button className="primary" title="Отправить сообщение" aria-label="Отправить сообщение" disabled={!message.trim() || !streamerReady || !hasWrite || sending}><Icon name="send" /></button></form>
    </div>
    {mode === "twitch" && <TwitchPopout active={active} login={login} zoom={config.app.uiZoom} />}

    <Modal isOpen={settings} onClose={() => setSettings(false)} title={<><Icon name="gear-8" /> Настройки читалки</>}>
      <div className="reader-settings">
        <label>Размер текста <input type="range" min={14} max={32} value={prefs.fontSize} onChange={(e) => patch({ fontSize: Number(e.target.value) })} /><output>{prefs.fontSize} px</output></label>
        {([
          ["badges", "Показывать значки Twitch"], ["timestamps", "Показывать время сообщений"], ["showEvents", "Показывать события канала"], ["highlightOwn", "Выделять упоминания стримера и ответы ему"], ["tracking", "Отмечать прочитанные сообщения"], ["ignoreOwnBot", "Не считать сообщения своего бота непрочитанными"],
        ] as const).map(([key, label]) => <ReaderToggle key={key} label={label} checked={prefs[key]} onChange={(on) => patch({ [key]: on })} />)}
        <label className="reader-ignored">Не считать непрочитанными сообщения от<textarea rows={3} value={ignoredDraft} placeholder="Ники ботов через запятую" onChange={(e) => { setIgnoredDraft(e.target.value); patch({ ignoredLogins: e.target.value.split(/[,\n]/).map((s) => s.trim().replace(/^@/, "")).filter(Boolean) }); }} /><small>Ники через запятую. Их сообщения останутся в ленте.</small></label>
      </div>
    </Modal>
    <Modal isOpen={!!moderation} onClose={() => { if (!pending) setModeration(null); }} title={moderation?.action === "warn" ? "Предупредить пользователя" : moderation?.action === "ban" ? "Забанить пользователя" : "Выдать мут"} size="small">
      {moderation && <div className="reader-action"><strong>{moderation.entry.name} <small>@{moderation.entry.login}</small></strong><blockquote>{moderation.entry.text}</blockquote>{moderation.action === "timeout" && <label>Длительность <select value={duration} onChange={(e) => setDuration(Number(e.target.value))}><option value={60}>1 минута</option><option value={600}>10 минут</option><option value={1800}>30 минут</option><option value={3600}>1 час</option><option value={86400}>1 день</option></select></label>}<label>{moderation.action === "warn" ? "Причина предупреждения" : "Причина (необязательно)"}<input aria-label="Причина модерации" required={moderation.action === "warn"} maxLength={500} value={reason} onChange={(e) => setReason(e.target.value)} /></label>{moderation.action === "warn" && <small>Пользователь сможет писать в чат после подтверждения предупреждения.</small>}{actionError && <p role="alert" className="reader-action-error">{actionError}</p>}<div className="reader-action-buttons"><button disabled={pending} onClick={() => setModeration(null)}>Отмена</button><button className={moderation.action === "warn" ? "warning" : "danger"} disabled={pending || (moderation.action === "warn" && !reason.trim())} onClick={() => {
        setPending(true); setActionError(""); void executeModeration(moderation.entry, moderation.generation, moderation.action, moderation.action === "timeout" ? duration : null, reason).then(() => setModeration(null)).catch((e) => setActionError(errText(e))).finally(() => setPending(false));
      }}>{pending ? "Отправка…" : moderation.action === "warn" ? `Предупредить ${moderation.entry.name}` : moderation.action === "ban" ? `Забанить ${moderation.entry.name}` : `Выдать мут ${moderation.entry.name}`}</button></div></div>}
    </Modal>
    <Modal isOpen={rewards} onClose={() => setRewards(false)} title={<><Icon name="channel-points" /> Награды за баллы</>} size="large">
      <ReaderRewards onConfigure={() => { setRewards(false); goTo("rewards"); }} />
    </Modal>
  </section>;
}
