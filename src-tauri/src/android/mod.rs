//! Android backend: adb device management plus the scrcpy server protocol.

pub mod adb;
pub mod control;
#[cfg(test)]
mod hardware_tests;
pub mod server;
pub mod session;
pub mod stream;
