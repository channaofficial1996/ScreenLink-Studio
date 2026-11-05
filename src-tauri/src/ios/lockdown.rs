//! Read-only lockdownd queries. Before a session is established, lockdownd answers a small set of
//! identity keys (device name, model, iOS version) which needs no pairing.

use super::usbmux::Transport;
use plist::Value;
use std::io::{self, Read, Write};

const MAX_RESPONSE_SIZE: u32 = 1024 * 1024;
const LABEL: &str = "ScreenLinkStudio";

fn send<W: Write>(w: &mut W, body: &plist::Dictionary) -> io::Result<()> {
    let mut xml = Vec::new();
    plist::to_writer_xml(&mut xml, body).map_err(io::Error::other)?;
    w.write_all(&(xml.len() as u32).to_be_bytes())?;
    w.write_all(&xml)
}

fn receive<R: Read>(r: &mut R) -> io::Result<plist::Dictionary> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len);
    if len == 0 || len > MAX_RESPONSE_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad lockdown length"));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    Value::from_reader(io::Cursor::new(body))
        .map_err(io::Error::other)?
        .into_dictionary()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "lockdown reply is not a dictionary"))
}

fn get_value<T: Read + Write>(conn: &mut T, key: &str) -> io::Result<Option<String>> {
    let mut msg = plist::Dictionary::new();
    msg.insert("Label".into(), LABEL.into());
    msg.insert("Request".into(), "GetValue".into());
    msg.insert("Key".into(), key.into());
    send(conn, &msg)?;
    let reply = receive(conn)?;
    Ok(reply.get("Value").and_then(Value::as_string).map(str::to_string))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub name: Option<String>,
    pub product_type: Option<String>,
    pub ios_version: Option<String>,
}

pub fn identity(conn: &mut Box<dyn Transport>) -> DeviceIdentity {
    let mut get = |key| get_value(conn, key).ok().flatten();
    DeviceIdentity {
        name: get("DeviceName"),
        product_type: get("ProductType"),
        ios_version: get("ProductVersion"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Scripted lockdownd: replies to each request with the next queued value.
    struct Fake {
        replies: VecDeque<Vec<u8>>,
        current: io::Cursor<Vec<u8>>,
        requests: Vec<u8>,
    }

    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.current.read(buf)?;
            if n > 0 {
                return Ok(n);
            }
            match self.replies.pop_front() {
                Some(next) => {
                    self.current = io::Cursor::new(next);
                    self.current.read(buf)
                }
                None => Ok(0),
            }
        }
    }

    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.requests.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn reply(value: Option<&str>) -> Vec<u8> {
        let mut d = plist::Dictionary::new();
        d.insert("Request".into(), "GetValue".into());
        match value {
            Some(v) => {
                d.insert("Value".into(), v.into());
            }
            None => {
                d.insert("Error".into(), "MissingValue".into());
            }
        }
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &d).unwrap();
        let mut framed = (xml.len() as u32).to_be_bytes().to_vec();
        framed.extend(xml);
        framed
    }

    fn fake(values: &[Option<&str>]) -> Fake {
        Fake {
            replies: values.iter().map(|v| reply(*v)).collect(),
            current: io::Cursor::new(Vec::new()),
            requests: Vec::new(),
        }
    }

    #[test]
    fn reads_identity_keys() {
        let mut f = fake(&[Some("Alex's iPhone"), Some("iPhone15,2"), Some("18.1")]);
        assert_eq!(get_value(&mut f, "DeviceName").unwrap().as_deref(), Some("Alex's iPhone"));
        assert_eq!(get_value(&mut f, "ProductType").unwrap().as_deref(), Some("iPhone15,2"));
        assert_eq!(get_value(&mut f, "ProductVersion").unwrap().as_deref(), Some("18.1"));
        assert!(String::from_utf8_lossy(&f.requests).contains("GetValue"));
    }

    #[test]
    fn missing_value_is_none_not_an_error() {
        let mut f = fake(&[None]);
        assert_eq!(get_value(&mut f, "DeviceName").unwrap(), None);
    }

    #[test]
    fn identity_tolerates_a_closed_connection() {
        let mut conn: Box<dyn Transport> = Box::new(fake(&[Some("Phone")]));
        let id = identity(&mut conn);
        assert_eq!(id.name.as_deref(), Some("Phone"));
        assert_eq!(id.ios_version, None);
    }
}
