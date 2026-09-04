// Значения по умолчанию, совпадающие с Rust `Default` (см. schema.rs).
import type { IconName } from "../components/Icon";
import type { Command, MediaResponse, PeriodicEvent, Response, Reward } from "./generated/config";

export function newId(prefix: string): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`;
}

export const defaultMedia = (): MediaResponse => ({
  enabled: false,
  file: "",
  secondaryFile: "",
  volume: 100,
  overlay: null,
  queueMode: "queue",
  chromakey: "none",
  imageDurationSec: null,
  animation: { enter: "none", exit: "none", enterDuration: 0.5, exitDuration: 0.5 },
  text: { enabled: false, content: "", position: "overlay", animation: "none", animationAmplitude: 1, font: {} },
});

export const defaultResponse = (): Response => ({ chat: { enabled: false, components: [] }, media: defaultMedia() });

export const defaultCommand = (): Command => ({
  id: newId("cmd"),
  enabled: true,
  name: "",
  aliases: [],
  permissions: [],
  cooldownSec: 0,
  response: defaultResponse(),
});

export const defaultReward = (rewardId: string, rewardTitle: string): Reward => ({
  id: newId("reward"),
  enabled: true,
  managed: false,
  refundIfUnavailable: false,
  originalRewardId: null,
  rewardId,
  rewardTitle,
  response: defaultResponse(),
});

export const defaultPeriodic = (): PeriodicEvent => ({
  id: newId("periodic"),
  name: "",
  enabled: true,
  intervalSec: 300,
  offsetSec: 0,
  color: "",
  fireOnStart: false,
  response: defaultResponse(),
});

export const EVENT_TYPES: Record<string, { label: string; icon: IconName; description: string; vars: string[] }> = {
  follow: { icon: "follower-heart", label: "Новый фолловер", description: "Кто-то подписался на канал (follow)", vars: ["user"] },
  subscribe: { icon: "vip-star", label: "Новая подписка", description: "Новая платная подписка на канал", vars: ["user", "tier", "isGift"] },
  resubscribe: { icon: "vip-star", label: "Переподписка", description: "Продление подписки с сообщением", vars: ["user", "tier", "months", "streakMonths", "message"] },
  giftSub: { icon: "gift", label: "Подарочная подписка", description: "Кто-то дарит подписки", vars: ["user", "tier", "total", "isAnonymous"] },
  bits: { icon: "bits", label: "Bits / Cheer", description: "Кто-то отправляет Bits в чат", vars: ["user", "bits", "message", "isAnonymous"] },
  raid: { icon: "users", label: "Рейд", description: "Входящий рейд на канал", vars: ["user", "viewers"] },
  watchStreak: { icon: "lightning", label: "Watch Streak", description: "Зритель делится серией просмотренных стримов", vars: ["user", "streakCount", "channelPointsAwarded", "systemMessage", "message"] },
};

export { FONT_FAMILIES } from "./fonts.generated";

export const TEXT_ANIMATIONS = [
  { value: "none", label: "— Без анимации —" },
  { value: "bounce", label: "Bounce" },
  { value: "pulse", label: "Pulse" },
  { value: "rubberBand", label: "Rubber Band" },
  { value: "tada", label: "Tada" },
  { value: "wave", label: "Wave" },
  { value: "wiggle", label: "Wiggle" },
  { value: "wobble", label: "Wobble" },
];

export const MEDIA_ENTER_ANIMATIONS = [
  { value: "none", label: "— Без анимации —" },
  { value: "fadeIn", label: "Fade In (плавное появление)" },
  { value: "fadeInLeft", label: "Сдвиг слева" },
  { value: "fadeInRight", label: "Сдвиг справа" },
  { value: "fadeInTop", label: "Сдвиг сверху" },
  { value: "fadeInBottom", label: "Сдвиг снизу" },
  { value: "scaleIn", label: "Scale In (увеличение из центра)" },
];

export const MEDIA_EXIT_ANIMATIONS = [
  { value: "none", label: "— Без анимации —" },
  { value: "fadeOut", label: "Fade Out (плавное исчезновение)" },
  { value: "fadeOutLeft", label: "Уход налево" },
  { value: "fadeOutRight", label: "Уход направо" },
  { value: "fadeOutTop", label: "Уход наверх" },
  { value: "fadeOutBottom", label: "Уход вниз" },
  { value: "scaleOut", label: "Scale Out (уменьшение в центр)" },
];

/** Тестовые значения переменных для предпросмотров. */
export const SAMPLE_VARS: Record<string, string> = {
  user: "TestUser", username: "TestUser", target: "TestTarget", message: "текст сообщения",
  tier: "Tier 1", tierRaw: "1000", isGift: "false", months: "6", streakMonths: "3", total: "5",
  isAnonymous: "false", bits: "250", viewers: "42", fromUserId: "12345", userId: "12345",
  streakCount: "120", channelPointsAwarded: "450", systemMessage: "TestStreaker sparked a watch streak!",
};

/** Подстановка `{var}` образцами (как в ядре — одним проходом). */
export function substituteSample(text: string): string {
  return text.replace(/\{(\w+)\}/g, (m, v: string) => SAMPLE_VARS[v] ?? m);
}

export function fileKind(name: string): "video" | "audio" | "image" | "unknown" {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["mp4", "webm", "mov", "m4v", "mkv", "avi", "flv", "ogv"].includes(ext)) return "video";
  if (["mp3", "wav", "ogg", "oga", "m4a", "flac", "aac", "opus"].includes(ext)) return "audio";
  if (["jpg", "jpeg", "png", "gif", "webp", "bmp", "apng"].includes(ext)) return "image";
  return "unknown";
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} Б`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} КБ`;
  return `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
}

export function formatInterval(seconds: number): string {
  if (seconds < 60) return `${seconds} сек`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const parts: string[] = [];
  if (h > 0) parts.push(`${h} ч`);
  if (m > 0) parts.push(`${m} мин`);
  if (s > 0 && h === 0) parts.push(`${s} сек`);
  return parts.join(" ");
}
