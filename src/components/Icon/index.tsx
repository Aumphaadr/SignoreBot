// Единая иконка интерфейса. Рисуется цветом текста (currentColor), размер —
// от кегля (1em), поэтому подходит вместо прежних react-icons без правок вёрстки.

import { ICONS, type IconName } from "./icons";
import "./Icon.css";

export type { IconName };

export default function Icon({
  name,
  size,
  className = "",
  title,
}: {
  name: IconName;
  /** Размер стороны; по умолчанию 1em — тянется за кеглем текста. */
  size?: number | string;
  className?: string;
  /** Если задан, иконка становится доступной для чтения с экрана. */
  title?: string;
}) {
  const icon = ICONS[name];
  if (!icon) return null;
  const [viewBox, body, attrs] = icon;
  const root: Record<string, string> = {};
  for (const m of attrs.matchAll(/([a-zA-Z:-]+)="([^"]*)"/g)) root[m[1]] = m[2];
  return (
    <svg
      className={`icon ${className}`.trim()}
      viewBox={viewBox}
      width={size ?? "1em"}
      height={size ?? "1em"}
      fill={root.fill ?? "currentColor"}
      stroke={root.stroke}
      strokeWidth={root["stroke-width"]}
      strokeLinecap={root["stroke-linecap"] as "round" | undefined}
      strokeLinejoin={root["stroke-linejoin"] as "round" | undefined}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      focusable="false"
      dangerouslySetInnerHTML={{ __html: title ? `<title>${title}</title>${body}` : body }}
    />
  );
}
