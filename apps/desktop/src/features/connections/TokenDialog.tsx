import { zodResolver } from "@hookform/resolvers/zod"
import { useId, useState } from "react"
import { useForm, useWatch } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

import { ErrorState } from "@/components/shared"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { keys, repositoryStateKeys, useNativeMutation } from "@/lib/api"
import { BrandMark, Icon, PROVIDER_LABEL } from "@/lib/icons"
import type { AccountSummary, Provider } from "@/lib/native"
import { cn } from "@/lib/utils"

import { PROVIDER_COPY } from "./providerCopy.ts"

const schema = z.object({
  token: z.string().trim().min(1, "Paste a personal access token to continue.").max(2048, "This token is too long."),
  storageMode: z.enum(["keyring", "session_only"]),
})

type TokenFormValues = z.input<typeof schema>

const STORAGE_OPTIONS = [
  {
    value: "keyring",
    title: "System keyring (recommended)",
    description: "The token is kept in your operating system's keyring and survives restarts.",
  },
  {
    value: "session_only",
    title: "Session only — cleared when the app quits",
    description: "Nothing is written to disk. After a restart, scheduled backups stop until you connect again.",
  },
] as const

export interface TokenDialogProps {
  provider: Provider
  open: boolean
  onOpenChange: (open: boolean) => void
  onConnected?: (account: AccountSummary) => void
  mode?: "connect" | "replace"
}

/**
 * Masked PAT entry (DD-03: "Connect with token", never "Authorize"). The form lives in a
 * child that unmounts when the dialog closes, so the token never outlives the dialog.
 */
export function TokenDialog({ provider, open, onOpenChange, onConnected, mode = "connect" }: TokenDialogProps) {
  const [pending, setPending] = useState(false)
  const label = PROVIDER_LABEL[provider]
  return (
    <Dialog open={open} onOpenChange={(next) => (pending && !next ? undefined : onOpenChange(next))}>
      <DialogContent className="max-h-[calc(100dvh-2rem)] grid-rows-[auto_minmax(0,1fr)] overflow-hidden p-0 sm:max-w-lg">
        <DialogHeader className="border-b px-5 pt-5 pb-4">
          <div className="flex items-center gap-3 pr-8">
            <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-lg border bg-card">
              <BrandMark provider={provider} size={20} />
            </span>
            <div className="min-w-0">
              <DialogTitle className="text-[15px] font-semibold">
                {mode === "replace" ? `Replace ${label} token` : `Connect ${label} with a token`}
              </DialogTitle>
              <DialogDescription className="mt-1 text-[13px]">
                {mode === "replace"
                  ? "The new token is validated before it replaces the current one. If validation fails, nothing changes."
                  : `The token is validated with ${label} before it is stored.`}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <TokenForm
          provider={provider}
          mode={mode}
          onPendingChange={setPending}
          onCancel={() => onOpenChange(false)}
          onConnected={(account) => {
            onConnected?.(account)
            onOpenChange(false)
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

function TokenForm({
  provider,
  mode,
  onPendingChange,
  onCancel,
  onConnected,
}: {
  provider: Provider
  mode: "connect" | "replace"
  onPendingChange: (pending: boolean) => void
  onCancel: () => void
  onConnected: (account: AccountSummary) => void
}) {
  const ids = useId()
  const tokenId = `${ids}-token`
  const helpId = `${ids}-help`
  const errorId = `${ids}-error`
  const [reveal, setReveal] = useState(false)
  const copy = PROVIDER_COPY[provider]
  const label = PROVIDER_LABEL[provider]
  const mutation = useNativeMutation("gbm_credentials_validate_store", [
    keys.accounts,
    keys.bootstrap,
    keys.diagnostics,
    ...repositoryStateKeys,
  ])
  const form = useForm<TokenFormValues>({
    resolver: zodResolver(schema),
    defaultValues: { token: "", storageMode: "keyring" },
  })
  const storageMode = useWatch({ control: form.control, name: "storageMode" })
  const tokenError = form.formState.errors.token?.message
  const appError = mutation.error?.appError ?? null

  const submit = form.handleSubmit((values) => {
    onPendingChange(true)
    mutation.mutate(
      { provider, token: values.token.trim(), storageMode: values.storageMode },
      {
        onSuccess: (result) => {
          form.reset({ token: "", storageMode: values.storageMode })
          toast.success(mode === "replace" ? `${label} token replaced` : `${label} connected`, {
            description: `Signed in as ${result.account.username}.`,
          })
          onConnected(result.account)
        },
        onSettled: () => onPendingChange(false),
      },
    )
  })

  const tokenField = form.register("token")

  return (
    <form onSubmit={submit} noValidate className="flex min-h-0 flex-col">
      <div className="flex min-h-0 flex-col gap-5 overflow-y-auto px-5 py-4">
        <div className="flex flex-col gap-2">
          <label htmlFor={tokenId} className="text-[13px] font-medium">
            Personal access token
          </label>
          <div className="flex gap-2">
            <input
              id={tokenId}
              type={reveal ? "text" : "password"}
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck={false}
              data-1p-ignore
              data-lpignore="true"
              readOnly={mutation.isPending}
              aria-invalid={tokenError ? true : undefined}
              aria-describedby={tokenError ? `${errorId} ${helpId}` : helpId}
              className="h-9 w-full min-w-0 rounded-lg border border-input bg-card px-3 font-mono text-[13px] outline-none placeholder:font-sans placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20"
              placeholder={`Paste your ${label} token`}
              {...tokenField}
            />
            <Button
              type="button"
              variant="outline"
              aria-pressed={reveal}
              aria-controls={tokenId}
              onClick={() => setReveal((value) => !value)}
              className="w-[68px]"
            >
              {reveal ? "Hide" : "Show"}
            </Button>
          </div>
          {tokenError ? (
            <p id={errorId} className="text-xs font-medium text-danger">
              {tokenError}
            </p>
          ) : null}
          <div id={helpId} className="flex flex-col gap-1.5 rounded-lg bg-muted/60 px-3 py-2.5 text-xs leading-relaxed text-muted-foreground">
            <p className="flex gap-2">
              <Icon name="event.auth" size={14} className="mt-0.5 text-foreground/70" />
              <span>{copy.scopeHelp}</span>
            </p>
            <p className="pl-[22px]">
              How to create a token:{" "}
              <span className="break-all text-foreground/80 underline decoration-dotted underline-offset-2 ui-selectable">{copy.docsUrl}</span>
            </p>
          </div>
        </div>

        <fieldset className="flex flex-col gap-2" disabled={mutation.isPending}>
          <legend className="mb-2 text-[13px] font-medium">Where to keep the token</legend>
          {STORAGE_OPTIONS.map((option) => (
            <label
              key={option.value}
              className={cn(
                "flex cursor-pointer gap-3 rounded-lg border px-3 py-2.5 transition-colors has-focus-visible:ring-3 has-focus-visible:ring-ring/50",
                storageMode === option.value ? "border-primary/40 bg-primary-soft/60" : "hover:bg-muted/50",
              )}
            >
              <input
                type="radio"
                value={option.value}
                className="mt-0.5 size-4 shrink-0 accent-primary outline-none"
                {...form.register("storageMode")}
              />
              <span className="flex flex-col gap-0.5">
                <span className="text-[13px] font-medium">{option.title}</span>
                <span className="text-xs text-muted-foreground">{option.description}</span>
              </span>
            </label>
          ))}
        </fieldset>

        {appError ? (
          <ErrorState
            error={appError}
            title={mode === "replace" ? "The token was not replaced" : `${label} was not connected`}
            action={
              appError.code === "KEYRING_UNAVAILABLE" && storageMode === "keyring" ? (
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => form.setValue("storageMode", "session_only", { shouldDirty: true })}
                >
                  Use session-only storage
                </Button>
              ) : undefined
            }
          />
        ) : null}
        {appError?.code === "KEYRING_UNAVAILABLE" ? (
          <p className="-mt-2 text-xs text-muted-foreground">
            The token is never saved in plain text. Choose session-only storage to continue now, or unlock the system keyring
            and try again.
          </p>
        ) : null}
      </div>
      <DialogFooter className="mx-0 mb-0 px-5 py-3">
        <Button type="button" variant="outline" onClick={onCancel} disabled={mutation.isPending}>
          Cancel
        </Button>
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending ? (
            <>
              <Spinner aria-hidden="true" />
              Validating…
            </>
          ) : mode === "replace" ? (
            "Replace token"
          ) : (
            "Connect with token"
          )}
        </Button>
      </DialogFooter>
    </form>
  )
}
