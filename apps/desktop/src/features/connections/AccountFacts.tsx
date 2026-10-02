import { useId, useState, type ReactNode } from "react"

import { STORAGE_MODE } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { dateTime, relativeTime } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { AccountSummary } from "@/lib/native"
import { cn } from "@/lib/utils"

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-[13px] text-foreground">{children}</dd>
    </div>
  )
}

function expiryText(expiresAt: string | null): { text: string; past: boolean } {
  const formatted = dateTime(expiresAt)
  if (expiresAt === null || formatted === null) return { text: "Expiry not provided by the provider", past: false }
  const past = new Date(expiresAt).getTime() < Date.now()
  return { text: past ? `Expired ${formatted}` : `Expires ${formatted}`, past }
}

/** Verified account facts: storage, last verification, expiry only when known, capabilities on demand. */
export function AccountFacts({ account, className }: { account: AccountSummary; className?: string }) {
  const [showCapabilities, setShowCapabilities] = useState(false)
  const listId = useId()
  const verified = relativeTime(account.verifiedAt)
  const expiry = expiryText(account.expiresAt)
  const capabilities = account.capabilitySummary
  return (
    <div className={cn("flex flex-col gap-3", className)}>
      <dl className="grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-3">
        <Fact label="Credential storage">{STORAGE_MODE[account.storageMode]}</Fact>
        <Fact label="Last verified">
          {verified ? <span title={dateTime(account.verifiedAt) ?? undefined}>{verified}</span> : "Not verified yet"}
        </Fact>
        <Fact label="Token expiry">
          <span className={cn(expiry.past && "font-medium text-danger", account.expiresAt === null && "text-muted-foreground")}>
            {expiry.text}
          </span>
        </Fact>
      </dl>
      {capabilities.length > 0 ? (
        <div className="flex flex-col gap-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="-ml-2 w-fit text-[13px] text-muted-foreground"
            aria-expanded={showCapabilities}
            aria-controls={listId}
            onClick={() => setShowCapabilities((value) => !value)}
          >
            <Icon name="action.next" size={14} className={cn("transition-transform", showCapabilities && "rotate-90")} />
            {showCapabilities ? "Hide verified capabilities" : `Show verified capabilities (${capabilities.length})`}
          </Button>
          {showCapabilities ? (
            <ul id={listId} className="flex flex-col gap-1.5 rounded-lg bg-muted/60 px-3 py-2.5">
              {capabilities.map((capability) => (
                <li key={capability} className="flex items-start gap-2 text-[13px]">
                  <Icon name="status.ready" size={14} className="mt-0.5 text-success" />
                  <span className="min-w-0 break-words ui-selectable">{capability}</span>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">No capabilities were reported for this credential.</p>
      )}
    </div>
  )
}
