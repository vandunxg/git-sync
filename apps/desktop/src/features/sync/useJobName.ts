import { JOB_KIND } from "@/components/shared"
import { keys, useNativeQuery } from "@/lib/api"
import { splitFullName } from "@/lib/format"
import type { Job } from "@/lib/native"

export interface JobName {
  /** Short display name (repository name or job kind for account-wide jobs). */
  name: string
  /** Full identity for the title attribute. */
  full: string
  pending: boolean
}

/** Resolves a job's repositoryId to its name; account-wide jobs use their kind label. */
export function useJobName(job: Pick<Job, "repositoryId" | "kind">): JobName {
  const repositoryId = job.repositoryId
  const repository = useNativeQuery(keys.repository(repositoryId ?? ""), "gbm_repository_get", { repositoryId: repositoryId ?? "" }, {
    enabled: repositoryId !== null,
    staleTime: 60_000,
  })
  if (repositoryId === null) return { name: JOB_KIND[job.kind], full: JOB_KIND[job.kind], pending: false }
  if (repository.data) {
    return { name: splitFullName(repository.data.fullName).name, full: repository.data.fullName, pending: false }
  }
  if (repository.isPending) return { name: "Loading repository…", full: repositoryId, pending: true }
  return { name: "Repository unavailable", full: `Repository ${repositoryId} could not be loaded`, pending: false }
}
