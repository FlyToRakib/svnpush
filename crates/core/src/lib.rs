//! `svnpush-core` is the SVNpush release engine.
//!
//! It holds every piece of release logic and has no UI dependency: the
//! desktop shell is a thin layer over this crate. See `docs/PLAN.md`
//! section 4 for the module map.

pub mod detect;
pub mod edit;
pub mod error;
pub mod package;
pub mod readme;
pub mod report;
pub mod secret;
pub mod svn;
pub mod text;
pub mod tools;
pub mod verify;
pub mod version;

pub use error::Coded;
