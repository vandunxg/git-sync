import { Fragment } from "react"

import { Icon } from "@/lib/icons"
import { cn } from "@/lib/utils"

export const STEPS = [
  { step: 1, label: "Welcome" },
  { step: 2, label: "Backup folder" },
  { step: 3, label: "GitHub" },
  { step: 4, label: "GitLab" },
  { step: 5, label: "Review & start" },
] as const

export type Step = (typeof STEPS)[number]["step"]

/** Real step chips: done = check icon (navigable back), current = filled number, upcoming = muted. */
export function Stepper({ current, onSelect }: { current: Step; onSelect: (step: Step) => void }) {
  return (
    <nav aria-label="Setup progress">
      <ol className="flex flex-wrap items-center justify-center gap-y-2">
        {STEPS.map(({ step, label }, index) => {
          const state = step < current ? "done" : step === current ? "current" : "upcoming"
          const content = (
            <>
              <span
                className={cn(
                  "inline-flex size-5 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold",
                  state === "done" && "bg-success text-white",
                  state === "current" && "bg-primary text-primary-foreground",
                  state === "upcoming" && "bg-muted text-muted-foreground ring-1 ring-border",
                )}
                aria-hidden="true"
              >
                {state === "done" ? <Icon name="status.ready" size={12} className="stroke-[2.5]" /> : step}
              </span>
              <span>{label}</span>
              <span className="sr-only">{state === "done" ? " (completed)" : state === "current" ? " (current step)" : ""}</span>
            </>
          )
          const chipClass = cn(
            "inline-flex h-8 items-center gap-2 rounded-full px-3 text-xs font-medium whitespace-nowrap",
            state === "done" && "bg-success-soft text-success",
            state === "current" && "bg-primary-soft text-primary ring-1 ring-primary/25",
            state === "upcoming" && "bg-card text-muted-foreground ring-1 ring-border",
          )
          return (
            <Fragment key={step}>
              {index > 0 ? (
                <li aria-hidden="true" className={cn("mx-1.5 h-px w-5", step <= current ? "bg-success/40" : "bg-border")} />
              ) : null}
              <li aria-current={state === "current" ? "step" : undefined}>
                {state === "done" ? (
                  <button
                    type="button"
                    className={cn(chipClass, "outline-none hover:bg-success/15 focus-visible:ring-3 focus-visible:ring-ring/50")}
                    onClick={() => onSelect(step)}
                  >
                    {content}
                  </button>
                ) : (
                  <span className={chipClass}>{content}</span>
                )}
              </li>
            </Fragment>
          )
        })}
      </ol>
    </nav>
  )
}
