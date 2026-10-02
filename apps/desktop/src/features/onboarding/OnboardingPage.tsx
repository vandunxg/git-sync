import { useEffect, useState } from "react"
import { Navigate, useNavigate, useSearchParams } from "react-router"

import { IconTile } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { accountFor } from "@/features/connections"
import { keys, useAccounts, useNativePort, useNativeQuery, useScanAction } from "@/lib/api"
import { Icon } from "@/lib/icons"

import { FolderStep, INITIAL_FOLDER, isFolderCommitted, type FolderState } from "./FolderStep.tsx"
import { ProviderStep, type DestinationState } from "./ProviderStep.tsx"
import { ReviewStep } from "./ReviewStep.tsx"
import type { Step } from "./Stepper.tsx"
import { WelcomeStep } from "./WelcomeStep.tsx"

function parseStep(raw: string | null): Step {
  const value = Number(raw)
  return value === 2 || value === 3 || value === 4 || value === 5 ? value : 1
}

function SetupComplete() {
  const navigate = useNavigate()
  const scan = useScanAction()
  return (
    <div className="mx-auto flex w-full max-w-[560px] flex-col px-6 py-20">
      <Card className="gap-5">
        <div className="flex flex-col items-center gap-3 px-6 text-center">
          <IconTile icon="status.synced" tone="success" />
          <h1 id="onboarding-heading" tabIndex={-1} className="text-[22px] font-semibold outline-none">
            Setup complete
          </h1>
          <p className="text-sm text-muted-foreground">
            Your backup configuration is saved. Nothing has been scanned or pushed yet. Run the first scan to discover your
            GitHub repositories; its progress appears in Sync Center.
          </p>
        </div>
        <div className="flex flex-wrap justify-center gap-2 px-6">
          <Button onClick={scan.run} disabled={!scan.available || scan.pending} aria-describedby={scan.unavailableReason ? "scan-reason" : undefined}>
            <Icon name="action.scan" />
            {scan.pending ? "Queuing scan…" : "Run first scan"}
          </Button>
          <Button variant="outline" onClick={() => void navigate("/", { replace: true })}>
            Open dashboard
            <Icon name="action.next" />
          </Button>
        </div>
        {scan.unavailableReason ? (
          <p id="scan-reason" className="px-6 text-center text-xs text-muted-foreground">
            {scan.unavailableReason}
          </p>
        ) : null}
      </Card>
    </div>
  )
}

/** UI-01/UI-02 onboarding. Step lives in the URL; non-secret inputs survive Back; tokens never leave the dialog. */
export function OnboardingPage() {
  const [params, setParams] = useSearchParams()
  const port = useNativePort()
  const bootstrap = useNativeQuery(keys.bootstrap, "gbm_app_bootstrap", {}, { staleTime: 60_000 })
  const accounts = useAccounts()
  const [folder, setFolder] = useState<FolderState>(INITIAL_FOLDER)
  const [destination, setDestination] = useState<DestinationState>({ namespace: null, visibility: "private" })
  const [finished, setFinished] = useState(false)

  const requested = parseStep(params.get("step"))
  const github = accountFor(accounts.data?.items, "github")
  const gitlab = accountFor(accounts.data?.items, "gitlab")
  const root = isFolderCommitted(folder) ? (folder.committed?.root ?? null) : null
  const githubReady = github?.authState === "connected"
  const gitlabReady = gitlab?.authState === "connected"

  let reachable: Step = 2
  if (root) reachable = 3
  if (root && githubReady) reachable = 4
  if (root && githubReady && gitlabReady && destination.namespace) reachable = 5
  const step: Step = requested > reachable && !accounts.isPending ? reachable : requested

  // Move focus to the new step's heading so keyboard and screen-reader users land in context.
  useEffect(() => {
    document.getElementById("onboarding-heading")?.focus({ preventScroll: true })
    document.querySelector("[data-onboarding-scroll]")?.scrollTo({ top: 0 })
  }, [step, finished])

  if (bootstrap.data?.onboardingComplete && !finished) return <Navigate to="/" replace />

  const goTo = (next: Step) => setParams(next === 1 ? {} : { step: String(next) })

  let content
  if (finished) {
    content = <SetupComplete />
  } else if (step === 1) {
    content = (
      <WelcomeStep
        bootstrap={bootstrap.data}
        error={bootstrap.error}
        checking={bootstrap.isFetching}
        onRecheck={() => void bootstrap.refetch()}
        onStart={() => goTo(2)}
      />
    )
  } else if (step === 2) {
    content = <FolderStep folder={folder} onChange={setFolder} onBack={() => goTo(1)} onNext={() => goTo(3)} onSelectStep={goTo} />
  } else if (step === 3 || step === 4) {
    const provider = step === 3 ? "github" : "gitlab"
    content = (
      <ProviderStep
        key={provider}
        provider={provider}
        account={provider === "github" ? github : gitlab}
        accountsLoading={accounts.isPending}
        accountsError={accounts.error}
        onRetryAccounts={() => void accounts.refetch()}
        destination={destination}
        onDestinationChange={setDestination}
        onBack={() => goTo(step === 3 ? 2 : 3)}
        onNext={() => goTo(step === 3 ? 4 : 5)}
        onSelectStep={goTo}
      />
    )
  } else if (root && github && gitlab && destination.namespace) {
    content = (
      <ReviewStep
        root={root}
        github={github}
        gitlab={gitlab}
        destination={{ ...destination, namespace: destination.namespace }}
        settings={bootstrap.data?.settings}
        onBack={() => goTo(4)}
        onFinished={() => setFinished(true)}
        onSelectStep={goTo}
      />
    )
  }

  return (
    <div className="h-full overflow-x-hidden overflow-y-auto bg-background" data-onboarding-scroll>
      <div className="flex items-center justify-between gap-3 px-6 pt-4">
        <span className="flex items-center gap-2 text-[13px] font-semibold text-muted-foreground">
          <Icon name="nav.repositories" className="text-primary" />
          Git Backup Manager
        </span>
        {port.kind === "fixture" ? (
          <span className="rounded-md bg-warning-soft px-2.5 py-1 text-xs font-medium text-warning">Demo data · fixture adapter</span>
        ) : null}
      </div>
      <main>{content}</main>
    </div>
  )
}
