// The renderer reaches native commands only through this port. Production injects the
// Tauri client from lib/native; isolated test/demo entries may inject a labeled fixture
// adapter. Screens never import either implementation directly.

import { createContext, useContext, type ReactNode } from "react"

import type { CommandMap, CommandName, CommandResult } from "@/lib/native"

export interface NativePort {
  /** "native" for the Tauri client; anything else must be visibly labeled in the UI. */
  readonly kind: "native" | "fixture"
  call<C extends CommandName>(command: C, args: CommandMap[C]["request"]): Promise<CommandResult<C>>
}

const PortContext = createContext<NativePort | null>(null)

export function NativePortProvider({ port, children }: { port: NativePort; children: ReactNode }) {
  return <PortContext.Provider value={port}>{children}</PortContext.Provider>
}

export function useNativePort(): NativePort {
  const port = useContext(PortContext)
  if (port === null) throw new Error("NativePortProvider is missing")
  return port
}
