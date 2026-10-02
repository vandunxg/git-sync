import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { useState } from "react"
import { RouterProvider } from "react-router/dom"

import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { NativePortProvider, type NativePort } from "@/lib/api"

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

/** Root providers. The port decides native vs. labeled fixture data; screens do not. */
export function App({ port }: { port: NativePort }) {
  const [client] = useState(createQueryClient)
  const [router] = useState(createAppRouter)
  return (
    <NativePortProvider port={port}>
      <QueryClientProvider client={client}>
        <TooltipProvider>
          <RouterProvider router={router} />
          <Toaster position="bottom-right" visibleToasts={3} />
        </TooltipProvider>
      </QueryClientProvider>
    </NativePortProvider>
  )
}
