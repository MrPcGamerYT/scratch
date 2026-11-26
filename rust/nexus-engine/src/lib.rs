//! Independent engine foundation. Does not use or launch QEMU.
//! Android boot and accelerated guest graphics are not implemented yet.
pub mod image;
pub mod ata;
pub mod overlay;
pub mod guest_memory;
pub mod virtqueue;
pub mod virtio_block;
pub mod virtio_mmio;
#[cfg(test)]
mod virtio_mmio_tests;
pub mod kernel;
pub mod uart;
pub mod pit;
pub mod pit0;
pub mod pic;
pub mod rtc;
pub mod platform;
pub mod timing;
pub mod cpu;
pub mod boot;
pub mod io;
pub mod whp;

#[cfg(all(windows,not(target_arch="x86_64")))]
compile_error!("The direct WHP engine currently supports Windows x64 only");

#[cfg(test)]
mod virtio_block_tests;
