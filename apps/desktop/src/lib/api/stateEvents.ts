// Bridges native `gbm://state` invalidations to the Query cache: a committed change
// invalidates the read models it affects; start, gaps and backend restarts re-query all.

import {
  useQueryClient,
  type QueryClient,
  type QueryKey,
} from "@tanstack/react-query"
import { useEffect } from "react"

import {
  connectStateEvents,
  type StateEvent,
  type StateEventSource,
} from "@/lib/native/events.ts"

import { keys, repositoryStateKeys } from "./keys.ts"

/** Query key prefixes a committed change of this type can affect. */
export function invalidationKeys(event: StateEvent): QueryKey[] {
  const repository =
    event.repositoryId === null ? [] : [keys.refs(event.repositoryId)]
  switch (event.type) {
    case "catalog_changed":
    case "repository_changed":
      return [...repositoryStateKeys, ["refs"], ["plan"], keys.bootstrap]
    case "job_changed":
      return [
        ...repositoryStateKeys,
        ...repository,
        ["plan"],
        ["logs"],
        keys.queuePaused,
      ]
    case "queue_changed":
      return [keys.jobs, keys.queuePaused, keys.dashboard]
    case "settings_changed":
      return [keys.settings, keys.bootstrap, keys.diagnostics]
    case "account_changed":
      return [
        keys.accounts,
        keys.bootstrap,
        ["namespaces"],
        ...repositoryStateKeys,
      ]
  }
}

export function stateEventHandlers(client: QueryClient) {
  return {
    invalidate: (event: StateEvent) => {
      for (const queryKey of invalidationKeys(event))
        void client.invalidateQueries({ queryKey })
    },
    requeryAll: () => void client.invalidateQueries(),
  }
}

/** Keeps the Query cache in step with committed native state while mounted. */
export function useStateEvents(source: StateEventSource | null) {
  const client = useQueryClient()
  useEffect(() => {
    if (source === null) return
    return connectStateEvents(source, stateEventHandlers(client))
  }, [client, source])
}
