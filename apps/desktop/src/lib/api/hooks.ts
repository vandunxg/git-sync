import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryKey,
  type UseQueryOptions,
} from "@tanstack/react-query"

import type { CommandMap, CommandName } from "@/lib/native"

import { NativeCommandError } from "./errors.ts"
import { useNativePort, type NativePort } from "./port.tsx"

type Response<C extends CommandName> = CommandMap[C]["response"]
type Request<C extends CommandName> = CommandMap[C]["request"]

export async function runNative<C extends CommandName>(
  port: NativePort,
  command: C,
  args: Request<C>,
): Promise<Response<C>> {
  const result = await port.call(command, args)
  if (!result.ok) throw new NativeCommandError(command, result.error)
  return result.data
}

/** Query a read-model command; failures stay typed and keep the last cached data. */
export function useNativeQuery<C extends CommandName>(
  key: QueryKey,
  command: C,
  args: Request<C>,
  options?: Omit<UseQueryOptions<Response<C>, NativeCommandError>, "queryKey" | "queryFn">,
) {
  const port = useNativePort()
  return useQuery<Response<C>, NativeCommandError>({
    queryKey: key,
    queryFn: () => runNative(port, command, args),
    ...options,
  })
}

/** Mutation for one command. Invalidates the given keys only after a native success. */
export function useNativeMutation<C extends CommandName>(command: C, invalidate: QueryKey[] = []) {
  const port = useNativePort()
  const client = useQueryClient()
  return useMutation<Response<C>, NativeCommandError, Request<C>>({
    mutationFn: (args) => runNative(port, command, args),
    onSuccess: async () => {
      await Promise.all(invalidate.map((queryKey) => client.invalidateQueries({ queryKey })))
    },
  })
}
