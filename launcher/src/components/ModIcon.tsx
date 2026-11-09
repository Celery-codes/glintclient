import { Puzzle } from "lucide-react";
import { useState } from "react";

/** A mod's icon, with a neutral placeholder when it has none or it fails to load. */
export function ModIcon({ url, size = 44 }: { url: string | null; size?: number }) {
  const [failed, setFailed] = useState(false);
  return (
    <div
      className="flex shrink-0 items-center justify-center overflow-hidden rounded-md bg-raised text-faint"
      style={{ width: size, height: size }}
    >
      {url && !failed ? (
        <img
          src={url}
          alt=""
          loading="lazy"
          draggable={false}
          onError={() => setFailed(true)}
          className="h-full w-full object-cover"
        />
      ) : (
        <Puzzle size={size * 0.45} strokeWidth={1.6} />
      )}
    </div>
  );
}
