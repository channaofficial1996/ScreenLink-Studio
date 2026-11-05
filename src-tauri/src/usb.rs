//! Raw USB enumeration. ADB and usbmuxd only report devices they can already talk to, so this is
//! what lets the app recognize a phone with USB debugging off, or an iPhone with no Apple driver
//! installed, and say exactly what the user needs to do.

use nusb::MaybeFuture;

const APPLE_VENDOR: u16 = 0x05ac;
/// iPhone, iPad and iPod touch product IDs fall in this range; Macs, keyboards and so on do not.
const APPLE_MOBILE_PRODUCTS: std::ops::RangeInclusive<u16> = 0x1290..=0x12af;

/// Vendors that ship Android phones. The list only narrows candidates: `is_phone_like` still has
/// to match, because several of these vendors also make USB drives and peripherals.
const ANDROID_VENDORS: [u16; 19] = [
    0x18d1, // Google
    0x22b8, // Motorola
    0x04e8, // Samsung
    0x2717, // Xiaomi
    0x12d1, // Huawei
    0x2a70, // OnePlus
    0x22d9, // OPPO
    0x2d95, // vivo
    0x0bb4, // HTC
    0x1004, // LG
    0x0fce, // Sony
    0x17ef, // Lenovo
    0x19d2, // ZTE
    0x0b05, // ASUS
    0x1949, // Amazon
    0x2e04, // HMD/Nokia
    0x2ae5, // Fairphone
    0x2b4c, // Zuk
    0x05c6, // Qualcomm reference
];

const CLASS_IMAGE: u8 = 0x06;
const CLASS_VENDOR: u8 = 0xff;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsbKind {
    Android,
    Apple,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbPhone {
    pub kind: UsbKind,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: Option<String>,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
}

pub fn classify(vendor: u16, product: u16, interface_classes: &[u8]) -> Option<UsbKind> {
    if vendor == APPLE_VENDOR {
        return APPLE_MOBILE_PRODUCTS.contains(&product).then_some(UsbKind::Apple);
    }
    let phone_like = interface_classes.iter().any(|c| matches!(*c, CLASS_IMAGE | CLASS_VENDOR));
    (ANDROID_VENDORS.contains(&vendor) && phone_like).then_some(UsbKind::Android)
}

pub fn scan() -> Vec<UsbPhone> {
    let devices = match nusb::list_devices().wait() {
        Ok(devices) => devices,
        Err(e) => {
            log::debug!("USB enumeration failed: {e}");
            return Vec::new();
        }
    };
    devices
        .filter_map(|d| {
            let classes: Vec<u8> = d.interfaces().map(|i| i.class()).collect();
            let kind = classify(d.vendor_id(), d.product_id(), &classes)?;
            Some(UsbPhone {
                kind,
                vendor_id: d.vendor_id(),
                product_id: d.product_id(),
                serial: d.serial_number().map(str::to_string),
                product: d.product_string().map(str::to_string),
                manufacturer: d.manufacturer_string().map(str::to_string),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_apple_mobile_devices_only() {
        assert_eq!(classify(0x05ac, 0x12a8, &[]), Some(UsbKind::Apple)); // iPhone
        assert_eq!(classify(0x05ac, 0x12ab, &[]), Some(UsbKind::Apple)); // iPad
        assert_eq!(classify(0x05ac, 0x024f, &[3]), None); // Apple keyboard
    }

    #[test]
    fn android_requires_phone_like_interfaces() {
        assert_eq!(classify(0x22b8, 0x2e81, &[6, 0xff]), Some(UsbKind::Android));
        assert_eq!(classify(0x04e8, 0x61fb, &[8]), None); // Samsung portable SSD
        assert_eq!(classify(0x046d, 0xc24f, &[0xff, 3]), None); // Logitech wheel
    }
}
