// Подсказки для капсул на карточках команд, наград, событий и периодики.
// Текст собирается из данных (алиасы, оверлей, права…), а не пишется руками,
// чтобы подсказка не расходилась с состоянием. Имена выделяются <Em>.

import type { ReactNode } from "react";
import type { Overlay, Response } from "../../api";
import { formatInterval } from "../../api/defaults";
import Tooltip from "../Tooltip";

export type Subject = { kind: "command" | "reward" | "event" | "periodic"; name: string };

export const Em = ({ children }: { children: ReactNode }) => <span className="hint-em">{children}</span>;

/** Обёртка капсулы: показывает подсказку при наведении. */
export function Hint({ text, children }: { text: ReactNode; children: ReactNode }) {
  return <Tooltip text={text} inline>{children}</Tooltip>;
}

/** «команда !x» / «награда «X»» / «событие «X»» / «периодическое «X»»; `acc` — винительный падеж («за награду», «на событие»). */
export function subj(s: Subject, acc = false): ReactNode {
  switch (s.kind) {
    case "command": return <>{acc ? "команду" : "команда"} <Em>!{s.name}</Em></>;
    case "reward": return <>{acc ? "награду" : "награда"} <Em>«{s.name}»</Em></>;
    case "event": return <>событие <Em>«{s.name}»</Em></>;
    case "periodic": return <>периодическое <Em>«{s.name}»</Em></>;
  }
}

/** Что делает реакция: «команда !x отправляет текст в чат…». Для событий и наград — «на событие … бот отправляет». */
export function hintReaction(s: Subject, r: Response): ReactNode {
  const c = r.chat.enabled, m = r.media.enabled;
  const what = c && m ? "текст в чат и медиа на оверлей" : c ? "текст в чат" : m ? "медиа на оверлей" : null;
  const lead: ReactNode = s.kind === "event" ? <>на {subj(s, true)} бот</> : s.kind === "reward" ? <>за {subj(s, true)} бот</> : subj(s);
  if (!what) return <>{lead} пока ничего не делает — реакция не настроена</>;
  return <>{lead} отправляет {what}</>;
}

export function hintStatus(s: Subject, enabled: boolean): ReactNode {
  // род: команда/награда — ж., событие/периодическое — ср.
  const f = s.kind === "command" || s.kind === "reward";
  const on = f ? "включена" : "включено", off = f ? "выключена" : "выключено", it = f ? "её" : "его";
  return enabled
    ? <>{subj(s)} {on} — нажмите, чтобы выключить</>
    : <>{subj(s)} {off} — бот {it} не выполняет; нажмите, чтобы включить</>;
}

export function hintAliases(name: string, aliases: string[]): ReactNode {
  return <>команда <Em>!{name}</Em> имеет алиас{aliases.length > 1 ? "ы" : ""}: {aliases.map((a, i) => <span key={a}>{i > 0 && ", "}<Em>!{a}</Em></span>)}</>;
}

/** Целевой оверлей не выбран: ядро шлёт копию на каждый настроенный. */
export function hintOverlayAll(overlays: Overlay[]): ReactNode {
  return (
    <>
      целевой оверлей не выбран, поэтому копия медиа уходит на каждый из настроенных ({overlays.length}):{" "}
      {overlays.map((o, i) => <span key={o.id}>{i > 0 && ", "}<Em>{o.name}</Em></span>)}
      . Чтобы медиа показывалось один раз, выберите оверлей в редакторе.
    </>
  );
}

export function hintOverlay(ov: Overlay): ReactNode {
  return <>медиа отправляется на оверлей: <Em>{ov.name}</Em> (/overlay/{ov.path})</>;
}

const ROLE_LABEL: Record<string, string> = { broadcaster: "Стример", moderators: "Модераторы", vips: "VIP", subscribers: "Подписчики" };

export function hintPermissions(name: string, perms: string[]): ReactNode {
  const roles = perms.filter((p) => !p.startsWith("user:")).map((p) => ROLE_LABEL[p] ?? p);
  const users = perms.filter((p) => p.startsWith("user:")).map((p) => p.slice(5));
  return (
    <>
      команду <Em>!{name}</Em> могут вызывать{roles.length > 0 && <>: {roles.join(", ")}</>}
      {users.length > 0 && <>{roles.length > 0 ? "; а также выбранные" : " только выбранные"}: {users.map((u, i) => <span key={u}>{i > 0 && ", "}<Em>{u}</Em></span>)}</>}
    </>
  );
}

export function hintCooldown(name: string, sec: number, userSec = 0): ReactNode {
  const all = sec > 0 ? <>не чаще раза в {formatInterval(sec)} для всех</> : null;
  const per = userSec > 0 ? <>не чаще раза в {formatInterval(userSec)} для одного зрителя</> : null;
  return <>команда <Em>!{name}</Em> срабатывает {all}{all && per ? " и " : ""}{per} — лишние вызовы в это время игнорируются</>;
}

export function hintReply(name: string): ReactNode {
  return <>ответ на <Em>!{name}</Em> уходит реплаем на сообщение зрителя — в чате видно, кому отвечает бот</>;
}

export function hintInterval(name: string, sec: number): ReactNode {
  return <>периодическое <Em>«{name}»</Em> повторяется каждые {formatInterval(sec)} ({sec} с)</>;
}

export function hintOffset(name: string, sec: number): ReactNode {
  return <>первое срабатывание <Em>«{name}»</Em> — через {formatInterval(sec)} после запуска бота, дальше по интервалу; смещение разводит таймеры, чтобы они не срабатывали разом</>;
}

export function hintFireOnStart(name: string): ReactNode {
  return <>периодическое <Em>«{name}»</Em> срабатывает сразу при запуске бота, не дожидаясь первого интервала</>;
}

export function hintNext(name: string, sec: number): ReactNode {
  return <>следующее срабатывание <Em>«{name}»</Em> через {formatInterval(sec)}</>;
}

export function hintSkipGifted(): ReactNode {
  return <>подарочные подписки не вызывают эту реакцию — на них приходит отдельное событие «Подарочная подписка»</>;
}

export function hintRewardMissing(title: string): ReactNode {
  return <>награды <Em>«{title}»</Em> больше нет на канале — реакция не сработает; выберите другую награду в редакторе</>;
}

/** Кнопка «Если недоступен» на карточке оверлея: есть ли резерв, включён ли, из чего состоит. */
export function hintFallback(ov: Overlay, all: Overlay[]): ReactNode {
  const fb = ov.fallback;
  const c = !!fb?.chat.enabled, m = !!fb?.media.enabled;
  if (!fb || (!c && !m)) return <>резервная реакция для <Em>«{ov.name}»</Em> не настроена: если оверлей выключен, медиа ждёт его в очереди 30 секунд</>;
  const target = m && fb.media.overlay ? all.find((x) => x.id === fb.media.overlay)?.name : null;
  const media: ReactNode = <>медиа на {target ? <Em>«{target}»</Em> : "все оверлеи"}</>;
  const what: ReactNode = c && m ? <>текст в чат и {media}</> : c ? "текст в чат" : media;
  return ov.fallbackEnabled
    ? <>если <Em>«{ov.name}»</Em> выключен, бот отправляет {what}; медиа для него в очередь не ставится</>
    : <>резервная реакция настроена ({what}), но выключена — медиа ждёт оверлей в очереди 30 секунд</>;
}
