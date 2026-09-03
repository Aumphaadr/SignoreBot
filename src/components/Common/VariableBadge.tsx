import { copyText } from "../../api/clipboard";
import Tooltip from "../Tooltip";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import "./VariableBadge.css";

export const VARIABLE_DESCRIPTIONS: Record<string, string> = {
  user: "Пользователь, который вызвал команду, событие или награду.",
  username: "Имя пользователя, связанного с действием.",
  target: "Первый аргумент после команды. Если аргумент не указан, бот выберет случайного зрителя.",
  message: "Текст сообщения (аргументы команды) или текст, введённый при активации награды.",
  tier: "Уровень платной подписки: Tier 1, Tier 2 или Tier 3.",
  isGift: "Была ли подписка подарочной: true или false.",
  months: "Общее количество месяцев подписки.",
  streakMonths: "Месяцев подряд в текущей серии.",
  total: "Количество подаренных подписок.",
  isAnonymous: "Было ли действие анонимным: true или false.",
  bits: "Количество Bits.",
  viewers: "Количество зрителей во входящем рейде.",
  fromUserId: "Twitch ID рейдера.",
  userId: "Twitch ID пользователя.",
  streakCount: "Стримов подряд в watch streak.",
  channelPointsAwarded: "Начислено баллов канала за watch streak.",
  systemMessage: "Системный текст уведомления Twitch.",
};

export function VariableBadge({ name, description, className = "" }: { name: string; description?: string; className?: string }) {
  const { showNotification } = useNotification();
  const n = name.replace(/[{}]/g, "");
  const copy = () => {
    void copyText(`{${n}}`);
    showNotification(`{${n}} скопировано — вставьте в текст`, NOTIFICATION_TYPES.INFO, 1500);
  };
  return (
    <Tooltip text={`${description ?? VARIABLE_DESCRIPTIONS[n] ?? `Переменная {${n}}.`} Клик — скопировать.`}>
      <code className={`variable-badge ${className}`.trim()} onClick={copy} role="button" style={{ cursor: "copy" }}>{`{${n}}`}</code>
    </Tooltip>
  );
}

export function VariableBadges({ variables, descriptions = {}, className = "" }: { variables: string[]; descriptions?: Record<string, string>; className?: string }) {
  return (
    <span className={`variable-badges ${className}`.trim()}>
      {variables.map((v) => <VariableBadge key={v} name={v} description={descriptions[v]} />)}
    </span>
  );
}
