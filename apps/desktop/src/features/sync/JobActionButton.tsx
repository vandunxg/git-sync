import { useState } from "react"
import { toast } from "sonner"

import { ConfirmDialog } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { PlanReviewDialog } from "@/features/repositories"
import { ERROR_COPY, keys, repositoryStateKeys, useNativeMutation, type NativeCommandError } from "@/lib/api"
import { Icon } from "@/lib/icons"
import type { Job } from "@/lib/native"

import { jobAction } from "./jobs.ts"

function failure(title: string, error: NativeCommandError) {
  toast.error(title, { description: `${ERROR_COPY[error.appError.code]} (${error.appError.code})` })
}

/**
 * The one contextual action for a job: Cancel, Retry or Review changes. Results are
 * reported exactly as the native side confirmed them (cancel is a request, retry queues).
 */
export function JobActionButton({
  job,
  name,
  onRetried,
  size = "sm",
}: {
  job: Job
  name: string
  onRetried?: (jobId: string) => void
  size?: "sm" | "default"
}) {
  const action = jobAction(job)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [reviewOpen, setReviewOpen] = useState(false)
  const cancel = useNativeMutation("gbm_job_cancel", [keys.jobs, ...repositoryStateKeys])
  const retry = useNativeMutation("gbm_job_retry", [keys.jobs, ...repositoryStateKeys])

  function runCancel() {
    cancel.mutate(
      { jobId: job.id },
      {
        onSuccess: (updated) => {
          setConfirmOpen(false)
          if (updated.state === "canceled") toast.success("Job canceled", { description: name })
          else toast.info("Cancellation requested", { description: "The job shows Canceling until the native side confirms it stopped." })
        },
        onError: (error) => failure("Could not cancel the job", error),
      },
    )
  }

  function runRetry() {
    retry.mutate(
      { jobId: job.id },
      {
        onSuccess: (result) => {
          toast.success("Retry queued", { description: `${name} · attempt ${job.attempt + 1}` })
          onRetried?.(result.jobId)
        },
        onError: (error) => failure("Retry was not queued", error),
      },
    )
  }

  switch (action.kind) {
    case "none":
      return null
    case "canceling":
      return (
        <Button size={size} variant="outline" disabled>
          Canceling…
        </Button>
      )
    case "cancel":
      return (
        <>
          <Button
            size={size}
            variant="outline"
            aria-label={`Cancel job for ${name}`}
            disabled={cancel.isPending}
            onClick={() => (action.confirm ? setConfirmOpen(true) : runCancel())}
          >
            <Icon name="action.cancel" />
            {cancel.isPending ? "Canceling…" : "Cancel"}
          </Button>
          {action.confirm ? (
            <ConfirmDialog
              open={confirmOpen}
              onOpenChange={setConfirmOpen}
              destructive
              icon="state.warning"
              title="Cancel while mirroring to GitLab?"
              description={
                <>
                  <p>
                    {name} is pushing refs to GitLab. Some refs may already be updated, so the result can be uncertain.
                  </p>
                  <p className="mt-2">After cancellation the app checks the destination instead of assuming a rollback.</p>
                </>
              }
              confirmLabel="Cancel job"
              pendingLabel="Requesting cancel…"
              pending={cancel.isPending}
              onConfirm={runCancel}
            />
          ) : null}
        </>
      )
    case "retry":
      return (
        <Button size={size} variant="outline" aria-label={`Retry job for ${name}`} disabled={retry.isPending} onClick={runRetry}>
          <Icon name="action.retry" />
          {retry.isPending ? "Queuing…" : "Retry"}
        </Button>
      )
    case "review":
      return (
        <>
          <Button size={size} variant="outline" aria-label={`Review changes for ${name}`} onClick={() => setReviewOpen(true)}>
            <Icon name="status.blocked" />
            Review changes
          </Button>
          <PlanReviewDialog planId={action.planId} open={reviewOpen} onOpenChange={setReviewOpen} />
        </>
      )
  }
}
