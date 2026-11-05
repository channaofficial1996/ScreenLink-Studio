//! "Phone only" window mode for screen recording and teaching: the window becomes a borderless,
//! always-on-top panel exactly the shape of the phone, docked to one side of the desktop. The
//! previous window geometry is saved and restored on exit.

use crate::commands::AppState;
use crate::error::{AppError, AppResult};
use serde::Deserialize;
use std::sync::Mutex;
use tauri::{LogicalSize, PhysicalPosition, PhysicalSize, Size, State, WebviewWindow};

const MIN_PHONE_WINDOW: LogicalSize<f64> = LogicalSize::new(180.0, 180.0);
const MIN_NORMAL_WINDOW: LogicalSize<f64> = LogicalSize::new(1040.0, 680.0);
/// Fraction of the work area the panel may take, leaving desktop space for teaching content.
const MAX_HEIGHT_RATIO: f64 = 0.94;
const MAX_WIDTH_RATIO: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Left,
    Right,
}

struct Saved {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    maximized: bool,
}

struct Active {
    saved: Saved,
    side: Side,
}

#[derive(Default)]
pub struct PhoneMode {
    active: Mutex<Option<Active>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Largest phone-shaped rectangle that fits the work area, docked to `side`, centered vertically.
pub fn place(
    work: (i32, i32, u32, u32),
    video: (u32, u32),
    side: Side,
    margin: u32,
) -> Placement {
    let (left, top, work_w, work_h) = work;
    let (video_w, video_h) = (video.0.max(1) as f64, video.1.max(1) as f64);
    let scale = (work_h as f64 * MAX_HEIGHT_RATIO / video_h).min(work_w as f64 * MAX_WIDTH_RATIO / video_w);
    let width = (video_w * scale).round() as u32;
    let height = (video_h * scale).round() as u32;
    let x = match side {
        Side::Left => left + margin as i32,
        Side::Right => left + work_w as i32 - width as i32 - margin as i32,
    };
    let y = top + (work_h as i32 - height as i32) / 2;
    Placement { x, y, width, height }
}

fn apply(window: &WebviewWindow, width: u32, height: u32, side: Side) -> tauri::Result<()> {
    let monitor = window.current_monitor()?.or(window.primary_monitor()?);
    let Some(monitor) = monitor else { return Ok(()) };
    let area = monitor.work_area();
    let margin = (12.0 * monitor.scale_factor()).round() as u32;
    let p = place(
        (area.position.x, area.position.y, area.size.width, area.size.height),
        (width, height),
        side,
        margin,
    );
    window.set_size(PhysicalSize::new(p.width, p.height))?;
    window.set_position(PhysicalPosition::new(p.x, p.y))?;
    Ok(())
}

fn failed(e: tauri::Error) -> AppError {
    AppError::Unsupported(format!("Could not change the window: {e}"))
}

#[tauri::command]
pub fn phone_mode_enter(
    window: WebviewWindow,
    state: State<'_, PhoneMode>,
    width: u32,
    height: u32,
    side: Side,
) -> AppResult<()> {
    let mut active = state.active.lock().unwrap();
    if active.is_none() {
        let maximized = window.is_maximized().unwrap_or(false);
        if maximized {
            window.unmaximize().map_err(failed)?;
        }
        *active = Some(Active {
            saved: Saved {
                position: window.outer_position().map_err(failed)?,
                size: window.outer_size().map_err(failed)?,
                maximized,
            },
            side,
        });
    }
    if let Some(a) = active.as_mut() {
        a.side = side;
    }
    window.set_decorations(false).map_err(failed)?;
    window.set_min_size(Some(Size::Logical(MIN_PHONE_WINDOW))).map_err(failed)?;
    window.set_always_on_top(true).map_err(failed)?;
    apply(&window, width, height, side).map_err(failed)
}

/// Re-fit after the phone rotates or the panel moves to the other side.
#[tauri::command]
pub fn phone_mode_fit(
    window: WebviewWindow,
    state: State<'_, PhoneMode>,
    width: u32,
    height: u32,
    side: Option<Side>,
) -> AppResult<()> {
    let mut active = state.active.lock().unwrap();
    let Some(current) = active.as_mut() else { return Ok(()) };
    if let Some(side) = side {
        current.side = side;
    }
    apply(&window, width, height, current.side).map_err(failed)
}

#[tauri::command]
pub fn phone_mode_pin(window: WebviewWindow, pinned: bool) -> AppResult<()> {
    window.set_always_on_top(pinned).map_err(failed)
}

#[tauri::command]
pub fn phone_mode_exit(
    window: WebviewWindow,
    state: State<'_, PhoneMode>,
    app_state: State<'_, AppState>,
) -> AppResult<()> {
    let Some(active) = state.active.lock().unwrap().take() else { return Ok(()) };
    let always_on_top = app_state.settings.lock().unwrap().always_on_top;
    window.set_always_on_top(always_on_top).map_err(failed)?;
    window.set_decorations(true).map_err(failed)?;
    window.set_size(active.saved.size).map_err(failed)?;
    window.set_position(active.saved.position).map_err(failed)?;
    window.set_min_size(Some(Size::Logical(MIN_NORMAL_WINDOW))).map_err(failed)?;
    if active.saved.maximized {
        window.maximize().map_err(failed)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: (i32, i32, u32, u32) = (0, 0, 2560, 1400);

    #[test]
    fn portrait_keeps_the_phone_aspect_and_docks_right() {
        let p = place(WORK, (864, 1920), Side::Right, 12);
        assert_eq!(p.height, 1316);
        assert_eq!(p.width, 592);
        assert_eq!(p.x, 2560 - 592 - 12);
        assert_eq!(p.y, (1400 - 1316) / 2);
    }

    #[test]
    fn docks_left_including_on_a_secondary_monitor() {
        let p = place((-1920, 100, 1920, 1000), (864, 1920), Side::Left, 12);
        assert_eq!(p.x, -1908);
        assert!(p.y >= 100);
    }

    #[test]
    fn landscape_is_limited_by_width() {
        let p = place((0, 0, 1920, 1040), (1920, 864), Side::Right, 12);
        assert_eq!(p.width, 1152);
        assert_eq!(p.height, 518);
    }

    #[test]
    fn zero_sized_video_does_not_panic() {
        let p = place(WORK, (0, 0), Side::Right, 12);
        assert!(p.width > 0);
    }
}
