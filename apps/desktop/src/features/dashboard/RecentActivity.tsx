import { Link } from "react-router"

import { activityVisual, ACTIVITY_TYPE_LABEL, ErrorState, IconTile, RowSkeletons, SectionCard, SEVERITY } from "@/components/shared"
import { Button } from "@/components/ui/button"
import { keys, useNativeQuery } from "@/lib/api"
import { dateTime, relativeTime } from "@/lib/format"
import { Icon } from "@/lib/icons"
import type { ActivityEntry, ActivityRequest } from "@/lib/native"

const RECENT_ARGS: ActivityRequest = { cursor: null, limit: 6, repositoryId: null, severity: "all", search: "" }

function ActivityRow({ entry }: { entry: ActivityEntry }) {
  const visual = activityVisual(entry)
  const when = relativeTime(entry.occurredAt)
  const kind = entry.severity === "info" ? ACTIVITY_TYPE_LABEL[entry.type] : `${ACTIVITY_TYPE_LABEL[entry.type]} · ${SEVERITY[entry.severity].label}`
  return (
    <li>
      <Link
        to={`/activity?event=${encodeURIComponent(entry.id)}`}
        className="-mx-2 flex min-w-0 items-start gap-3 rounded-lg px-2 py-2 outline-none hover:bg-muted/60 focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <IconTile icon={visual.icon} tone={visual.tone} size="sm" />
        <span className="min-w-0 flex-1">
          <span className="line-clamp-2 text-[13px] leading-snug font-medium" title={entry.message}>
            {entry.message}
          </span>
          <span className="mt-0.5 block truncate text-xs text-muted-foreground">
            {kind} ·{" "}
            <time dateTime={entry.occurredAt} title={dateTime(entry.occurredAt) ?? undefined}>
              {when ?? "Unknown time"}
            </time>
          </span>
        </span>
      </Link>
    </li>
  )
}

export function RecentActivity() {
  const activity = useNativeQuery([...keys.activity, RECENT_ARGS], "gbm_activity_list", RECENT_ARGS)

  let body
  if (activity.isPending) {
    body = <RowSkeletons rows={5} />
  } else if (activity.isError && !activity.data) {
    body = (
      <ErrorState
        error={activity.error.appError}
        title="Could not load recent activity"
        onRetry={() => void activity.refetch()}
        retrying={activity.isFetching}
      />
    )
  } else if (activity.data.items.length === 0) {
    body = (
      <div className="flex items-center gap-3 rounded-lg bg-muted/50 px-4 py-3 text-[13px] text-muted-foreground">
        <Icon name="state.empty" />
        No activity recorded yet.
      </div>
    )
  } else {
    body = (
      <ul className="flex flex-col gap-0.5" aria-label="Recent activity">
        {activity.data.items.map((entry) => (
          <ActivityRow key={entry.id} entry={entry} />
        ))}
      </ul>
    )
  }

  return (
    <SectionCard
      icon="nav.activity"
      title="Recent activity"
      description="Latest recorded events"
      action={
        <Button variant="ghost" size="sm" nativeButton={false} render={<Link to="/activity" />}>
          View all
          <Icon name="action.next" size={14} />
        </Button>
      }
    >
      {body}
    </SectionCard>
  )
}
