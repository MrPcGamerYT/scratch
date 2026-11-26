//! Independent engine foundation. Does not use or launch QEMU.
//! Android boot and accelerated guest graphics are not implemented yet.
pub mod image;
pub mod boot;
pub mod io;
pub mod whp;

#[cfg(all(windows,not(target_arch="x86_64")))]
compile_error!("The direct WHP engine currently supports Windows x64 only");
