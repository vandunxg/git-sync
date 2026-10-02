import type { ReactNode } from "react"

import { Card, CardAction, CardContent, CardDescription, CardHeader } from "@/components/ui/card"
import type { IconKey } from "@/lib/icons"
import { cn } from "@/lib/utils"

import { IconTile, type Tone } from "./StatusBadge.tsx"

/** A meaningful section: icon + title + description, optional header action. */
export function SectionCard({
  icon,
  tone,
  title,
  description,
  action,
  children,
  className,
  contentClassName,
  headingLevel = 2,
}: {
  icon?: IconKey
  tone?: Tone
  title: ReactNode
  description?: ReactNode
  action?: ReactNode
  children?: ReactNode
  className?: string
  contentClassName?: string
  headingLevel?: 2 | 3
}) {
  const Heading = headingLevel === 2 ? "h2" : "h3"
  return (
    <Card className={className}>
      <CardHeader>
        <div className="flex min-w-0 items-center gap-3">
          {icon ? <IconTile icon={icon} tone={tone} /> : null}
          <div className="min-w-0">
            <Heading data-slot="card-title" className="text-[15px] leading-snug font-semibold">
              {title}
            </Heading>
            {description ? <CardDescription>{description}</CardDescription> : null}
          </div>
        </div>
        {action ? <CardAction>{action}</CardAction> : null}
      </CardHeader>
      {children ? <CardContent className={cn("min-w-0", contentClassName)}>{children}</CardContent> : null}
    </Card>
  )
}
