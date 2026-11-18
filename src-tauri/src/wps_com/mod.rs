pub mod types;
pub mod process_kill;

#[cfg(windows)]
mod windows_impl;
#[cfg(not(windows))]
mod non_windows;

#[cfg(windows)]
pub use windows_impl::{
  check_engine, check_wps_com, convert_batch_to_pdf, convert_to_pdf, get_wps_version, probe_engines,
};
#[cfg(not(windows))]
pub use non_windows::{
  check_engine, check_wps_com, convert_batch_to_pdf, convert_to_pdf, get_wps_version, probe_engines,
};
