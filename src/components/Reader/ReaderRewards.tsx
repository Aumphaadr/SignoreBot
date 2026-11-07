import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { api, errText, type ChannelReward, type Reward } from "../../api";
import { useAppState } from "../../state/AppState";
import { NOTIFICATION_TYPES, useNotification } from "../Notification";
import Icon from "../Icon";
import { readerRewardItems, rewardNeedsInput, type ReaderRewardItem } from "./reader";
import RewardPicture from "./RewardPicture";
import "./ReaderRewards.css";

function rewardStyle(color: string | undefined): CSSProperties {
  const bg = color && /^#[\da-f]{6}$/i.test(color) ? color : "#3f3558";
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(bg.slice(i, i + 2), 16));
  return { "--reward-background": bg, "--reward-foreground": r * .299 + g * .587 + b * .114 > 160 ? "#17151e" : "#fff" } as CSSProperties;
}
function RewardTile({ item, disabled, busy, onRun, onConfigure }: { item: ReaderRewardItem; disabled: boolean; busy: boolean; onRun: (input: string) => void; onConfigure: () => void }) {
  const { reaction: reward, channel } = item;
  const [input, setInput] = useState("");
  const title = channel?.title || reward?.rewardTitle || "Награда";
  const runnable = !!reward?.enabled;
  const needsInput = rewardNeedsInput(reward, channel);
  return <form className="reader-reward-tile" style={rewardStyle(channel?.backgroundColor)} data-reward-id={reward?.id ?? item.id} aria-busy={busy} onSubmit={(e) => { e.preventDefault(); if (!disabled && runnable) onRun(input); }}>
    <button className="reader-reward-art" type="submit" disabled={disabled || !runnable} aria-label={`Запустить: ${title}`} title={channel?.prompt || title}>
      <RewardPicture url={channel?.image} /><strong>{title}</strong>
    </button>
    {needsInput && <input className="reader-reward-input" aria-label={`Текст награды: ${title}`} placeholder={channel?.prompt || "Введите текст"} title={channel?.prompt || "Текст для реакции"} value={input} maxLength={500} required={!!channel?.requiresInput} disabled={disabled || !runnable} onChange={(e) => setInput(e.target.value)} />}
    <div className="reader-reward-bottom"><span className="reader-reward-cost" title={channel ? `${title}: стоимость на Twitch — ${channel.cost} баллов` : "Стоимость на Twitch пока неизвестна"}><Icon name="channel-points" />{channel ? channel.cost.toLocaleString("ru-RU") : "—"}</span>{runnable ? <button type="submit" disabled={disabled} className="primary small">{busy ? "Запускается…" : "Запустить"}</button> : <button type="button" className="small" onClick={onConfigure} title={reward ? "Реакция SignoreBot выключена" : "Для этой награды нет реакции SignoreBot"}>Настроить</button>}</div>
  </form>;
}
export default function ReaderRewards({ onConfigure }: { onConfigure: () => void }) {
  const { config, status, onChanged } = useAppState();
  const { showNotification } = useNotification();
  const [channel, setChannel] = useState<ChannelReward[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const mounted = useRef(false);
  const revision = useRef(0);
  const inFlight = useRef(false);
  const refresh = useCallback(async () => {
    const ticket = ++revision.current; setLoading(true);
    try { const next = await api.rewardsChannel(); if (mounted.current && ticket === revision.current) { setChannel(next); setError(""); } }
    catch (e) { if (mounted.current && ticket === revision.current) setError(errText(e)); }
    finally { if (mounted.current && ticket === revision.current) setLoading(false); }
  }, []);
  useEffect(() => {
    mounted.current = true; void refresh();
    const stop = onChanged("rewards", () => void refresh());
    return () => { mounted.current = false; revision.current++; stop(); };
  }, [onChanged, refresh]);
  const run = async (reward: Reward, input: string) => {
    if (inFlight.current) return;
    inFlight.current = true; setBusy(reward.id);
    try {
      const login = status?.broadcaster.login ?? "";
      const outcome = await api.responseTest(reward.response, { user: login, username: login, message: input, input });
      if (outcome.mediaUnavailable) throw new Error(outcome.chatSent ? "Сообщение отправлено, но оверлей недоступен: медиа не доставлено" : "Оверлей недоступен: медиа не доставлено");
      if (!outcome.chatSent && !outcome.mediaSent) throw new Error("Реакция не выполнена. Проверьте её настройки и подключение");
      showNotification(`Запущено: ${reward.rewardTitle}`, NOTIFICATION_TYPES.SUCCESS, 2500);
    } catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 6000); }
    finally { inFlight.current = false; if (mounted.current) setBusy(null); }
  };
  const rewards = readerRewardItems(channel, config.rewards);
  return <div className="reader-rewards">
    <p className="reader-rewards-note">Тестовый запуск реакции SignoreBot. Погашение на Twitch не создаётся, баллы не списываются.</p>
    {loading && <p className="reader-rewards-note"><Icon name="loader" className="spinning" /> Загрузка наград Twitch…</p>}
    {error && <div className="reader-rewards-error" role="status">Не удалось загрузить оформление с Twitch: {error} <button className="small" onClick={() => void refresh()}>Повторить</button></div>}
    {rewards.length ? <div className="reader-rewards-grid">{rewards.map((item) => <RewardTile key={item.id} item={item} disabled={loading || !!busy || !status?.running} busy={!!item.reaction && busy === item.reaction.id} onRun={(input) => { if (item.reaction) void run(item.reaction, input); }} onConfigure={onConfigure} />)}</div> : <p className="reader-rewards-note">На канале нет доступных наград.</p>}
    <div className="reader-rewards-footer"><button onClick={() => void api.rewardsQueueUrl().then((url) => import("@tauri-apps/plugin-opener").then(({ openUrl }) => openUrl(url))).catch((e) => showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 5000))}><Icon name="external-link" /> Очередь запросов Twitch</button></div>
  </div>;
}
