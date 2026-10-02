import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Icon } from "@/lib/icons"

/** Copies only the given OID/log text; never anything credential-bearing. */
export function CopyButton({ value, label, children }: { value: string; label: string; children?: string }) {
  const copy = () => {
    navigator.clipboard.writeText(value).then(
      () => toast.success("Copied to clipboard"),
      () => toast.error("Could not copy", { description: "The clipboard is not available." }),
    )
  }
  if (children) {
    return (
      <Button variant="outline" size="sm" onClick={copy} aria-label={label}>
        <Icon name="action.copy" size={14} />
        {children}
      </Button>
    )
  }
  return (
    <Button variant="ghost" size="icon-xs" onClick={copy} aria-label={label} title={label}>
      <Icon name="action.copy" size={12} />
    </Button>
  )
}
