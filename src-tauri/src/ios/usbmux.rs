//! Client for usbmuxd, the Apple service that multiplexes connections to iOS devices over USB.
//! It ships with macOS, and with Apple Devices or iTunes on Windows. This is the same public local
//! interface used by libimobiledevice and by Xcode's tooling.
//!
//! Only device discovery and read-only queries are used here; nothing is written to the device.

use plist::Value;
use std::io::{self, Read, Write};
use std::time::Duration;

const PROTOCOL_VERSION: u32 = 1;
const MESSAGE_PLIST: u32 = 8;
const HEADER_SIZE: usize = 16;
const MAX_MESSAGE_SIZE: u32 = 4 * 1024 * 1024;
const LOCKDOWN_PORT: u16 = 62078;
const IO_TIMEOUT: Duration = Duration::from_secs(3);
const CLIENT_ID: &str = "ScreenLink Studio";

pub trait Transport: Read + Write {}
impl<T: Read + Write> Transport for T {}

fn connect() -> io::Result<Box<dyn Transport>> {
    #[cfg(unix)]
    {
        let s = std::os::unix::net::UnixStream::connect("/var/run/usbmuxd")?;
        s.set_read_timeout(Some(IO_TIMEOUT))?;
        s.set_write_timeout(Some(IO_TIMEOUT))?;
        Ok(Box::new(s))
    }
    #[cfg(windows)]
    {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 27015));
        let s = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300))?;
        s.set_read_timeout(Some(IO_TIMEOUT))?;
        s.set_write_timeout(Some(IO_TIMEOUT))?;
        Ok(Box::new(s))
    }
}

pub fn write_message<W: Write>(w: &mut W, tag: u32, body: &plist::Dictionary) -> io::Result<()> {
    let mut xml = Vec::new();
    plist::to_writer_xml(&mut xml, body).map_err(io::Error::other)?;
    let mut packet = Vec::with_capacity(HEADER_SIZE + xml.len());
    packet.extend(((HEADER_SIZE + xml.len()) as u32).to_le_bytes());
    packet.extend(PROTOCOL_VERSION.to_le_bytes());
    packet.extend(MESSAGE_PLIST.to_le_bytes());
    packet.extend(tag.to_le_bytes());
    packet.extend(xml);
    w.write_all(&packet)
}

pub fn read_message<R: Read>(r: &mut R) -> io::Result<plist::Dictionary> {
    let mut header = [0u8; HEADER_SIZE];
    r.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header[0..4].try_into().unwrap());
    if length < HEADER_SIZE as u32 || length > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad usbmuxd length"));
    }
    let mut body = vec![0u8; length as usize - HEADER_SIZE];
    r.read_exact(&mut body)?;
    Value::from_reader(io::Cursor::new(body))
        .map_err(io::Error::other)?
        .into_dictionary()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "usbmuxd reply is not a dictionary"))
}

fn request(kind: &str) -> plist::Dictionary {
    let mut d = plist::Dictionary::new();
    d.insert("MessageType".into(), kind.into());
    d.insert("ClientVersionString".into(), CLIENT_ID.into());
    d.insert("ProgName".into(), CLIENT_ID.into());
    d.insert("kLibUSBMuxVersion".into(), 3.into());
    d
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxDevice {
    pub device_id: u64,
    pub udid: String,
    pub product_id: u64,
}

pub fn parse_device_list(reply: &plist::Dictionary) -> Vec<MuxDevice> {
    reply
        .get("DeviceList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.as_dictionary()?;
            let props = entry.get("Properties")?.as_dictionary()?;
            // USB-only release: wireless sync entries report ConnectionType "Network".
            if props.get("ConnectionType").and_then(Value::as_string) != Some("USB") {
                return None;
            }
            Some(MuxDevice {
                device_id: entry.get("DeviceID")?.as_unsigned_integer()?,
                udid: props.get("SerialNumber")?.as_string()?.to_string(),
                product_id: props
                    .get("ProductID")
                    .and_then(Value::as_unsigned_integer)
                    .unwrap_or(0),
            })
        })
        .collect()
}

/// `Err` means usbmuxd itself is not reachable (no Apple driver on Windows).
pub fn list_devices() -> io::Result<Vec<MuxDevice>> {
    let mut conn = connect()?;
    write_message(&mut conn, 1, &request("ListDevices"))?;
    Ok(parse_device_list(&read_message(&mut conn)?))
}

/// A stored pair record is how usbmuxd remembers that the user tapped Trust on this device.
pub fn is_paired(udid: &str) -> io::Result<bool> {
    let mut conn = connect()?;
    let mut msg = request("ReadPairRecord");
    msg.insert("PairRecordID".into(), udid.into());
    write_message(&mut conn, 2, &msg)?;
    let reply = read_message(&mut conn)?;
    Ok(reply.contains_key("PairRecordData"))
}

/// Open a tunnel to the device's lockdown service.
pub fn connect_lockdown(device_id: u64) -> io::Result<Box<dyn Transport>> {
    let mut conn = connect()?;
    let mut msg = request("Connect");
    msg.insert("DeviceID".into(), device_id.into());
    // usbmuxd expects the port in network byte order inside this integer field.
    msg.insert("PortNumber".into(), (LOCKDOWN_PORT.to_be() as u64).into());
    write_message(&mut conn, 3, &msg)?;
    let reply = read_message(&mut conn)?;
    match reply.get("Number").and_then(Value::as_unsigned_integer) {
        Some(0) => Ok(conn),
        _ => Err(io::Error::new(io::ErrorKind::ConnectionRefused, "lockdown connect refused")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_entry(id: u64, udid: &str, connection: &str) -> Value {
        let mut props = plist::Dictionary::new();
        props.insert("ConnectionType".into(), connection.into());
        props.insert("SerialNumber".into(), udid.into());
        props.insert("ProductID".into(), 0x12a8u64.into());
        let mut entry = plist::Dictionary::new();
        entry.insert("DeviceID".into(), id.into());
        entry.insert("Properties".into(), Value::Dictionary(props));
        Value::Dictionary(entry)
    }

    #[test]
    fn message_round_trip() {
        let mut wire = Vec::new();
        write_message(&mut wire, 7, &request("ListDevices")).unwrap();
        assert_eq!(u32::from_le_bytes(wire[0..4].try_into().unwrap()) as usize, wire.len());
        assert_eq!(u32::from_le_bytes(wire[4..8].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(wire[8..12].try_into().unwrap()), 8);
        assert_eq!(u32::from_le_bytes(wire[12..16].try_into().unwrap()), 7);
        let parsed = read_message(&mut io::Cursor::new(wire)).unwrap();
        assert_eq!(parsed.get("MessageType").unwrap().as_string(), Some("ListDevices"));
    }

    #[test]
    fn lists_usb_devices_and_skips_network() {
        let mut reply = plist::Dictionary::new();
        reply.insert(
            "DeviceList".into(),
            Value::Array(vec![
                device_entry(3, "00008101-001A", "USB"),
                device_entry(4, "00008030-00FF", "Network"),
            ]),
        );
        assert_eq!(
            parse_device_list(&reply),
            vec![MuxDevice { device_id: 3, udid: "00008101-001A".into(), product_id: 0x12a8 }]
        );
    }

    #[test]
    fn rejects_bad_lengths() {
        let wire = [0u8; 16];
        assert!(read_message(&mut io::Cursor::new(wire)).is_err());
        let mut huge = [0u8; 16];
        huge[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_message(&mut io::Cursor::new(huge)).is_err());
    }

    #[test]
    fn empty_reply_is_no_devices() {
        assert!(parse_device_list(&plist::Dictionary::new()).is_empty());
    }
}
