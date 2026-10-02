import { useNavigate } from "react-router"

import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { BrandMark, Icon } from "@/lib/icons"
import type { RepositorySummary } from "@/lib/native"

import { useRepositoryCommands } from "./hooks.ts"
import { gitlabOpenBlocker, primaryActionFor, workspaceOpenBlocker } from "./model.ts"

/** Row menu with only implemented actions (UI-04): detail, sync/retry/review, safe openers, inclusion. */
export function RepositoryActionsMenu({ repo, listRevision }: { repo: RepositorySummary; listRevision: number }) {
  const navigate = useNavigate()
  const commands = useRepositoryCommands()
  const action = primaryActionFor(repo)
  const detailPath = `/repositories/${encodeURIComponent(repo.id)}`
  const folderBlocker = workspaceOpenBlocker(repo)
  const gitlabBlocker = gitlabOpenBlocker(repo)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="ghost" size="icon-sm" aria-label={`Actions for ${repo.fullName}`} />}
        onClick={(event) => event.stopPropagation()}
      >
        <Icon name="action.more" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-60" onClick={(event) => event.stopPropagation()}>
        <DropdownMenuGroup>
          <DropdownMenuLabel className="truncate" title={repo.fullName}>
            {repo.fullName}
          </DropdownMenuLabel>
          <DropdownMenuItem onClick={() => navigate(detailPath)}>
            <Icon name="nav.repositories" />
            Open details
          </DropdownMenuItem>
          {action === "sync" || action === "retry" ? (
            <DropdownMenuItem disabled={commands.createPlan.isPending} onClick={() => commands.createPlan.mutate(repo.id)}>
              <Icon name={action === "retry" ? "action.retry" : "action.sync"} />
              {action === "retry" ? "Retry sync" : "Sync now"}
            </DropdownMenuItem>
          ) : null}
          {action === "review" ? (
            <DropdownMenuItem onClick={() => navigate(`${detailPath}?review=1`)}>
              <Icon name="status.blocked" />
              Review changes
            </DropdownMenuItem>
          ) : null}
          {action === "reconnect" ? (
            <DropdownMenuItem onClick={() => navigate("/connections")}>
              <Icon name="action.reconnect" />
              {repo.destinationState === "inaccessible" ? "Reconnect GitLab" : "Reconnect"}
            </DropdownMenuItem>
          ) : null}
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem
            disabled={folderBlocker !== null}
            title={folderBlocker ?? undefined}
            onClick={() => commands.openWorkspace.mutate(repo.id)}
          >
            <Icon name="action.open_folder" />
            <span className="flex flex-col">
              Open folder
              {folderBlocker ? <span className="text-xs text-muted-foreground">{folderBlocker}</span> : null}
            </span>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => commands.openProvider.mutate({ repositoryId: repo.id, provider: "github" })}>
            <BrandMark provider="github" size={16} />
            Open on GitHub
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={gitlabBlocker !== null}
            onClick={() => commands.openProvider.mutate({ repositoryId: repo.id, provider: "gitlab" })}
          >
            <BrandMark provider="gitlab" size={16} />
            <span className="flex flex-col">
              Open on GitLab
              {gitlabBlocker ? <span className="text-xs text-muted-foreground">{gitlabBlocker}</span> : null}
            </span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          disabled={commands.setIncluded.isPending}
          onClick={() =>
            commands.setIncluded.mutate({ repositories: [repo], selected: !repo.selected, expectedRevision: listRevision })
          }
        >
          <Icon name={repo.selected ? "status.no_changes" : "event.created"} />
          {repo.selected ? "Exclude from backup" : "Include in backup"}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
