#[cfg(target_vendor = "apple")]
mod apple;
#[cfg(target_vendor = "apple")]
pub(super) use apple::*;

#[cfg(not(target_vendor = "apple"))]
mod unsupported;
#[cfg(not(target_vendor = "apple"))]
pub(super) use unsupported::*;
