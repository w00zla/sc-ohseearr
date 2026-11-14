//! The platform independent parts of a live capture, shared by the capture crates of the platforms
//! (`sc-ohseearr-capture-win`, `sc-ohseearr-capture-linux`) and the apps on top of them.
//!
//! The captured image ([`Pixels`]: the capture's BGRA rows, converted only when read), the change gate
//! ([`Gate`]) on its grey thumbnail ([`Pixels::thumb`]), and the cursor-rest detector ([`Rest`]) with the
//! timing a pulling reader uses. No capture here, and no platform code.

mod gate;
mod pixels;
mod rest;

pub use gate::Gate;
pub use pixels::{Pixels, unpad};
pub use rest::{PULL_POLL, PULL_TIMEOUT_MS, REST, REST_PX, Rest};
