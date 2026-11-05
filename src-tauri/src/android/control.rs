//! Typed input events from the UI and their scrcpy v5 control-socket encoding.
//! Layouts follow app/src/control_msg.c. Unknown or out-of-range values are rejected rather than
//! forwarded, so the UI can only ever trigger this fixed set of device actions.

use serde::Deserialize;

const TYPE_INJECT_KEYCODE: u8 = 0;
const TYPE_INJECT_TEXT: u8 = 1;
const TYPE_INJECT_TOUCH: u8 = 2;
const TYPE_INJECT_SCROLL: u8 = 3;
const TYPE_BACK_OR_SCREEN_ON: u8 = 4;
const TYPE_EXPAND_NOTIFICATIONS: u8 = 5;
const TYPE_EXPAND_SETTINGS: u8 = 6;
const TYPE_COLLAPSE_PANELS: u8 = 7;
const TYPE_SET_CLIPBOARD: u8 = 9;
const TYPE_SET_DISPLAY_POWER: u8 = 10;
const TYPE_ROTATE_DEVICE: u8 = 11;
const TYPE_RESET_VIDEO: u8 = 17;

const POINTER_ID_MOUSE: u64 = u64::MAX;
const TEXT_MAX_BYTES: usize = 300;
const CLIPBOARD_MAX_BYTES: usize = (1 << 18) - 14;
const MAX_KEYCODE: u32 = 400;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum InputEvent {
    Touch { action: u8, x: i32, y: i32, w: u16, h: u16, pressure: f32, button: u32, buttons: u32 },
    Scroll { x: i32, y: i32, w: u16, h: u16, hscroll: f32, vscroll: f32, buttons: u32 },
    Key { action: u8, code: u32, repeat: u32, meta: u32 },
    Text { text: String },
    Paste { text: String, sequence: u64 },
    BackOrScreenOn { action: u8 },
    ExpandNotifications,
    ExpandSettings,
    CollapsePanels,
    RotateDevice,
    DisplayPower { on: bool },
    ResetVideo,
}

impl InputEvent {
    /// Serialize for the control socket, or `None` if a field is out of range.
    pub fn encode(&self) -> Option<Vec<u8>> {
        match self {
            InputEvent::Touch { action, x, y, w, h, pressure, button, buttons } => {
                // DOWN, UP, MOVE, HOVER_MOVE only.
                if !matches!(action, 0 | 1 | 2 | 7) || !pressure.is_finite() {
                    return None;
                }
                let mut b = Vec::with_capacity(32);
                b.push(TYPE_INJECT_TOUCH);
                b.push(*action);
                b.extend(POINTER_ID_MOUSE.to_be_bytes());
                position(&mut b, *x, *y, *w, *h);
                b.extend(u16fp(*pressure).to_be_bytes());
                b.extend(button.to_be_bytes());
                b.extend(buttons.to_be_bytes());
                Some(b)
            }
            InputEvent::Scroll { x, y, w, h, hscroll, vscroll, buttons } => {
                if !hscroll.is_finite() || !vscroll.is_finite() {
                    return None;
                }
                let mut b = Vec::with_capacity(21);
                b.push(TYPE_INJECT_SCROLL);
                position(&mut b, *x, *y, *w, *h);
                b.extend(i16fp((*hscroll / 16.0).clamp(-1.0, 1.0)).to_be_bytes());
                b.extend(i16fp((*vscroll / 16.0).clamp(-1.0, 1.0)).to_be_bytes());
                b.extend(buttons.to_be_bytes());
                Some(b)
            }
            InputEvent::Key { action, code, repeat, meta } => {
                if *action > 1 || *code > MAX_KEYCODE {
                    return None;
                }
                let mut b = Vec::with_capacity(14);
                b.push(TYPE_INJECT_KEYCODE);
                b.push(*action);
                b.extend(code.to_be_bytes());
                b.extend(repeat.to_be_bytes());
                b.extend(meta.to_be_bytes());
                Some(b)
            }
            InputEvent::Text { text } => {
                let text = truncate_utf8(text, TEXT_MAX_BYTES);
                let mut b = Vec::with_capacity(5 + text.len());
                b.push(TYPE_INJECT_TEXT);
                b.extend((text.len() as u32).to_be_bytes());
                b.extend(text.as_bytes());
                Some(b)
            }
            InputEvent::Paste { text, sequence } => {
                let text = truncate_utf8(text, CLIPBOARD_MAX_BYTES);
                let mut b = Vec::with_capacity(14 + text.len());
                b.push(TYPE_SET_CLIPBOARD);
                b.extend(sequence.to_be_bytes());
                b.push(1); // paste immediately
                b.extend((text.len() as u32).to_be_bytes());
                b.extend(text.as_bytes());
                Some(b)
            }
            InputEvent::BackOrScreenOn { action } => {
                (*action <= 1).then(|| vec![TYPE_BACK_OR_SCREEN_ON, *action])
            }
            InputEvent::ExpandNotifications => Some(vec![TYPE_EXPAND_NOTIFICATIONS]),
            InputEvent::ExpandSettings => Some(vec![TYPE_EXPAND_SETTINGS]),
            InputEvent::CollapsePanels => Some(vec![TYPE_COLLAPSE_PANELS]),
            InputEvent::RotateDevice => Some(vec![TYPE_ROTATE_DEVICE]),
            InputEvent::DisplayPower { on } => Some(vec![TYPE_SET_DISPLAY_POWER, *on as u8]),
            InputEvent::ResetVideo => Some(vec![TYPE_RESET_VIDEO]),
        }
    }
}

fn position(b: &mut Vec<u8>, x: i32, y: i32, w: u16, h: u16) {
    b.extend(x.to_be_bytes());
    b.extend(y.to_be_bytes());
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
}

/// Same fixed-point conversion as sc_float_to_u16fp: 1.0 saturates to 0xffff.
fn u16fp(f: f32) -> u16 {
    let u = (f.clamp(0.0, 1.0) * 65536.0) as u32;
    u.min(0xffff) as u16
}

/// Same fixed-point conversion as sc_float_to_i16fp.
fn i16fp(f: f32) -> i16 {
    let i = (f.clamp(-1.0, 1.0) * 32768.0) as i32;
    i.clamp(-0x8000, 0x7fff) as i16
}

fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_matches_scrcpy() {
        let e = InputEvent::Key { action: 1, code: 66, repeat: 5, meta: 0x41 };
        assert_eq!(
            e.encode().unwrap(),
            [0x00, 0x01, 0, 0, 0, 0x42, 0, 0, 0, 5, 0, 0, 0, 0x41]
        );
    }

    #[test]
    fn text_matches_scrcpy() {
        let e = InputEvent::Text { text: "hello, world!".into() };
        let mut want = vec![0x01, 0, 0, 0, 13];
        want.extend(b"hello, world!");
        assert_eq!(e.encode().unwrap(), want);
    }

    #[test]
    fn touch_matches_scrcpy_layout() {
        let e = InputEvent::Touch {
            action: 0, x: 100, y: 200, w: 1080, h: 1920, pressure: 1.0, button: 1, buttons: 1,
        };
        let b = e.encode().unwrap();
        assert_eq!(b.len(), 32);
        assert_eq!(&b[0..2], &[0x02, 0x00]);
        assert_eq!(&b[2..10], &[0xff; 8]);
        assert_eq!(&b[10..14], &[0, 0, 0, 100]);
        assert_eq!(&b[14..18], &[0, 0, 0, 200]);
        assert_eq!(&b[18..22], &[0x04, 0x38, 0x07, 0x80]);
        assert_eq!(&b[22..24], &[0xff, 0xff]); // pressure 1.0
        assert_eq!(&b[24..28], &[0, 0, 0, 1]);
        assert_eq!(&b[28..32], &[0, 0, 0, 1]);
    }

    #[test]
    fn scroll_saturates_like_scrcpy() {
        let e = InputEvent::Scroll {
            x: 260, y: 1026, w: 1080, h: 1920, hscroll: 16.0, vscroll: -16.0, buttons: 1,
        };
        assert_eq!(
            e.encode().unwrap(),
            [
                0x03, 0, 0, 0x01, 0x04, 0, 0, 0x04, 0x02, 0x04, 0x38, 0x07, 0x80, 0x7F, 0xFF,
                0x80, 0x00, 0, 0, 0, 1
            ]
        );
    }

    #[test]
    fn paste_sets_flag_and_sequence() {
        let e = InputEvent::Paste { text: "hi".into(), sequence: 7 };
        assert_eq!(
            e.encode().unwrap(),
            [9, 0, 0, 0, 0, 0, 0, 0, 7, 1, 0, 0, 0, 2, b'h', b'i']
        );
    }

    #[test]
    fn single_byte_commands() {
        assert_eq!(InputEvent::RotateDevice.encode().unwrap(), [11]);
        assert_eq!(InputEvent::ResetVideo.encode().unwrap(), [17]);
        assert_eq!(InputEvent::DisplayPower { on: true }.encode().unwrap(), [10, 1]);
        assert_eq!(InputEvent::BackOrScreenOn { action: 1 }.encode().unwrap(), [4, 1]);
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(InputEvent::Key { action: 9, code: 1, repeat: 0, meta: 0 }.encode().is_none());
        assert!(InputEvent::Key { action: 0, code: 9999, repeat: 0, meta: 0 }.encode().is_none());
        assert!(InputEvent::Touch {
            action: 99, x: 0, y: 0, w: 1, h: 1, pressure: 1.0, button: 0, buttons: 0
        }
        .encode()
        .is_none());
        assert!(InputEvent::Touch {
            action: 0, x: 0, y: 0, w: 1, h: 1, pressure: f32::NAN, button: 0, buttons: 0
        }
        .encode()
        .is_none());
    }

    #[test]
    fn text_truncates_on_char_boundary() {
        let e = InputEvent::Text { text: "é".repeat(200) };
        let b = e.encode().unwrap();
        let len = u32::from_be_bytes(b[1..5].try_into().unwrap()) as usize;
        assert_eq!(len, 300);
        assert!(std::str::from_utf8(&b[5..]).is_ok());
    }

    #[test]
    fn deserializes_from_ui_json() {
        let e: InputEvent = serde_json::from_str(
            r#"{"t":"touch","action":2,"x":5,"y":6,"w":720,"h":1600,"pressure":1,"button":0,"buttons":1}"#,
        )
        .unwrap();
        assert!(matches!(e, InputEvent::Touch { x: 5, y: 6, .. }));
        let e: InputEvent = serde_json::from_str(r#"{"t":"backOrScreenOn","action":0}"#).unwrap();
        assert_eq!(e, InputEvent::BackOrScreenOn { action: 0 });
        assert!(serde_json::from_str::<InputEvent>(r#"{"t":"shell","cmd":"reboot"}"#).is_err());
    }
}
