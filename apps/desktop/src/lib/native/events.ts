// `gbm://state` invalidation events (IPC.md "Event semantics"). Events are hints, never
// data: they only make the renderer re-query committed native state. Every event of one
// native run carries a strictly increasing sequence (`e-<run>.<seq>`), so a duplicate or
// reordered event is ignored and a lost one (gap) or a restarted backend forces a full
// re-query. Subscription happens before the snapshot is requested, so no change between
// the two can be missed.

import { listen } from "@tauri-apps/api/event"

import type { Event as StateEvent } from "./generated/contract.ts"
import { validateDefinition } from "./validate.ts"

export type { StateEvent }

export const STATE_CHANNEL = "gbm://state"

const EVENT_ID = /^e-([A-Za-z0-9]{1,32})\.([1-9][0-9]{0,15})$/

export interface ParsedStateEvent {
  event: StateEvent
  run: string
  sequence: number
}

/** Validates the payload against the contract and reads its run/sequence. */
export function parseStateEvent(raw: unknown): ParsedStateEvent | null {
  if (validateDefinition("Event", raw).length > 0) return null
  const event = raw as StateEvent
  const match = EVENT_ID.exec(event.eventId)
  if (match === null) return null
  const [, run, digits] = match
  if (run === undefined || digits === undefined) return null
  const sequence = Number(digits)
  if (!Number.isSafeInteger(sequence)) return null
  return { event, run, sequence }
}

export type RequeryReason = "subscribed" | "gap" | "restart" | "invalid"

export interface StateEventHandlers {
  /** Re-query what this committed change affects. */
  invalidate(event: StateEvent): void
  /** Re-query everything (start, lost/invalid event, backend restart). */
  requeryAll(reason: RequeryReason): void
}

/** Orders events of one subscription; see the module comment. */
export class StateEventTracker {
  private run: string | null = null
  private last = 0
  private stopped = false
  private readonly handlers: StateEventHandlers

  constructor(handlers: StateEventHandlers) {
    this.handlers = handlers
  }

  receive(raw: unknown): void {
    if (this.stopped) return
    const parsed = parseStateEvent(raw)
    if (parsed === null) {
      this.handlers.requeryAll("invalid")
      return
    }
    if (this.run !== parsed.run) {
      const restarted = this.run !== null
      this.run = parsed.run
      this.last = parsed.sequence
      // A first event right after subscribing is covered by the subscribe snapshot
      // only if it was committed before it; invalidate it to be sure.
      if (restarted) this.handlers.requeryAll("restart")
      else this.handlers.invalidate(parsed.event)
      return
    }
    if (parsed.sequence <= this.last) return // duplicate or reordered: already handled
    const gap = parsed.sequence > this.last + 1
    this.last = parsed.sequence
    if (gap) this.handlers.requeryAll("gap")
    else this.handlers.invalidate(parsed.event)
  }

  stop(): void {
    this.stopped = true
  }
}

export interface StateEventSource {
  /** Starts delivering raw payloads; resolves to the unsubscribe function. */
  subscribe(onPayload: (payload: unknown) => void): Promise<() => void>
}

/** The Tauri event channel (`core:event:default` listen permission only). */
export const tauriStateEvents: StateEventSource = {
  subscribe: (onPayload) =>
    listen<unknown>(STATE_CHANNEL, (event) => onPayload(event.payload)),
}

/**
 * Subscribes, then requests the snapshot (`requeryAll("subscribed")`). The returned
 * teardown unsubscribes; if teardown happens before the subscription resolved, the
 * subscription is released as soon as it does and no handler runs afterwards. If the
 * subscription fails, the snapshot is still requested and data is shown as last read.
 */
export function connectStateEvents(
  source: StateEventSource,
  handlers: StateEventHandlers
): () => void {
  const tracker = new StateEventTracker(handlers)
  let unsubscribe: (() => void) | null = null
  let tornDown = false
  source
    .subscribe((payload) => tracker.receive(payload))
    .then((stop) => {
      if (tornDown) stop()
      else unsubscribe = stop
    })
    .catch(() => undefined)
    .finally(() => {
      if (!tornDown) handlers.requeryAll("subscribed")
    })
  return () => {
    tornDown = true
    tracker.stop()
    unsubscribe?.()
    unsubscribe = null
  }
}

function revisionOf(value: unknown): number | null {
  if (typeof value !== "object" || value === null || !("revision" in value))
    return null
  const revision = (value as { revision: unknown }).revision
  return typeof revision === "number" && Number.isSafeInteger(revision)
    ? revision
    : null
}

/**
 * True if `incoming` is a read model from an older committed revision than `current`
 * (a slow query that started before a newer one finished). It must not replace it.
 */
export function isOlderRevision(current: unknown, incoming: unknown): boolean {
  const before = revisionOf(current)
  const after = revisionOf(incoming)
  return before !== null && after !== null && after < before
}
