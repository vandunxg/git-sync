// TEST-EVENTS / TEST-FRESHNESS (renderer side): subscribe-then-snapshot, duplicate and
// reordered events ignored, gaps / restarts / invalid payloads re-query everything,
// teardown unsubscribes, and an older read model never replaces a newer one.

import { QueryClient } from "@tanstack/react-query"
import { describe, expect, it } from "vitest"

import { invalidationKeys, keepNewestRevision, keys } from "@/lib/api"
import type { RepositorySummary } from "@/lib/native"
import {
  connectStateEvents,
  isOlderRevision,
  parseStateEvent,
  StateEventTracker,
  type StateEvent,
  type StateEventSource,
} from "@/lib/native/events.ts"
import { validateDefinition } from "@/lib/native/validate.ts"

function event(
  seq: number,
  revision: number,
  run = "run1",
  extra: Partial<StateEvent> = {}
): StateEvent {
  return {
    schemaVersion: 1,
    eventId: `e-${run}.${seq}`,
    revision,
    occurredAt: "2026-10-02T12:34:56.789Z",
    type: "job_changed",
    repositoryId: "repo-1",
    jobId: "job-1",
    ...extra,
  }
}

function recorder() {
  const log: string[] = []
  return {
    log,
    handlers: {
      invalidate: (e: StateEvent) => log.push(`invalidate ${e.eventId}`),
      requeryAll: (reason: string) => log.push(`requery ${reason}`),
    },
  }
}

describe("state event parsing", () => {
  it("accepts the exact shape the native publisher emits", () => {
    // Same fields as crates/application events::tests (e-run1.1, revision 41).
    const native = event(1, 41, "run1", {
      jobId: "job-7",
      repositoryId: "repo-3",
    })
    expect(validateDefinition("Event", native)).toEqual([])
    expect(parseStateEvent(native)).toEqual({
      event: native,
      run: "run1",
      sequence: 1,
    })
  })

  it("rejects contract violations and foreign event ids", () => {
    for (const bad of [
      { ...event(1, 1), token: "x" },
      { ...event(1, -1) },
      { ...event(1, 1), eventId: "run1-1" },
      { ...event(1, 1), eventId: "e-run1.0" },
      { ...event(1, 1), eventId: "e-run.1.2" },
      { ...event(1, 1), type: "progress" },
      null,
      "e-run1.1",
    ]) {
      expect(parseStateEvent(bad)).toBeNull()
    }
  })
})

describe("event ordering", () => {
  it("ignores duplicates and reordered events and re-queries on a gap", () => {
    const { log, handlers } = recorder()
    const tracker = new StateEventTracker(handlers)
    tracker.receive(event(4, 10))
    tracker.receive(event(5, 11))
    tracker.receive(event(5, 11)) // duplicate
    tracker.receive(event(3, 9)) // older, reordered
    tracker.receive(event(8, 15)) // 6 and 7 lost
    tracker.receive(event(7, 14)) // late arrival of a lost one
    tracker.receive(event(9, 16))
    expect(log).toEqual([
      "invalidate e-run1.4",
      "invalidate e-run1.5",
      "requery gap",
      "invalidate e-run1.9",
    ])
  })

  it("re-queries everything after a backend restart or an invalid payload", () => {
    const { log, handlers } = recorder()
    const tracker = new StateEventTracker(handlers)
    tracker.receive(event(1, 10))
    tracker.receive(event(1, 2, "run2"))
    tracker.receive({ garbage: true })
    tracker.receive(event(2, 3, "run2"))
    expect(log).toEqual([
      "invalidate e-run1.1",
      "requery restart",
      "requery invalid",
      "invalidate e-run2.2",
    ])
  })
})

function fakeSource() {
  let deliver: ((payload: unknown) => void) | null = null
  let release: ((stop: () => void) => void) | null = null
  const order: string[] = []
  const source: StateEventSource = {
    subscribe(onPayload) {
      order.push("subscribe")
      deliver = onPayload
      return new Promise((resolve) => {
        release = resolve
      })
    },
  }
  return {
    source,
    order,
    emit: (payload: unknown) => deliver?.(payload),
    resolve: () => release?.(() => order.push("unsubscribe")),
  }
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe("subscription lifecycle", () => {
  it("subscribes before requesting the snapshot and unsubscribes on teardown", async () => {
    const fake = fakeSource()
    const { log, handlers } = recorder()
    const teardown = connectStateEvents(fake.source, {
      ...handlers,
      requeryAll: (reason) => {
        fake.order.push(`snapshot ${reason}`)
        handlers.requeryAll(reason)
      },
    })
    // A change committed while subscribing is still seen.
    fake.emit(event(1, 5))
    fake.resolve()
    await flush()
    expect(fake.order).toEqual(["subscribe", "snapshot subscribed"])
    teardown()
    fake.emit(event(2, 6))
    expect(fake.order).toEqual([
      "subscribe",
      "snapshot subscribed",
      "unsubscribe",
    ])
    expect(log).toEqual(["invalidate e-run1.1", "requery subscribed"])
  })

  it("releases a subscription that resolves after teardown", async () => {
    const fake = fakeSource()
    const { log, handlers } = recorder()
    const teardown = connectStateEvents(fake.source, handlers)
    teardown()
    fake.resolve()
    await flush()
    fake.emit(event(1, 5))
    expect(fake.order).toEqual(["subscribe", "unsubscribe"])
    expect(log).toEqual([])
  })

  it("still loads the snapshot if event delivery is unavailable", async () => {
    const { log, handlers } = recorder()
    connectStateEvents(
      { subscribe: () => Promise.reject(new Error("no channel")) },
      handlers
    )
    await flush()
    expect(log).toEqual(["requery subscribed"])
  })
})

function summary(revisionTag: Partial<RepositorySummary>): RepositorySummary {
  return {
    id: "repo-1",
    providerRepositoryId: "42",
    fullName: "octo/tool",
    defaultBranch: "main",
    isPrivate: true,
    isFork: false,
    isArchived: false,
    selected: true,
    sourceState: "unknown",
    mirrorState: "unknown",
    destinationState: "unknown",
    workspaceState: "disabled",
    primaryStatus: "unknown",
    pendingRefChanges: null,
    observedAt: null,
    verifiedAt: null,
    lastErrorCode: null,
    coverage: {
      gitRefs: "not_checked",
      localObjects: "not_checked",
      lfs: "not_checked",
      submodules: "none",
    },
    ...revisionTag,
  }
}

describe("freshness", () => {
  it("an older snapshot never regresses newer data or fakes verification", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    })
    const key = keys.repositories
    const newer = {
      items: [summary({ primaryStatus: "stale", sourceState: "stale" })],
      revision: 12,
    }
    const older = {
      items: [
        summary({
          primaryStatus: "synced",
          destinationState: "current",
          verifiedAt: "2026-10-01T00:00:00.000Z",
        }),
      ],
      revision: 9,
    }
    const fetch = (data: unknown) =>
      client.fetchQuery({
        queryKey: key,
        queryFn: () => data,
        structuralSharing: keepNewestRevision,
        staleTime: 0,
      })
    await fetch(newer)
    await fetch(older)
    expect(client.getQueryData(key)).toEqual(newer)
    expect(isOlderRevision(newer, older)).toBe(true)
    // Same or newer revisions replace; payloads without a revision are not judged.
    const same = { ...newer, items: [summary({ primaryStatus: "unknown" })] }
    await fetch(same)
    expect(client.getQueryData(key)).toEqual(same)
    expect(isOlderRevision({ revision: 3 }, { items: [] })).toBe(false)
  })

  it("maps each change type to the read models it affects", () => {
    const job = invalidationKeys(event(1, 1))
    expect(job).toContainEqual(keys.jobs)
    expect(job).toContainEqual(keys.dashboard)
    expect(job).toContainEqual(keys.refs("repo-1"))
    expect(job).toContainEqual(["plan"])
    expect(job).toContainEqual(["logs"])
    expect(
      invalidationKeys(event(1, 1, "run1", { type: "catalog_changed" }))
    ).toContainEqual(["plan"])
    expect(
      invalidationKeys(event(1, 1, "run1", { type: "settings_changed" }))
    ).toContainEqual(keys.settings)
    expect(
      invalidationKeys(event(1, 1, "run1", { type: "queue_changed" }))
    ).toContainEqual(keys.queuePaused)
    expect(
      invalidationKeys(event(1, 1, "run1", { type: "account_changed" }))
    ).toContainEqual(keys.accounts)
    expect(
      invalidationKeys(event(1, 1, "run1", { type: "catalog_changed" }))
    ).toContainEqual(keys.repositories)
  })
})
