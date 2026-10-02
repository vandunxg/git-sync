import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { useState } from "react"
import { RouterProvider } from "react-router/dom"

import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { NativePortProvider, useStateEvents, type NativePort } from "@/lib/api"
import type { StateEventSource } from "@/lib/native/events.ts"

import { createAppRouter } from "./router.tsx"

function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: true, staleTime: 15_000 },
      // gcTime 0: mutation variables (e.g. a PAT) are dropped as soon as the dialog unmounts.
      mutations: { retry: false, gcTime: 0 },
    },
  })
}

/** Re-queries committed native state on `gbm://state` invalidations. */
function StateEventBridge({ source }: { source: StateEventSource | null }) {
  useStateEvents(source)
  return null
}

/** Root providers. The port decides native vs. labeled fixture data; screens do not.
 * `events` is the native invalidation channel (null for fixture entries). */
export function App({
  port,
  events = null,
}: {
  port: NativePort
  events?: StateEventSource | null
}) {
  const [client] = useState(createQueryClient)
  const [router] = useState(createAppRouter)
  return (
    <NativePortProvider port={port}>
      <QueryClientProvider client={client}>
        <StateEventBridge source={events} />
        <TooltipProvider>
          <RouterProvider router={router} />
          <Toaster position="bottom-right" visibleToasts={3} />
        </TooltipProvider>
      </QueryClientProvider>
    </NativePortProvider>
  )
}
