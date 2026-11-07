import { useEffect, useState } from "react";
import Icon from "../Icon";
export default function RewardPicture({ url, size = 48 }: { url?: string | null; size?: number }) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [url]);
  return url && !failed ? <img src={url} alt="" referrerPolicy="no-referrer" onError={() => setFailed(true)} /> : <Icon name="channel-points" size={size} />;
}
