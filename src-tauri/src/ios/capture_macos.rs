//! iPhone and iPad screen capture on macOS, through public Apple APIs only.
//!
//! Setting CoreMediaIO's "allow screen capture devices" property makes each connected, trusted iOS
//! device appear as an AVFoundation capture device, which is how QuickTime Player does it. Frames
//! are JPEG-encoded and go to the same canvas as Android video through the same bridge.
//!
//! NOT YET RUN ON REAL HARDWARE: this module is type-checked for macOS targets but has never been
//! executed, because development so far has had no Mac or iPhone. See docs/TESTING.md.

use crate::android::session::{AudioState, MirrorPhase, MirrorStatus};
use crate::bridge::{frame, Bridge, BridgeHandler, KIND_JPEG};
use crate::error::{AppError, AppResult};
use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchQueueAttr};
use jpeg_encoder::{ColorType, Encoder};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput,
    AVCaptureOutput, AVCaptureSession, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeMuxed, AVMediaTypeVideo,
};
use objc2_core_foundation::CFString;
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, CVPixelBuffer,
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const JPEG_QUALITY: u8 = 80;
const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(33);
const DEVICE_WAIT: Duration = Duration::from_secs(8);

#[repr(C)]
struct CMIOObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreMediaIO", kind = "framework")]
extern "C" {
    fn CMIOObjectSetPropertyData(
        object_id: u32,
        address: *const CMIOObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> i32;
}

const CMIO_SYSTEM_OBJECT: u32 = 1;
const PROPERTY_ALLOW_SCREEN_CAPTURE: u32 = u32::from_be_bytes(*b"yes ");
const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");

fn enable_screen_capture_devices() -> AppResult<()> {
    let address = CMIOObjectPropertyAddress {
        selector: PROPERTY_ALLOW_SCREEN_CAPTURE,
        scope: SCOPE_GLOBAL,
        element: 0,
    };
    let allow: u32 = 1;
    let status = unsafe {
        CMIOObjectSetPropertyData(
            CMIO_SYSTEM_OBJECT,
            &address,
            0,
            std::ptr::null(),
            size_of::<u32>() as u32,
            (&raw const allow).cast(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(AppError::Mirror(format!("macOS refused device screen capture (status {status}).")))
    }
}

/// macOS gates iOS screen capture behind the camera permission.
fn ensure_camera_permission() -> AppResult<()> {
    let media = unsafe { AVMediaTypeVideo }
        .ok_or_else(|| AppError::Mirror("AVFoundation is unavailable.".into()))?;
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    if status == AVAuthorizationStatus::Authorized {
        return Ok(());
    }
    if status != AVAuthorizationStatus::NotDetermined {
        return Err(AppError::Unsupported(
            "Allow ScreenLink Studio under System Settings > Privacy & Security > Camera.".into(),
        ));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let block = RcBlock::new(move |granted: Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &block) };
    match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(true) => Ok(()),
        _ => Err(AppError::Unsupported("Camera access is required to show an iPhone screen.".into())),
    }
}

fn find_device(udid: &str) -> Option<Retained<AVCaptureDevice>> {
    let wanted = udid.replace('-', "");
    let media = unsafe { AVMediaTypeMuxed }?;
    let deadline = Instant::now() + DEVICE_WAIT;
    loop {
        #[allow(deprecated)]
        let devices = unsafe { AVCaptureDevice::devicesWithMediaType(media) };
        for device in devices.iter() {
            let id = unsafe { device.uniqueID() }.to_string().replace('-', "");
            if id.eq_ignore_ascii_case(&wanted) {
                return Some(device);
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        // The device appears asynchronously after the CoreMediaIO property is set.
        std::thread::sleep(Duration::from_millis(250));
    }
}

struct DelegateState {
    bridge: Arc<Bridge>,
    last_frame: Mutex<Option<Instant>>,
    announced: AtomicBool,
    app: AppHandle,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ScreenLinkFrameDelegate"]
    #[ivars = DelegateState]
    struct FrameDelegate;

    unsafe impl NSObjectProtocol for FrameDelegate {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for FrameDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn capture_output(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            self.handle_sample(sample);
        }
    }
);

impl FrameDelegate {
    fn create(state: DelegateState) -> Retained<Self> {
        let this = Self::alloc().set_ivars(state);
        unsafe { msg_send![super(this), init] }
    }

    fn handle_sample(&self, sample: &CMSampleBuffer) {
        let state = self.ivars();
        {
            let mut last = state.last_frame.lock().unwrap();
            if last.is_some_and(|t| t.elapsed() < MIN_FRAME_INTERVAL) {
                return;
            }
            *last = Some(Instant::now());
        }
        let Some(buffer) = (unsafe { sample.image_buffer() }) else { return };
        let Some((jpeg, width, height)) = encode_jpeg(&buffer) else { return };
        state.bridge.send(frame(KIND_JPEG, 0, 0, &jpeg));

        if !state.announced.swap(true, Ordering::SeqCst) {
            update_size(&state.app, width, height);
        }
    }
}

fn encode_jpeg(buffer: &CVPixelBuffer) -> Option<(Vec<u8>, u32, u32)> {
    unsafe {
        CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly);
        let (width, height) = (CVPixelBufferGetWidth(buffer), CVPixelBufferGetHeight(buffer));
        let stride = CVPixelBufferGetBytesPerRow(buffer);
        let base = CVPixelBufferGetBaseAddress(buffer) as *const u8;
        let result = (!base.is_null()).then(|| {
            let mut packed = Vec::with_capacity(width * height * 4);
            for row in 0..height {
                packed.extend_from_slice(std::slice::from_raw_parts(base.add(row * stride), width * 4));
            }
            packed
        });
        CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly);

        let mut jpeg = Vec::with_capacity(512 * 1024);
        Encoder::new(&mut jpeg, JPEG_QUALITY)
            .encode(&result?, width as u16, height as u16, ColorType::Bgra)
            .ok()?;
        Some((jpeg, width as u32, height as u32))
    }
}

/// Status updates live beside the capture so the delegate thread can publish the first frame size.
static STATUS: Mutex<Option<MirrorStatus>> = Mutex::new(None);

fn publish(app: &AppHandle, status: MirrorStatus) {
    *STATUS.lock().unwrap() = Some(status.clone());
    let _ = app.emit("mirror-status", status);
}

fn update_size(app: &AppHandle, width: u32, height: u32) {
    let current = STATUS.lock().unwrap().clone();
    if let Some(mut status) = current {
        status.width = width;
        status.height = height;
        publish(app, status);
    }
}

struct NoInput;

impl BridgeHandler for NoInput {
    fn media_connected(&self, _reconnect: bool) {}
    // iOS has no input injection API, so anything arriving here is ignored by design.
    fn input(&self, _json: &str) {}
}

struct Running {
    session: Retained<AVCaptureSession>,
    // Kept alive for as long as the session holds a weak reference to it.
    _delegate: Retained<FrameDelegate>,
    _bridge: Arc<Bridge>,
}

// AVCaptureSession documents startRunning/stopRunning as callable from any serial context, and
// every access here goes through the mutex below.
unsafe impl Send for Running {}

#[derive(Default)]
pub struct IosCapture {
    running: Mutex<Option<Running>>,
}

impl IosCapture {
    pub fn start(&self, app: &AppHandle, udid: &str) -> AppResult<MirrorStatus> {
        self.stop(app);
        publish(app, MirrorStatus {
            phase: MirrorPhase::Starting,
            step: Some("connecting"),
            serial: Some(udid.to_string()),
            ..MirrorStatus::idle()
        });

        match self.open(app, udid) {
            Ok(status) => Ok(status),
            Err(e) => {
                publish(app, MirrorStatus { ended_reason: Some(e.to_string()), ..MirrorStatus::idle() });
                Err(e)
            }
        }
    }

    fn open(&self, app: &AppHandle, udid: &str) -> AppResult<MirrorStatus> {
        ensure_camera_permission()?;
        enable_screen_capture_devices()?;
        let device = find_device(udid).ok_or_else(|| {
            AppError::Mirror("macOS did not offer this device for capture. Unlock it and try again.".into())
        })?;

        let bridge = Arc::new(Bridge::start(Arc::new(NoInput))?);
        let delegate = FrameDelegate::create(DelegateState {
            bridge: bridge.clone(),
            last_frame: Mutex::new(None),
            announced: AtomicBool::new(false),
            app: app.clone(),
        });

        unsafe {
            let session = AVCaptureSession::new();
            let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&device)
                .map_err(|e| AppError::Mirror(format!("Could not open the device: {e}")))?;
            let output = AVCaptureVideoDataOutput::new();

            // CFString and NSString are toll-free bridged.
            let key = &*(kCVPixelBufferPixelFormatTypeKey as *const CFString as *const NSString);
            let value = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
            let video_settings = NSDictionary::from_slices(&[key], &[&*value]);
            output.setVideoSettings(Some(video_settings.cast_unchecked()));

            let queue = DispatchQueue::new("com.screenlink.capture", DispatchQueueAttr::SERIAL);
            output.setSampleBufferDelegate_queue(
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&queue),
            );

            session.beginConfiguration();
            if !session.canAddInput(&input) || !session.canAddOutput(&output) {
                session.commitConfiguration();
                return Err(AppError::Mirror("macOS could not configure the capture session.".into()));
            }
            session.addInput(&input);
            session.addOutput(&output);
            session.commitConfiguration();
            session.startRunning();

            let status = MirrorStatus {
                phase: MirrorPhase::Running,
                serial: Some(udid.to_string()),
                device_name: Some(device.localizedName().to_string()),
                audio: AudioState::Off,
                port: bridge.port,
                token: bridge.token.clone(),
                ..MirrorStatus::idle()
            };
            publish(app, status.clone());
            *self.running.lock().unwrap() =
                Some(Running { session, _delegate: delegate, _bridge: bridge });
            Ok(status)
        }
    }

    pub fn stop(&self, app: &AppHandle) {
        if let Some(running) = self.running.lock().unwrap().take() {
            unsafe { running.session.stopRunning() };
            publish(app, MirrorStatus::idle());
        }
    }
}
