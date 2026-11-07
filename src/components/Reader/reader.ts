import type { ChatEntry, ChatReaderSettings } from "../../api";

/** Unread exclusions never hide messages. Shared-account manual messages count normally. */
export function tracked(e: ChatEntry, prefs: ChatReaderSettings, botId: string | null, streamerId: string | null) {
  if (e.deleted || (!prefs.showEvents && e.event)) return false;
  if (prefs.ignoredLogins.some((s) => s.toLowerCase() === e.login.toLowerCase())) return false;
  if (prefs.ignoreOwnBot && (e.sentByBot || (botId && botId !== streamerId && e.userId === botId))) return false;
  return true;
}
export function forUs(e: ChatEntry, streamerId: string | null) {
  return !!streamerId && (e.reply?.userId === streamerId || e.fragments.some((f) => f.userId === streamerId));
}
/** Twitch permits almost-black nickname colors; keep nicknames readable on our dark background. */
export function nicknameColor(color: string) {
  if (!/^#[\da-f]{6}$/i.test(color)) return "#c3bafa";
  const channels = [1, 3, 5].map((i) => parseInt(color.slice(i, i + 2), 16));
  const light = channels.map((v) => v / 255).map((v) => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
  if (light[0] * .2126 + light[1] * .7152 + light[2] * .0722 >= .26) return color;
  return `rgb(${channels.map((n) => Math.round(n + (255 - n) * .5)).join(",")})`;
}

/** Keep text input available for configured reactions even if Twitch's catalog cannot load. */
export function rewardNeedsInput(reward: import("../../api").Reward | undefined, channel?: import("../../api").ChannelReward) {
  if (channel?.requiresInput) return true;
  if (!reward) return false;
  const textInput = (text: string) => /\{(?:message|input)\}/.test(text);
  return (reward.response.chat.enabled && reward.response.chat.components.some((c) =>
    c.type === "variable" ? ["message", "input"].includes(c.name) : c.type === "static" ? textInput(c.value) : c.type === "phrase" && c.phrases.some(textInput)))
    || (reward.response.media.enabled && reward.response.media.text.enabled && textInput(reward.response.media.text.content));
}

export type ReaderRewardItem = { id: string; channel?: import("../../api").ChannelReward; reaction?: import("../../api").Reward };
/** The catalog, not the configured subset, determines which Twitch rewards are visible. */
export function readerRewardItems(channel: import("../../api").ChannelReward[], reactions: import("../../api").Reward[]): ReaderRewardItem[] {
  const byId = new Map<string, ReaderRewardItem>();
  for (const reward of channel) if (reward.isEnabled) byId.set(reward.id, { id: reward.id, channel: reward, reaction: reactions.find((r) => r.rewardId === reward.id) });
  for (const reaction of reactions) if (reaction.enabled && !byId.has(reaction.rewardId) && !channel.some((r) => r.id === reaction.rewardId)) byId.set(reaction.rewardId, { id: reaction.rewardId, reaction });
  return [...byId.values()].sort((a, b) => (a.channel?.cost ?? Infinity) - (b.channel?.cost ?? Infinity)
    || (a.channel?.title ?? a.reaction?.rewardTitle ?? "").localeCompare(b.channel?.title ?? b.reaction?.rewardTitle ?? "", "ru") || a.id.localeCompare(b.id));
}
