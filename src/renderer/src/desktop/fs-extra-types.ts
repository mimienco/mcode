// ─── Task-C wire DTOs (mirror src/preload/api/filesystem-api.ts verbatim) ───
// Split from fs-client.ts: the full 34-method FsApi surface outgrew the
// 300-line oxlint cap; these Task-C (window.api.fs parity) DTOs live here.

export type FsImportSkipReason = 'missing' | 'symlink' | 'permission-denied' | 'unsupported'

export type FsImportItemResult =
  | {
      sourcePath: string
      status: 'imported'
      destPath: string
      kind: 'file' | 'directory'
      renamed: boolean
    }
  | {
      sourcePath: string
      status: 'skipped'
      reason: FsImportSkipReason
    }
  | {
      sourcePath: string
      status: 'failed'
      reason: string
    }

export type FsStagedExternalImportEntry =
  | { relativePath: string; kind: 'directory' }
  | { relativePath: string; kind: 'file'; contentBase64: string }

export type FsStagedExternalImportSource =
  | {
      sourcePath: string
      status: 'staged'
      name: string
      kind: 'file' | 'directory'
      entries: FsStagedExternalImportEntry[]
    }
  | {
      sourcePath: string
      status: 'skipped'
      reason: FsImportSkipReason
    }
  | {
      sourcePath: string
      status: 'failed'
      reason: string
    }

export type FsResolveDroppedPathsResult = {
  resolvedPaths: string[]
  skipped: { sourcePath: string; reason: FsImportSkipReason }[]
  failed: { sourcePath: string; reason: string }[]
}

export type FsDownloadFileResult = { canceled: true } | { canceled: false; destinationPath: string }

export type FsStartDownloadedFileResult =
  | { canceled: true }
  | { canceled: false; transferId: string; destinationPath: string }

export type FsLocalLogTailReadArgs = {
  filePath: string
  fromByteOffset: number
  expectedIdentity?: string
}

export type FsLocalLogTailReadResult = {
  contentBase64: string
  nextByteOffset: number
  fileSize: number
  fileIdentity: string
  hasMore: boolean
  reset: boolean
}

export type FsLocalLogTailWatchArgs = {
  filePath: string
  subscriptionId: string
}

export type FsLocalLogTailChangedPayload = {
  subscriptionId: string
  eventType: 'change' | 'rename'
}
