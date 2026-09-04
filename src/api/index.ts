// Типизированный слой над Tauri IPC. Всё общение панели с ядром — здесь.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Config, MigrationReport, Response } from "./generated/config";
import type {
  ChannelReward,
  CoreStatus,
  DataDirInfo,
  DeviceCode,
  ImportResult,
  LogEntry,
  MediaFile,
  MediaImportResult,
  ObsSource,
  ProbeResult,
  ShoutoutStatus,
  TimerStatus,
  UpdateInfo,
  ViewersInfo,
} from "./generated/api";

export type * from "./generated/config";
export type * from "./generated/api";

export type AccountKind = "broadcaster" | "bot";
export type ConfigSection = keyof Config;

/** События ядра → окно. */
export type ChangedWhat =
  | "auth"
  | "shoutout"
  | "eventsub"
  | "viewers"
  | "media"
  | "overlays"
  | "config"
  | "server"
  | "runtime";

export const api = {
  // статус
  status: () => invoke<CoreStatus>("status_get"),
  migrationDismiss: () => invoke<void>("migration_dismiss"),
  logHistory: () => invoke<LogEntry[]>("log_history"),
  logExport: (path: string) => invoke<number>("log_export", { path }),

  // конфиг
  configGet: () => invoke<Config>("config_get"),
  configSetSection: (section: ConfigSection, value: unknown) =>
    invoke<Config>("config_set_section", { section, value }),
  configExport: () => invoke<unknown>("config_export"),
  configExportWrite: (path: string, content: string) => invoke<void>("config_export_write", { path, content }),
  configImportFile: (path: string) => invoke<ImportResult>("config_import_file", { path }),

  // авторизация
  authStart: (kind: AccountKind) => invoke<DeviceCode>("auth_start", { kind }),
  authCancel: (kind: AccountKind) => invoke<void>("auth_cancel", { kind }),
  authLogout: (kind: AccountKind) => invoke<void>("auth_logout", { kind }),
  authRefresh: (kind: AccountKind) => invoke<void>("auth_refresh", { kind }),
  /** Бот — тот же аккаунт, что и стример (один токен, объединённые права). */
  authSetSameAccount: (on: boolean) => invoke<void>("auth_set_same_account", { on }),

  // медиа
  mediaList: () => invoke<MediaFile[]>("media_list"),
  mediaImport: (paths: string[]) => invoke<MediaImportResult>("media_import", { paths }),
  mediaDelete: (name: string) => invoke<void>("media_delete", { name }),
  mediaDeleteUnused: () => invoke<number>("media_delete_unused"),
  mediaProbe: (name: string) => invoke<ProbeResult>("media_probe", { name }),
  mediaUrl: (name: string) => invoke<string>("media_url", { name }),
  mediaTest: (response: Response) => invoke<boolean>("media_test", { response }),

  // действия
  eventTest: (eventType: string, extra?: Record<string, string>) =>
    invoke<void>("event_test", { eventType, extra: extra ?? null }),
  periodicTrigger: (id: string) => invoke<void>("periodic_trigger", { id }),
  periodicStatus: () => invoke<TimerStatus[]>("periodic_status"),
  shoutoutStatus: () => invoke<ShoutoutStatus>("shoutout_status"),
  shoutoutTrigger: (username: string) => invoke<void>("shoutout_trigger", { username }),
  shoutoutRemove: (id: number) => invoke<void>("shoutout_remove", { id }),
  shoutoutReset: () => invoke<void>("shoutout_reset"),
  rewardsChannel: () => invoke<ChannelReward[]>("rewards_channel"),
  chatSend: (text: string) => invoke<void>("chat_send", { text }),
  viewers: () => invoke<ViewersInfo>("viewers_get"),

  // оверлеи / OBS
  overlayClear: (path: string | null, all: boolean) => invoke<void>("overlay_clear", { path, all }),
  obsTest: () => invoke<ObsSource[]>("obs_test"),
  obsRefresh: () => invoke<string[]>("obs_refresh"),
  obsSetUrl: (inputName: string, overlayPath: string) =>
    invoke<string>("obs_set_url", { inputName, overlayPath }),
  overlayKeyRegenerate: () => invoke<string>("overlay_key_regenerate"),
  openDataDir: () => invoke<void>("app_open_data_dir"),
  updatesCheck: () => invoke<UpdateInfo>("updates_check"),
  dataDirInfo: () => invoke<DataDirInfo>("data_dir_info"),
  /** `path = null` — вернуться к стандартному каталогу. Применяется после перезапуска. */
  dataDirSet: (path: string | null, copy: boolean) => invoke<number>("data_dir_set", { path, copy }),
  appRestart: () => invoke<void>("app_restart"),

  // события
  onChanged: (cb: (what: ChangedWhat) => void): Promise<UnlistenFn> =>
    listen<ChangedWhat>("changed", (e) => cb(e.payload)),
  onLog: (cb: (entry: LogEntry) => void): Promise<UnlistenFn> =>
    listen<LogEntry>("log", (e) => cb(e.payload)),
};

export type { MigrationReport };

/** Человекочитаемое сообщение из ошибки invoke. */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
