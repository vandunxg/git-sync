import { cn } from "@/lib/utils"

import { ICON_STROKE, ICONS, iconSize, type IconKey } from "./registry.ts"

type IconProps = {
  name: IconKey
  className?: string
  size?: number
  /** Spins only for a real running job and only when motion is allowed. */
  spinning?: boolean
  /** Icon-only usage must pass a label; decorative icons stay hidden from AT. */
  label?: string
}

export function Icon({ name, className, size, spinning = false, label }: IconProps) {
  const Component = ICONS[name]
  return (
    <Component
      size={size ?? iconSize(name)}
      strokeWidth={ICON_STROKE}
      className={cn("shrink-0", spinning && "motion-safe:animate-spin", className)}
      aria-hidden={label === undefined ? true : undefined}
      aria-label={label}
      role={label === undefined ? undefined : "img"}
      focusable="false"
    />
  )
}
