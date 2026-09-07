//! Building outgoing transfers from local paths and placing incoming files
//! (ADR-0007). Independent of the network layer.

pub mod incoming;
pub mod outgoing;

pub use incoming::{Destination, Placement, TypeBucket};
pub use outgoing::{
    CollectError, CollectOptions, Outgoing, OutgoingFile, SkipReason, Skipped, collect,
};
