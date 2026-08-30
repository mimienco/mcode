mod external;
mod scoped;

pub use external::ExactPathCapability;
pub use scoped::{
    BoundedFileRead, BoundedFileSnapshot, DirectoryEntry, FsError, FsOperation, PathKind,
    PathMetadata, ScopeId, ScopedFilesystem,
};
