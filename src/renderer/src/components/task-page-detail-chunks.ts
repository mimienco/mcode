export const TASK_PAGE_DETAIL_IMPORTERS = {
  githubItem: () => import('@/components/GitHubItemDialog'),
  githubPullRequest: () => import('@/components/PullRequestPage'),
  githubProject: () => import('@/components/github-project/ProjectViewWrapper'),
  gitlabItem: () => import('@/components/GitLabItemDialog'),
  jiraIssue: () => import('@/components/JiraIssueWorkspace'),
  jiraConnect: () =>
    import('@/components/jira-connect-dialog').then(({ JiraConnectDialog }) => ({
      default: JiraConnectDialog
    })),
  linearIssue: () => import('@/components/LinearIssueWorkspace'),
  linearConnect: () =>
    import('@/components/linear-api-key-dialog').then(({ LinearApiKeyDialog }) => ({
      default: LinearApiKeyDialog
    }))
} as const

export type TaskPageDetailChunk = keyof typeof TASK_PAGE_DETAIL_IMPORTERS

type TaskPageDetailImporterMap = Record<TaskPageDetailChunk, () => Promise<unknown>>

export function createTaskPageDetailPrefetcher(importers: TaskPageDetailImporterMap) {
  const requests = new Map<TaskPageDetailChunk, Promise<unknown>>()

  return (chunk: TaskPageDetailChunk): Promise<unknown> => {
    const existing = requests.get(chunk)
    if (existing) {
      return existing
    }

    const request = importers[chunk]().catch((error: unknown) => {
      // A transient disk/update race must not permanently poison later intent.
      requests.delete(chunk)
      throw error
    })
    requests.set(chunk, request)
    return request
  }
}

export const prefetchTaskPageDetailChunk = createTaskPageDetailPrefetcher(
  TASK_PAGE_DETAIL_IMPORTERS
)
