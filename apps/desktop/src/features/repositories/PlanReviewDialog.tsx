import { useState, type ReactNode } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "react-router"
import { toast } from "sonner"

import { ErrorState, LoadingLabel, StatusBadge } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { ERROR_COPY, keys, NativeCommandError, repositoryStateKeys, runNative, useNativePort, useNativeQuery } from "@/lib/api"
import { dateTime, plural, relativeTime, shortOid } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { RefOperation, SyncPlan } from "@/lib/native"

import { isExpired, OPERATION_KIND } from "./model.ts"

/**
 * Review of one native sync plan (UI-10 pattern 6). Shows the exact ref operations and
 * approves only the digest/policy revision that is on screen; the native side decides.
 */
export function PlanReviewDialog({
  planId,
  open,
  onOpenChange,
}: {
  planId: string
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const plan = useNativeQuery(keys.plan(planId), "gbm_sync_plan_get", { planId }, { enabled: open, staleTime: 0 })
  const [busy, setBusy] = useState(false)

  return (
    <Dialog open={open} onOpenChange={(next) => (busy && !next ? undefined : onOpenChange(next))}>
      <DialogContent className="flex max-h-[calc(100dvh-3rem)] flex-col gap-0 p-0 sm:max-w-2xl" showCloseButton={!busy}>
        <DialogHeader className="border-b px-5 pt-5 pb-4">
          <DialogTitle className="flex items-center gap-2 text-base font-semibold">
            <Icon name="status.blocked" className="text-primary" />
            Review sync plan
          </DialogTitle>
          <DialogDescription>
            Exactly these ref changes will be applied to GitLab. Approval is bound to this plan version.
          </DialogDescription>
        </DialogHeader>
        {plan.isPending ? (
          <div className="flex flex-col gap-3 px-5 py-5" aria-busy="true">
            <LoadingLabel label="Loading plan" />
            <Skeleton className="h-4 w-1/2" />
            <Skeleton className="h-24 w-full" />
          </div>
        ) : plan.isError ? (
          <div className="px-5 py-5">
            <PlanError error={plan.error} onRetry={() => void plan.refetch()} retrying={plan.isFetching} />
          </div>
        ) : (
          <PlanReview
            key={`${plan.data.id}:${plan.data.planDigest}:${plan.data.policyRevision}`}
            plan={plan.data}
            onBusyChange={setBusy}
            onDone={() => onOpenChange(false)}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function PlanError({ error, onRetry, retrying }: { error: NativeCommandError; onRetry: () => void; retrying: boolean }) {
  if (error.appError.code === "PLAN_STALE") {
    return (
      <ErrorState
        error={error.appError}
        title="This plan is no longer current"
        action={<p className="text-[13px]">Create a new plan from the repository to review its current changes.</p>}
      />
    )
  }
  return <ErrorState error={error.appError} title="Could not load the plan" onRetry={onRetry} retrying={retrying} />
}

function PlanReview({ plan, onBusyChange, onDone }: { plan: SyncPlan; onBusyChange: (busy: boolean) => void; onDone: () => void }) {
  const port = useNativePort()
  const client = useQueryClient()
  const navigate = useNavigate()
  const [acknowledged, setAcknowledged] = useState(false)
  const expired = isExpired(plan.expiresAt)
  const destructive = plan.operations.filter((operation) => OPERATION_KIND[operation.kind].destructive)

  const queue = useMutation<{ jobId: string }, NativeCommandError>({
    mutationFn: async () => {
      let approvalId: string | null = null
      if (plan.requiresApproval) {
        const approval = await runNative(port, "gbm_sync_plan_approve", {
          planId: plan.id,
          expectedPlanDigest: plan.planDigest,
          expectedPolicyRevision: plan.policyRevision,
        })
        approvalId = approval.approvalId
      }
      return runNative(port, "gbm_sync_apply_enqueue", { planId: plan.id, approvalId })
    },
    onMutate: () => onBusyChange(true),
    onSettled: () => onBusyChange(false),
    onSuccess: async (result) => {
      toast.success("Sync queued", {
        description: `Job ${result.jobId} is queued; it is not complete until verification finishes.`,
        action: { label: "Sync Center", onClick: () => navigate("/sync") },
      })
      onDone()
      await Promise.all([...repositoryStateKeys, keys.plan(plan.id)].map((queryKey) => client.invalidateQueries({ queryKey })))
    },
  })

  const stale = queue.error?.appError.code === "PLAN_STALE"
  const blockedReason = expired
    ? "This plan expired. Create a new plan from the repository to continue."
    : stale
      ? "The plan changed since it was reviewed. Close this dialog and review the new plan."
      : plan.requiresApproval && !acknowledged
        ? "Confirm that you understand the history change to approve."
        : null

  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4 text-[13px]">
        {plan.requiresApproval ? (
          <div role="alert" className="flex items-start gap-2.5 rounded-lg border border-danger/30 bg-danger-soft px-3.5 py-2.5 text-danger">
            <Icon name="state.warning" className="mt-0.5" />
            <div>
              <p className="font-semibold">This plan changes existing history</p>
              <p className="text-danger/90">
                {plural(destructive.length, "operation")} of {plan.operations.length} will rewrite, move or delete refs that already
                exist on GitLab. Commits only reachable from the old values may become unreferenced there.
              </p>
            </div>
          </div>
        ) : null}

        <dl className="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-6 gap-y-2">
          <Fact term="Destination mapping">
            <span className="font-mono text-xs ui-selectable">{plan.destinationMappingId}</span>
          </Fact>
          <Fact term="Plan digest">
            <span className="font-mono text-xs ui-selectable" title={plan.planDigest}>
              {plan.planDigest}
            </span>
          </Fact>
          <Fact term="Policy revision">{plan.policyRevision}</Fact>
          <Fact term="Expires">
            <span className={expired ? "font-medium text-danger" : undefined} title={dateTime(plan.expiresAt) ?? undefined}>
              {expired ? "Expired" : relativeTime(plan.expiresAt) ?? "Unknown"} · {dateTime(plan.expiresAt) ?? "Unknown"}
            </span>
          </Fact>
          <Fact term="Recovery snapshot">
            {plan.preChangeSnapshotRequired
              ? "Required — the current GitLab refs are snapshotted before any change; the sync stops if the snapshot fails."
              : "Not required — no existing ref is rewritten or removed."}
          </Fact>
        </dl>

        <section aria-labelledby="plan-operations" className="flex min-w-0 flex-col gap-2">
          <h3 id="plan-operations" className="text-sm font-semibold">
            Ref operations ({plan.operations.length})
          </h3>
          <div className="rounded-lg border">
            <Table className="min-w-[520px] text-[13px]">
              <TableHeader className="text-xs">
                <TableRow className="hover:bg-transparent">
                  <TableHead className="pl-3 font-medium text-muted-foreground">Operation</TableHead>
                  <TableHead className="font-medium text-muted-foreground">Ref</TableHead>
                  <TableHead className="pr-3 font-medium text-muted-foreground">GitLab now → after</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {plan.operations.map((operation) => (
                  <OperationRow key={`${operation.kind}:${operation.refName}`} operation={operation} />
                ))}
              </TableBody>
            </Table>
          </div>
        </section>

        {plan.requiresApproval ? (
          <div className="flex items-start gap-2.5 rounded-lg border px-3.5 py-3">
            <Checkbox
              id={`ack-${plan.id}`}
              checked={acknowledged}
              disabled={expired || stale || queue.isPending}
              onCheckedChange={(checked) => setAcknowledged(checked)}
              className="mt-0.5"
            />
            <Label htmlFor={`ack-${plan.id}`} className="text-[13px] leading-snug font-normal">
              I understand this changes existing history on GitLab
            </Label>
          </div>
        ) : null}

        {queue.error && !stale ? (
          <ErrorState error={queue.error.appError} title="The sync was not queued" />
        ) : null}
      </div>
      <DialogFooter className="mx-0 mb-0 items-center px-5 py-3.5 sm:justify-between">
        <p id={`plan-reason-${plan.id}`} className="text-xs text-muted-foreground" aria-live="polite">
          {blockedReason ?? (queue.error && !stale ? ERROR_COPY[queue.error.appError.code] : "")}
        </p>
        <div className="flex gap-2">
          <Button variant="outline" onClick={onDone} disabled={queue.isPending}>
            Cancel
          </Button>
          <Button
            className={plan.requiresApproval ? "bg-danger text-white hover:bg-danger/90" : undefined}
            onClick={() => queue.mutate()}
            disabled={blockedReason !== null || queue.isPending}
            aria-describedby={`plan-reason-${plan.id}`}
          >
            <Icon name="action.sync" />
            {queue.isPending ? "Queuing…" : plan.requiresApproval ? "Approve & queue" : "Queue sync"}
          </Button>
        </div>
      </DialogFooter>
    </>
  )
}

function Fact({ term, children }: { term: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-muted-foreground">{term}</dt>
      <dd className="min-w-0 break-words">{children}</dd>
    </>
  )
}

function OperationRow({ operation }: { operation: RefOperation }) {
  const kind = OPERATION_KIND[operation.kind]
  return (
    <TableRow className="hover:bg-transparent">
      <TableCell className="pl-3">
        <StatusBadge status={kind} />
      </TableCell>
      <TableCell className="max-w-56 truncate font-mono text-xs" title={operation.refName}>
        {operation.refName}
      </TableCell>
      <TableCell className="pr-3 font-mono text-xs">
        <span className="inline-flex items-center gap-1.5">
          <Oid value={operation.expectedDestinationOid} missing="absent" />
          <Icon name="action.next" size={14} className="text-muted-foreground" />
          <Oid value={operation.sourceOid} missing="deleted" />
        </span>
      </TableCell>
    </TableRow>
  )
}

function Oid({ value, missing }: { value: string | null; missing: string }) {
  if (value === null) return <span className="font-sans text-muted-foreground italic">{missing}</span>
  return (
    <span title={value} className="ui-selectable">
      {shortOid(value)}
    </span>
  )
}
