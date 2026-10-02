import { Link } from "react-router"

import { EmptyState } from "@/components/shared"
import { Button } from "@/components/ui/button"

export function NotFound() {
  return (
    <EmptyState
      icon="status.unknown"
      title="This page does not exist"
      description="The link may point to a record that is no longer available."
      action={<Button render={<Link to="/" />}>Go to Dashboard</Button>}
    />
  )
}
