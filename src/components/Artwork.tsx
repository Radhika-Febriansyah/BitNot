import { Icon, type IconName } from "./Icon";

interface ArtworkProps {
  src?: string;
  /** Edge length in rem. Omit to let CSS size the tile (it stays square). */
  size?: number;
  /** Icon shown when there is no artwork. */
  fallback?: IconName;
  /** Tinted tile for built-in collections such as Favorites. */
  accent?: boolean;
  className?: string;
}

/** Square artwork. Shows a neutral tile with an icon when there is no artwork. */
export function Artwork({ src, size, fallback = "music", accent, className }: ArtworkProps) {
  const classes = ["art", accent ? "accent" : "", className ?? ""].filter(Boolean).join(" ");
  const style = size ? { width: `${size}rem`, height: `${size}rem` } : undefined;
  return (
    <span className={classes} style={style}>
      {src ? (
        <img src={src} alt="" draggable={false} />
      ) : (
        <Icon name={fallback} size={size ? Math.max(1, size * 0.42) : 2} />
      )}
    </span>
  );
}
