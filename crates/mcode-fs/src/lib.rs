mod scoped;

pub use scoped::{
    BoundedFileRead, BoundedFileSnapshot, DirectoryEntry, FsError, FsOperation, PathKind,
    PathMetadata, ScopeId, ScopedFilesystem,
};
