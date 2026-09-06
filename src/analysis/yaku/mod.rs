mod detect;
mod distance;
mod types;

pub use detect::detect_yaku;
pub use distance::yaku_shanten;
pub use types::{Yaku, YakuDetectionError, YakuDistanceError};
