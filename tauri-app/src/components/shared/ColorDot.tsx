interface ColorDotProps {
  color: string;
  size?: number;
}

export function ColorDot({ color, size = 10 }: ColorDotProps) {
  return (
    <span
      className="inline-block rounded-full shrink-0"
      style={{ width: size, height: size, backgroundColor: color }}
    />
  );
}
