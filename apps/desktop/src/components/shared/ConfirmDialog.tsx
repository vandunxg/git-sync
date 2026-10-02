import type { ReactNode } from "react"

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Icon, type IconKey } from "@/lib/icons"
import { cn } from "@/lib/utils"

/**
 * Specific confirmation: the body must state the exact effect. One primary action,
 * Cancel restores focus to the trigger, and the action stays disabled while pending.
 */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  confirmLabel,
  pendingLabel,
  onConfirm,
  pending = false,
  disabled = false,
  destructive = false,
  icon = "state.confirm",
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: ReactNode
  children?: ReactNode
  confirmLabel: string
  pendingLabel?: string
  onConfirm: () => void
  pending?: boolean
  disabled?: boolean
  destructive?: boolean
  icon?: IconKey
}) {
  return (
    <AlertDialog open={open} onOpenChange={(next) => (!pending || next ? onOpenChange(next) : undefined)}>
      <AlertDialogContent className="max-h-[calc(100dvh-4rem)] overflow-hidden data-[size=default]:sm:max-w-md">
        <AlertDialogHeader>
          <AlertDialogMedia className={cn(destructive ? "bg-danger-soft text-danger" : "bg-primary-soft text-primary")}>
            <Icon name={icon} size={20} />
          </AlertDialogMedia>
          <AlertDialogTitle className="font-semibold">{title}</AlertDialogTitle>
          <AlertDialogDescription render={<div />}>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        {children ? <div className="-mx-4 max-h-80 overflow-y-auto px-4 text-[13px]">{children}</div> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={pending}>Cancel</AlertDialogCancel>
          <Button
            variant={destructive ? "destructive" : "default"}
            onClick={onConfirm}
            disabled={pending || disabled}
          >
            {pending ? (pendingLabel ?? `${confirmLabel}…`) : confirmLabel}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
