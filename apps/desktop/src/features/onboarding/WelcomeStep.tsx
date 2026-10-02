import { ErrorState, IconTile } from "@/components/shared"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import type { NativeCommandError } from "@/lib/api"
import { Icon, type IconKey } from "@/lib/icons"
import type { Bootstrap, Diagnostic } from "@/lib/native"

const BENEFITS: { icon: IconKey; title: string; description: string }[] = [
  {
    icon: "event.scan",
    title: "Detect new repositories automatically",
    description: "Scheduled scans find repositories your GitHub token can read.",
  },
  {
    icon: "action.sync",
    title: "Keep local and GitLab copies in sync",
    description: "Changed refs are mirrored locally, pushed to GitLab and verified.",
  },
  {
    icon: "setting.notifications",
    title: "Surface only items that need attention",
    description: "History rewrites, failures and expired tokens wait for your review.",
  },
]

const CONFIGURED: { title: string; description: string }[] = [
  { title: "Backup folder", description: "Where verified local mirrors are kept" },
  { title: "GitHub connection", description: "Read-only access to your source repositories" },
  { title: "GitLab connection", description: "Destination account and namespace" },
  { title: "Review & start", description: "Planned project mapping before anything is created" },
  { title: "Initial scan", description: "Started by you once setup is finished" },
]

const BLOCKER_KEYS = new Set(["git", "keyring", "portal", "storage-root"])
const BLOCKER_COPY: Record<string, string> = {
  git: "Git is required to mirror repositories. Install Git, then check again.",
  keyring: "The system keyring is unavailable. You can still connect with a session-only token, which is cleared when the app quits.",
  portal: "The system folder picker is unavailable. Folder selection may fail until the desktop portal is running.",
  "storage-root": "The backup folder could not be verified. You will choose a folder in the next step.",
}

function setupBlockers(bootstrap: Bootstrap | undefined): Diagnostic[] {
  return (bootstrap?.diagnostics.items ?? []).filter(
    (item) => BLOCKER_KEYS.has(item.key) && (item.status === "missing" || item.status === "locked" || item.status === "unsupported"),
  )
}

export function WelcomeStep({
  bootstrap,
  error,
  checking,
  onRecheck,
  onStart,
}: {
  bootstrap: Bootstrap | undefined
  error: NativeCommandError | null
  checking: boolean
  onRecheck: () => void
  onStart: () => void
}) {
  const blockers = setupBlockers(bootstrap)
  return (
    <div className="mx-auto grid w-full max-w-[1120px] grid-cols-1 items-center gap-10 px-8 py-12 lg:grid-cols-[minmax(0,1fr)_400px] lg:gap-14 lg:py-20">
      <div className="flex min-w-0 flex-col gap-7">
        <span className="inline-flex size-12 items-center justify-center rounded-xl bg-primary-soft text-primary">
          <Icon name="nav.repositories" size={24} />
        </span>
        <div className="flex flex-col gap-3">
          <h1 id="onboarding-heading" tabIndex={-1} className="text-[34px] leading-tight font-semibold tracking-tight outline-none">
            Set up automatic repository backup
          </h1>
          <p className="max-w-[560px] text-[15px] leading-relaxed text-muted-foreground">
            GitHub stays your source of truth. Git Backup Manager keeps a verified local mirror and a GitLab backup copy of
            each repository you choose.
          </p>
        </div>
        <ul className="flex flex-col gap-4">
          {BENEFITS.map((benefit) => (
            <li key={benefit.title} className="flex items-start gap-3">
              <IconTile icon={benefit.icon} tone="success" />
              <div className="min-w-0">
                <p className="text-sm font-semibold">{benefit.title}</p>
                <p className="text-[13px] text-muted-foreground">{benefit.description}</p>
              </div>
            </li>
          ))}
        </ul>
        {error ? (
          <ErrorState error={error.appError} title="Setup checks could not run" onRetry={onRecheck} retrying={checking} />
        ) : blockers.length > 0 ? (
          <Alert className="border-warning/30 bg-warning-soft px-4 py-3 text-warning">
            <Icon name="state.warning" />
            <AlertTitle className="text-sm font-semibold">Check these before you continue</AlertTitle>
            <AlertDescription className="flex flex-col gap-2 text-[13px] text-warning/90">
              <ul className="flex flex-col gap-1.5">
                {blockers.map((item) => (
                  <li key={item.key}>
                    {BLOCKER_COPY[item.key] ?? item.detail} <span className="text-xs opacity-80 ui-selectable">({item.detail})</span>
                  </li>
                ))}
              </ul>
              <div>
                <Button variant="outline" onClick={onRecheck} disabled={checking}>
                  <Icon name="action.retry" />
                  {checking ? "Checking…" : "Check again"}
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        ) : null}
        <div>
          <Button className="h-10 px-4 text-sm" onClick={onStart}>
            Get started
            <Icon name="action.next" />
          </Button>
        </div>
      </div>

      <Card role="region" aria-labelledby="configured-heading" className="gap-4">
        <div className="flex items-center gap-3 px-5">
          <IconTile icon="setting.general" tone="info" />
          <div>
            <h2 id="configured-heading" className="text-[15px] font-semibold">
              What gets configured
            </h2>
            <p className="text-xs text-muted-foreground">5 short steps · nothing is pushed until you confirm</p>
          </div>
        </div>
        <ol className="flex flex-col px-5">
          {CONFIGURED.map((item, index) => (
            <li key={item.title} className="flex items-start gap-3 border-t py-3 first:border-t-0">
              <span className="mt-0.5 inline-flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold text-muted-foreground">
                {index + 1}
              </span>
              <div className="min-w-0">
                <p className="text-sm font-medium">{item.title}</p>
                <p className="text-xs text-muted-foreground">{item.description}</p>
              </div>
            </li>
          ))}
        </ol>
      </Card>
    </div>
  )
}
