// Official GitHub/GitLab marks from the simple-icons package (path data published from
// the brands' own assets; see node_modules/simple-icons/DISCLAIMER.md). Not hand-drawn.

import { siGithub, siGitlab } from "simple-icons"

import type { Provider } from "@/lib/native"
import { cn } from "@/lib/utils"

const MARKS = { github: siGithub, gitlab: siGitlab } as const

export const PROVIDER_LABEL: Record<Provider, string> = { github: "GitHub", gitlab: "GitLab" }

export function BrandMark({ provider, size = 18, className }: { provider: Provider; size?: number; className?: string }) {
  const mark = MARKS[provider]
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      className={cn("shrink-0", className)}
      aria-hidden="true"
      focusable="false"
    >
      <path d={mark.path} fill={`#${mark.hex}`} />
    </svg>
  )
}
