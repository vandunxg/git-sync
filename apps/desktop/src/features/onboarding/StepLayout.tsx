import type { ReactNode } from "react"

import { Button } from "@/components/ui/button"
import { Icon } from "@/lib/icons"

import { Stepper, type Step } from "./Stepper.tsx"

/** Shared frame for steps 2–5: title, real stepper, content, Back/primary footer. */
export function StepLayout({
  step,
  title,
  description,
  onSelectStep,
  children,
  onBack,
  primary,
  blockedReason,
}: {
  step: Step
  title: string
  description: string
  onSelectStep: (step: Step) => void
  children: ReactNode
  onBack?: () => void
  primary: ReactNode
  /** Accessible text explaining why the primary action is disabled. */
  blockedReason?: string | null
}) {
  return (
    <div className="mx-auto flex w-full max-w-[880px] flex-col gap-6 px-6 py-10">
      <header className="flex flex-col items-center gap-1 text-center">
        <h1 id="onboarding-heading" tabIndex={-1} className="text-[26px] leading-tight font-semibold tracking-tight outline-none">
          {title}
        </h1>
        <p className="text-sm text-muted-foreground">
          Step {step} of 5 · {description}
        </p>
      </header>
      <Stepper current={step} onSelect={onSelectStep} />
      <div className="flex min-w-0 flex-col gap-5">{children}</div>
      <footer className="flex flex-col items-center gap-2 pt-1">
        <div className="flex flex-wrap items-center justify-center gap-2">
          {onBack ? (
            <Button variant="outline" onClick={onBack}>
              <Icon name="action.previous" />
              Back
            </Button>
          ) : null}
          {primary}
        </div>
        {blockedReason ? (
          <p id="onboarding-blocked-reason" className="text-xs text-muted-foreground">
            {blockedReason}
          </p>
        ) : null}
      </footer>
    </div>
  )
}
