const icon = new URL("../../src-tauri/icons/32x32.png", import.meta.url).href;

export function AppIcon({ size, animated = false }: { size: number; animated?: boolean }) {
  return (
    <img
      alt=""
      aria-hidden="true"
      src={icon}
      width={size}
      height={size}
      className={animated ? "app-icon-animated" : undefined}
    />
  );
}
