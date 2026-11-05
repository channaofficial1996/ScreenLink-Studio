//! Apple device backend, kept entirely separate from the Android backend.
//!
//! What Apple actually permits decides what this module can do:
//!
//! - macOS: AVFoundation exposes a USB-connected iPhone or iPad as a capture device once
//!   CoreMediaIO's screen-capture flag is enabled. It is the public API QuickTime Player uses.
//!   Mirroring is view-only: iOS has no API for injecting touch or keyboard input from a computer.
//! - Windows: Apple provides no screen capture API at all. Detection works, but mirroring is
//!   reported as unsupported rather than faked. Third-party Windows tools rely on reverse-engineered
//!   USB protocols or a replacement libusb driver, which breaks Apple's own software.

#[cfg(target_os = "macos")]
pub mod capture_macos;
pub mod lockdown;
pub mod usbmux;

use crate::devices::{Capabilities, Compatibility, Device, DeviceState, Platform};
use crate::usb::UsbPhone;
use std::collections::HashMap;
use std::sync::Mutex;

/// Does this OS have a supported way to capture an iPhone screen?
pub const CAN_MIRROR: bool = cfg!(target_os = "macos");

#[derive(Debug, Clone, Default)]
struct Cached {
    name: Option<String>,
    product_type: Option<String>,
    ios_version: Option<String>,
}

#[derive(Default)]
pub struct IosMonitor {
    cache: Mutex<HashMap<String, Cached>>,
}

impl IosMonitor {
    /// Combine usbmuxd (trust, identity) with raw USB (presence). Returns devices plus whether
    /// usbmuxd is reachable, which on Windows means an Apple driver package is installed.
    pub fn scan(&self, usb: &[&UsbPhone]) -> (Vec<Device>, bool) {
        match usbmux::list_devices() {
            Ok(list) => {
                let devices: Vec<Device> = list
                    .into_iter()
                    .map(|d| {
                        let trusted = usbmux::is_paired(&d.udid).unwrap_or(false);
                        let cached = self.identity(&d);
                        device(&d.udid, &cached, trusted)
                    })
                    .collect();
                (with_unlisted(devices, usb, false), true)
            }
            Err(_) => (with_unlisted(Vec::new(), usb, true), false),
        }
    }

    fn identity(&self, d: &usbmux::MuxDevice) -> Cached {
        if let Some(c) = self.cache.lock().unwrap().get(&d.udid).filter(|c| c.name.is_some()) {
            return c.clone();
        }
        let cached = usbmux::connect_lockdown(d.device_id)
            .map(|mut conn| {
                let id = lockdown::identity(&mut conn);
                Cached { name: id.name, product_type: id.product_type, ios_version: id.ios_version }
            })
            .unwrap_or_default();
        self.cache.lock().unwrap().insert(d.udid.clone(), cached.clone());
        cached
    }
}

/// USB-visible Apple devices that usbmuxd did not report.
fn with_unlisted(mut devices: Vec<Device>, usb: &[&UsbPhone], no_driver: bool) -> Vec<Device> {
    for phone in usb {
        let id = phone
            .serial
            .clone()
            .unwrap_or_else(|| format!("apple-{:04x}", phone.product_id));
        if devices.iter().any(|d| same_udid(&d.id, &id)) {
            continue;
        }
        devices.push(no_driver_device(&id, phone, no_driver));
    }
    devices
}

/// USB descriptors report the UDID without the dash that usbmuxd includes on newer devices.
fn same_udid(a: &str, b: &str) -> bool {
    a.replace('-', "").eq_ignore_ascii_case(&b.replace('-', ""))
}

fn device(udid: &str, cached: &Cached, trusted: bool) -> Device {
    let (state, compatibility) = if !trusted {
        (
            DeviceState::Unauthorized,
            Compatibility::warning(
                "ios_not_trusted",
                "Unlock the device and tap Trust. If no prompt appears, open Finder (macOS) or the \
                 Apple Devices app (Windows) with the device connected.",
            ),
        )
    } else if CAN_MIRROR {
        (
            DeviceState::Connected,
            Compatibility::info(
                "ios_view_only",
                "View-only mirroring. iOS does not allow a computer to control the device.",
            ),
        )
    } else {
        (DeviceState::Unsupported, windows_limit())
    };

    let can_view = trusted && CAN_MIRROR;
    Device {
        id: udid.to_string(),
        platform: Platform::Ios,
        name: cached.name.clone().unwrap_or_else(|| "Apple device".into()),
        model: cached.product_type.clone(),
        manufacturer: Some("Apple".into()),
        os_version: cached.ios_version.as_ref().map(|v| format!("iOS {v}")),
        state,
        capabilities: Capabilities {
            mirror: can_view,
            control: false,
            audio: false,
            record: false,
            screenshot: can_view,
        },
        compatibility: Some(compatibility),
    }
}

fn no_driver_device(id: &str, phone: &UsbPhone, no_driver: bool) -> Device {
    let compatibility = if no_driver && cfg!(windows) {
        Compatibility::warning(
            "ios_no_driver",
            "Install the Apple Devices app from the Microsoft Store so this device can be \
             identified. Screen mirroring is still unavailable on Windows.",
        )
    } else {
        Compatibility::warning(
            "ios_not_ready",
            "Unlock the device and tap Trust if prompted. It should appear here within a few seconds.",
        )
    };
    Device {
        id: id.to_string(),
        platform: Platform::Ios,
        name: phone.product.clone().unwrap_or_else(|| "Apple device".into()),
        model: None,
        manufacturer: Some("Apple".into()),
        os_version: None,
        state: if CAN_MIRROR { DeviceState::Offline } else { DeviceState::Unsupported },
        capabilities: Capabilities::default(),
        compatibility: Some(compatibility),
    }
}

fn windows_limit() -> Compatibility {
    Compatibility::warning(
        "ios_windows_unsupported",
        "Apple does not provide a screen capture API on Windows, so iPhone and iPad mirroring is \
         only available in the macOS version of ScreenLink Studio.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached() -> Cached {
        Cached {
            name: Some("Test iPhone".into()),
            product_type: Some("iPhone15,2".into()),
            ios_version: Some("18.1".into()),
        }
    }

    #[test]
    fn untrusted_device_cannot_mirror_and_asks_for_trust() {
        let d = device("UDID", &cached(), false);
        assert_eq!(d.state, DeviceState::Unauthorized);
        assert!(!d.capabilities.mirror);
        assert_eq!(d.compatibility.unwrap().code, "ios_not_trusted");
    }

    #[test]
    fn never_claims_control_or_audio() {
        let d = device("UDID", &cached(), true);
        assert!(!d.capabilities.control);
        assert!(!d.capabilities.audio);
    }

    #[test]
    fn trusted_device_matches_platform_support() {
        let d = device("UDID", &cached(), true);
        if CAN_MIRROR {
            assert_eq!(d.state, DeviceState::Connected);
            assert!(d.capabilities.mirror);
        } else {
            assert_eq!(d.state, DeviceState::Unsupported);
            assert!(!d.capabilities.mirror);
            assert_eq!(d.compatibility.unwrap().code, "ios_windows_unsupported");
        }
    }

    #[test]
    fn matches_udid_with_and_without_dash() {
        assert!(same_udid("00008101-001A2B3C", "00008101001a2b3c"));
        assert!(!same_udid("00008101-001A2B3C", "00008101001a2b3d"));
    }

    #[test]
    fn usb_only_device_is_reported_without_driver() {
        let phone = UsbPhone {
            kind: crate::usb::UsbKind::Apple,
            vendor_id: 0x05ac,
            product_id: 0x12a8,
            serial: Some("ABC".into()),
            product: Some("iPhone".into()),
            manufacturer: Some("Apple Inc.".into()),
        };
        let devices = with_unlisted(Vec::new(), &[&phone], true);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "iPhone");
        assert!(!devices[0].capabilities.mirror);
    }
}
