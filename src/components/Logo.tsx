import logoUrl from "../assets/logo.png";

/** The BitNot app mark (source: docs/design/brand/). Decorative by default. */
export function Mark({ size = 1.5 }: { size?: number }) {
  return (
    <img
      className="mark"
      src={logoUrl}
      alt=""
      aria-hidden="true"
      draggable={false}
      style={{ width: `${size}rem`, height: `${size}rem` }}
    />
  );
}

/** Mark plus the BitNot wordmark. */
export function Wordmark({ size = 1.5 }: { size?: number }) {
  return (
    <span className="wordmark">
      <Mark size={size} />
      <span className="wordmark-text" style={{ fontSize: `${size * 0.6}rem` }}>
        BitNot
      </span>
    </span>
  );
}
