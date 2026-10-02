import type { AppError, CommandName } from "@/lib/native"

/** A typed native failure carried through TanStack Query; never converted to success. */
export class NativeCommandError extends Error {
  readonly appError: AppError
  readonly command: CommandName

  constructor(command: CommandName, appError: AppError) {
    super(appError.message)
    this.name = "NativeCommandError"
    this.appError = appError
    this.command = command
  }
}

export function toAppError(error: unknown): AppError | null {
  return error instanceof NativeCommandError ? error.appError : null
}

/** Short, actionable copy per stable error code (INTERACTION_AND_COPY.md). */
export const ERROR_COPY: Record<AppError["code"], string> = {
  AUTH_REQUIRED: "The connection needs a new token.",
  ACCESS_DENIED: "The token does not grant access to this resource.",
  RATE_LIMITED: "The provider is rate limiting requests.",
  NETWORK_UNAVAILABLE: "The network is unavailable.",
  TIMEOUT: "The operation timed out.",
  GIT_UNAVAILABLE: "Git is not installed or could not be started.",
  GIT_LFS_UNAVAILABLE: "Git LFS is not installed.",
  KEYRING_UNAVAILABLE: "The system keyring is unavailable or locked.",
  PATH_REJECTED: "This folder cannot be used for backups.",
  ROOT_RELOCATION_UNSUPPORTED: "Moving the backup folder is not supported in this version.",
  SOURCE_UNAVAILABLE: "Source unavailable; backups are retained.",
  PARTIAL_DISCOVERY: "Discovery is incomplete; some repositories may be missing.",
  PLAN_STALE: "The plan changed since it was reviewed.",
  APPROVAL_REQUIRED: "This plan changes existing history and needs review.",
  DESTINATION_DIVERGED: "The GitLab copy has diverged from the source.",
  PROTECTED_REF: "A protected branch blocked the update.",
  PUSH_PARTIAL: "Only some refs were pushed.",
  VERIFY_FAILED: "Backup verification failed.",
  DISK_FULL: "The backup disk is full.",
  MIRROR_INVALID: "The local mirror is invalid.",
  WORKSPACE_SKIPPED: "Your local edits were not changed; the workspace update was skipped.",
  LFS_INCOMPLETE: "Git refs synced; LFS payloads incomplete.",
  SUBMODULE_UNCOVERED: "Submodules are not covered by this backup.",
  CANCEL_UNCERTAIN: "Cancellation requested; checking destination.",
  CONTRACT_MISMATCH: "The application returned an unexpected response.",
  UNSUPPORTED: "This action is not available in this build yet.",
  INTERNAL: "The native application did not respond.",
}
