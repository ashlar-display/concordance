#![doc = include_str!("../doc/architecture.md")]
//! HDMI 2.1 mode negotiation — policy layer of the display connection stack.
//!
//! Given sink, source, and cable capabilities (all caller-supplied), produce a ranked
//! list of viable configurations. Answers: "what modes can I drive on this display, in
//! what priority order, using what color format and bit depth?"
//!
//! # Feature flags
//!
//! - **`std`** *(default)* — enables `std`-dependent types; implies `alloc`.
//! - **`alloc`** — enables the ranked iterator, `ReasoningTrace`, and `DefaultEnumerator`.
//! - **`serde`** — derives `Serialize`/`Deserialize` for all public types.
//!
//! Without `alloc`, [`is_config_viable`], [`enumerator::CandidateEnumerator`], and
//! [`SliceEnumerator`][enumerator::SliceEnumerator] are available.
#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod diagnostic;
pub mod engine;
pub mod error;
pub mod frl;
pub mod output;
pub mod probe;
pub mod types;

#[cfg(any(feature = "alloc", feature = "std"))]
pub mod builder;
pub mod enumerator;
#[cfg(any(feature = "alloc", feature = "std"))]
pub mod ranker;

pub use diagnostic::Diagnostic;
pub use engine::rule::TaggingAdapter;
pub use engine::{CheckList, MAX_WARNINGS};
pub use error::Error;
pub use output::warning::{LimitSource, TaggedViolation, Violation, Warning};
pub use probe::is_config_viable;
pub use types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

#[cfg(any(feature = "alloc", feature = "std"))]
pub use builder::{NegotiationLog, NegotiatorBuilder};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use output::config::NegotiatedConfig;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use output::rejection::RejectedConfig;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use output::trace::ReasoningTrace;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use types::{SinkBuildWarning, SupportedModes, sink_capabilities_from_display};
