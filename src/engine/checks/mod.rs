mod color;
mod dsc;
mod frl;
mod timing;

pub use color::{BitDepthCheck, ColorEncodingCheck};
pub use dsc::DscCheck;
pub use frl::{FrlCapacityCheck, FrlCeilingCheck};
pub use timing::{PixelClockCheck, RefreshRateCheck, TmdsClockCheck};

use crate::engine::rule::CheckList;
use crate::output::warning::Violation;

/// The ordered list of constraint rules applied by [`DefaultConstraintEngine`][super::DefaultConstraintEngine] by default.
///
/// Rules are evaluated in declaration order. In alloc mode all violations are
/// collected; in no-alloc mode the engine short-circuits on the first failure.
pub static DEFAULT_CHECKS: CheckList<Violation> = &[
    &FrlCeilingCheck,
    &RefreshRateCheck,
    &PixelClockCheck,
    &TmdsClockCheck,
    &ColorEncodingCheck,
    &BitDepthCheck,
    &DscCheck,
    // Last: the costliest check, and a candidate an earlier rule rejects for a more basic
    // reason (encoding, depth, DSC) is reported for that reason.
    &FrlCapacityCheck,
];
