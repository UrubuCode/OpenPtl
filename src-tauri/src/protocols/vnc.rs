use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use lz4_flex::frame::FrameEncoder;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::libs::models::BackendMessage;
use crate::protocols::rdp::{RdpInputBatch, RdpInputEvent, RdpMouseButton};
use crate::protocols::webrtc_stream;

pub type VncInputEvent = RdpInputEvent;
pub type VncInputBatch = RdpInputBatch;
pub use crate::protocols::rdp::RdpSessionFocusInput as VncSessionFocusInput;

const DEFAULT_VNC_PORT: u16 = 5900;
const RFB_VERSION: &[u8] = b"RFB 003.008\n";
const SECURITY_NONE: u8 = 1;
const SECURITY_VNC_AUTH: u8 = 2;

const VIDEO_PACKET_MAGIC: [u8; 4] = *b"TVNV";
const PACKET_VERSION: u8 = 1;
const VIDEO_PACKET_HEADER_LEN: usize = 32;
const VIDEO_RECT_HEADER_LEN: usize = 20;
const VIDEO_FRAME_BEGIN: u8 = 0b0000_0001;
const VIDEO_FRAME_END: u8 = 0b0000_0010;
const COMPRESSION_NONE: u8 = 0;
const COMPRESSION_LZ4: u8 = 1;

const ENCODING_CURSOR: i32 = -239;
const ENCODING_DESKTOP_SIZE: i32 = -223;

#[derive(Debug, Clone)]
pub struct VncSessionOptions {
    pub host: String,
    pub port: u16,
    pub password: String,
    pub timeout_seconds: u64,
    pub webrtc_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum VncSessionStartResult {
    Started { session_id: String },
    Error { message: BackendMessage },
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum VncSessionControlEvent {
    Connecting {
        session_id: String,
        message: BackendMessage,
    },
    Ready {
        session_id: String,
        width: u16,
        height: u16,
    },
    AuthRequired {
        session_id: String,
        message: BackendMessage,
    },
    Error {
        session_id: String,
        message: BackendMessage,
    },
    Stopped {
        session_id: String,
    },
    ReleasedCapture {
        session_id: String,
        message: BackendMessage,
    },
}

#[derive(Debug)]
enum VncSessionWorkerMessage {
    InputBatch(VncInputBatch),
    Focus(VncSessionFocusInput),
    Stop,
}

struct VncSessionWorker {
    tx: mpsc::Sender<VncSessionWorkerMessage>,
    _join: JoinHandle<()>,
}

#[derive(Default)]
pub struct VncSessionManager {
    sessions: HashMap<String, VncSessionWorker>,
}

impl VncSessionManager {
    pub fn start(
        &mut self,
        options: VncSessionOptions,
        control_channel: Channel<VncSessionControlEvent>,
        video_rects_channel: Channel<InvokeResponseBody>,
        cursor_channel: Channel<InvokeResponseBody>,
        app_handle: AppHandle,
    ) -> VncSessionStartResult {
        let session_id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel::<VncSessionWorkerMessage>();
        let control_channel = Arc::new(control_channel);
        let video_rects_channel = Arc::new(video_rects_channel);
        let cursor_channel = Arc::new(cursor_channel);
        let worker_session_id = session_id.clone();

        let worker_tx_for_input = tx.clone();
        let join = std::thread::Builder::new()
            .name(format!("vnc-session-{}", &session_id[..8]))
            .spawn(move || {
                run_vnc_session_worker(
                    worker_session_id,
                    options,
                    control_channel,
                    video_rects_channel,
                    cursor_channel,
                    rx,
                    worker_tx_for_input,
                    app_handle,
                );
            });

        let Ok(join) = join else {
            return VncSessionStartResult::Error {
                message: BackendMessage::key("vnc_worker_start_failed"),
            };
        };

        self.sessions
            .insert(session_id.clone(), VncSessionWorker { tx, _join: join });
        VncSessionStartResult::Started { session_id }
    }

    pub fn focus(&mut self, session_id: &str, focus: VncSessionFocusInput) -> Result<()> {
        let Some(session) = self.sessions.get(session_id) else {
            anyhow::bail!("vnc_session_not_found");
        };
        if let Err(error) = session.tx.send(VncSessionWorkerMessage::Focus(focus)) {
            self.sessions.remove(session_id);
            return Err(anyhow::Error::new(error).context("vnc focus send"));
        }
        Ok(())
    }

    pub fn input_batch(&mut self, session_id: &str, batch: VncInputBatch) -> Result<()> {
        let Some(session) = self.sessions.get(session_id) else {
            anyhow::bail!("vnc_session_not_found");
        };
        if let Err(error) = session.tx.send(VncSessionWorkerMessage::InputBatch(batch)) {
            self.sessions.remove(session_id);
            return Err(anyhow::Error::new(error).context("vnc input batch send"));
        }
        Ok(())
    }

    pub fn stop(&mut self, session_id: &str) -> Result<()> {
        let Some(session) = self.sessions.remove(session_id) else {
            anyhow::bail!("vnc_session_not_found");
        };
        let _ = session.tx.send(VncSessionWorkerMessage::Stop);
        Ok(())
    }
}

fn run_vnc_session_worker(
    session_id: String,
    options: VncSessionOptions,
    control_channel: Arc<Channel<VncSessionControlEvent>>,
    video_rects_channel: Arc<Channel<InvokeResponseBody>>,
    cursor_channel: Arc<Channel<InvokeResponseBody>>,
    rx: mpsc::Receiver<VncSessionWorkerMessage>,
    worker_tx_for_input: mpsc::Sender<VncSessionWorkerMessage>,
    app_handle: AppHandle,
) {
    if control_channel
        .send(VncSessionControlEvent::Connecting {
            session_id: session_id.clone(),
            message: BackendMessage::key("vnc_connecting"),
        })
        .is_err()
    {
        return;
    }

    let result = run_vnc_session_worker_inner(
        &session_id,
        options,
        &control_channel,
        &video_rects_channel,
        &cursor_channel,
        rx,
        worker_tx_for_input,
        app_handle.clone(),
    );

    let registry = webrtc_stream::registry();
    let rt = webrtc_stream::runtime();
    rt.block_on(async {
        if let Some(peer) = registry.lock().await.remove(&session_id) {
            peer.close().await;
        }
    });

    if let Err(error) = result {
        let reason = format!("{error:#}");
        let is_auth = reason.contains("vnc_auth") || reason.contains("auth_failed");
        let _ = if is_auth {
            control_channel.send(VncSessionControlEvent::AuthRequired {
                session_id: session_id.clone(),
                message: BackendMessage::key("vnc_auth_failed"),
            })
        } else {
            let mut params = HashMap::new();
            params.insert("reason".to_string(), reason);
            control_channel.send(VncSessionControlEvent::Error {
                session_id: session_id.clone(),
                message: BackendMessage::with_params("vnc_runtime_error", params),
            })
        };
    }

    let _ = control_channel.send(VncSessionControlEvent::Stopped {
        session_id: session_id.clone(),
    });
}

struct PointerState {
    button_mask: u8,
    last_x: u16,
    last_y: u16,
}

impl Default for PointerState {
    fn default() -> Self {
        Self {
            button_mask: 0,
            last_x: 0,
            last_y: 0,
        }
    }
}

fn run_vnc_session_worker_inner(
    session_id: &str,
    options: VncSessionOptions,
    control_channel: &Arc<Channel<VncSessionControlEvent>>,
    video_rects_channel: &Arc<Channel<InvokeResponseBody>>,
    _cursor_channel: &Arc<Channel<InvokeResponseBody>>,
    rx: mpsc::Receiver<VncSessionWorkerMessage>,
    worker_tx_for_input: mpsc::Sender<VncSessionWorkerMessage>,
    app_handle: AppHandle,
) -> Result<()> {
    let port = if options.port == 0 {
        DEFAULT_VNC_PORT
    } else {
        options.port
    };
    let addr = format!("{}:{}", options.host, port);
    let connect_timeout = Duration::from_secs(options.timeout_seconds.clamp(3, 30));

    let sock_addr = addr
        .to_socket_addrs()
        .with_context(|| format!("resolve {}", addr))?
        .next()
        .ok_or_else(|| anyhow!("vnc_host_invalid"))?;

    let stream = TcpStream::connect_timeout(&sock_addr, connect_timeout)
        .with_context(|| format!("vnc connect {}", addr))?;
    stream.set_nodelay(true).ok();
    stream
        .set_read_timeout(Some(Duration::from_millis(80)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .ok();

    let mut writer = stream.try_clone().context("vnc stream clone")?;
    let mut reader = BufReader::new(stream);

    // RFB version handshake
    let mut version_buf = [0u8; 12];
    reader
        .read_exact(&mut version_buf)
        .context("read rfb version")?;
    writer
        .write_all(RFB_VERSION)
        .context("write rfb version")?;

    // Security types
    let num_types = read_u8(&mut reader)?;
    if num_types == 0 {
        let reason_len = read_u32_be(&mut reader).unwrap_or(0);
        let mut reason = vec![0u8; reason_len.min(512) as usize];
        let _ = reader.read_exact(&mut reason);
        bail!("vnc_server_refused");
    }
    let mut types = vec![0u8; num_types as usize];
    reader
        .read_exact(&mut types)
        .context("read security types")?;

    let security_type = if types.contains(&SECURITY_VNC_AUTH) {
        SECURITY_VNC_AUTH
    } else if types.contains(&SECURITY_NONE) {
        SECURITY_NONE
    } else {
        bail!("vnc_no_supported_security");
    };
    writer
        .write_all(&[security_type])
        .context("write security type")?;

    if security_type == SECURITY_VNC_AUTH {
        let mut challenge = [0u8; 16];
        reader
            .read_exact(&mut challenge)
            .context("read vnc challenge")?;
        let response = vnc_auth_response(options.password.as_str(), &challenge);
        writer
            .write_all(&response)
            .context("write vnc auth response")?;
    }

    // Security result (RFB 3.8 always sends it)
    let security_result = read_u32_be(&mut reader).context("read security result")?;
    if security_result != 0 {
        bail!("vnc_auth_failed");
    }

    // ClientInit: shared=1
    writer.write_all(&[1u8]).context("write client init")?;

    // ServerInit
    let server_width = read_u16_be(&mut reader).context("read server width")?;
    let server_height = read_u16_be(&mut reader).context("read server height")?;
    let mut _pixel_format = [0u8; 16];
    reader
        .read_exact(&mut _pixel_format)
        .context("read pixel format")?;
    let name_len = read_u32_be(&mut reader).context("read name length")?;
    let mut name_buf = vec![0u8; name_len.min(256) as usize];
    reader.read_exact(&mut name_buf).context("read server name")?;

    let mut server_width = server_width;
    let mut server_height = server_height;

    // Request 32bpp BGRA little-endian (red_shift=16, green_shift=8, blue_shift=0)
    send_set_pixel_format(&mut writer)?;
    // Raw (0) + Cursor (-239) + DesktopSize (-223) pseudo-encodings.
    send_set_encodings(&mut writer, &[0i32, ENCODING_CURSOR, ENCODING_DESKTOP_SIZE])?;

    if control_channel
        .send(VncSessionControlEvent::Ready {
            session_id: session_id.to_string(),
            width: server_width,
            height: server_height,
        })
        .is_err()
    {
        return Ok(());
    }

    let image_size = server_width as usize * server_height as usize * 4;
    let mut image: Vec<u8> = vec![0u8; image_size];
    let mut frame_id: u64 = 0;
    let stream_started = Instant::now();
    let mut last_emit = Instant::now();
    let mut pointer = PointerState::default();
    let mut focused = true;

    let webrtc_peer: Option<Arc<webrtc_stream::StreamPeer>> = if options.webrtc_enabled {
        let rt = webrtc_stream::runtime();
        let ice_app_handle = app_handle.clone();
        let ice_event_name = format!("vnc-ice-{}", session_id);
        let result = rt.block_on(async {
            webrtc_stream::StreamPeer::new(
                server_width as u32,
                server_height as u32,
                move |candidate| {
                    let _ = ice_app_handle.emit(&ice_event_name, candidate);
                },
            )
            .await
        });
        match result {
            Ok((peer, mut input_rx)) => {
                let peer = Arc::new(peer);
                let registry = webrtc_stream::registry();
                let session_key = session_id.to_string();
                rt.block_on(async {
                    registry.lock().await.insert(session_key.clone(), Arc::clone(&peer));
                });
                let worker_tx = worker_tx_for_input.clone();
                rt.spawn(async move {
                    while let Some(bytes) = input_rx.recv().await {
                        if let Ok(batch) = serde_json::from_slice::<RdpInputBatch>(&bytes) {
                            let _ = worker_tx.send(VncSessionWorkerMessage::InputBatch(batch));
                        }
                    }
                });
                Some(peer)
            }
            Err(error) => {
                let _ = control_channel.send(VncSessionControlEvent::Error {
                    session_id: session_id.to_string(),
                    message: BackendMessage::key(format!("vnc_webrtc_init_failed: {error:#}")),
                });
                None
            }
        }
    } else {
        None
    };

    send_fb_update_request(&mut writer, 0, 0, 0, server_width, server_height)?;

    'worker: loop {
        loop {
            match rx.try_recv() {
                Ok(VncSessionWorkerMessage::Stop) => break 'worker,
                Ok(VncSessionWorkerMessage::Focus(focus)) => {
                    let was_focused = focused;
                    focused = focus.focused;
                    if was_focused && !focused {
                        let _ = control_channel.send(VncSessionControlEvent::ReleasedCapture {
                            session_id: session_id.to_string(),
                            message: BackendMessage::key("vnc_capture_released"),
                        });
                    }
                }
                Ok(VncSessionWorkerMessage::InputBatch(batch)) => {
                    for event in &batch.events {
                        if let Err(error) = send_input_event(&mut writer, event, &mut pointer) {
                            let _ = control_channel.send(VncSessionControlEvent::Error {
                                session_id: session_id.to_string(),
                                message: BackendMessage::key(format!("vnc_input_error: {error:#}")),
                            });
                            break 'worker;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break 'worker,
            }
        }

        let mut type_buf = [0u8; 1];
        match reader.read_exact(&mut type_buf) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                // Viewer requested a keyframe (RTCP PLI) while the screen is idle:
                // re-encode the current framebuffer so it isn't stuck black.
                if let Some(peer) = webrtc_peer.as_ref() {
                    if peer.keyframe_requested() {
                        let rt = webrtc_stream::runtime();
                        let peer = Arc::clone(peer);
                        let snapshot = image.clone();
                        rt.spawn(async move {
                            let _ = peer.push_bgra_frame(&snapshot).await;
                        });
                    }
                }
                send_fb_update_request(&mut writer, 1, 0, 0, server_width, server_height)?;
                continue;
            }
            Err(error) => {
                return Err(anyhow::Error::new(error).context("read vnc server msg type"))
            }
        }

        match type_buf[0] {
            0 => {
                let _padding = read_u8(&mut reader)?;
                let num_rects = read_u16_be(&mut reader)?;
                let mut dirty: Vec<(u16, u16, u16, u16)> = Vec::new();

                for _ in 0..num_rects {
                    let x = read_u16_be(&mut reader)?;
                    let y = read_u16_be(&mut reader)?;
                    let w = read_u16_be(&mut reader)?;
                    let h = read_u16_be(&mut reader)?;
                    let encoding = read_i32_be(&mut reader)?;

                    match encoding {
                        0 => {
                            let size = w as usize * h as usize * 4;
                            if size > 0 {
                                let mut buf = vec![0u8; size];
                                reader
                                    .read_exact(&mut buf)
                                    .context("read raw rect pixels")?;
                                apply_rect(&mut image, server_width, x, y, w, h, &buf);
                                dirty.push((x, y, w, h));
                            }
                        }
                        ENCODING_CURSOR => {
                            // RichCursor: (x, y) = hotspot, (w, h) = cursor size.
                            let pixels_len = w as usize * h as usize * 4;
                            let mask_len = ((w as usize + 7) / 8) * h as usize;
                            let mut pixels = vec![0u8; pixels_len];
                            if pixels_len > 0 {
                                reader
                                    .read_exact(&mut pixels)
                                    .context("read cursor pixels")?;
                            }
                            let mut mask = vec![0u8; mask_len];
                            if mask_len > 0 {
                                reader.read_exact(&mut mask).context("read cursor mask")?;
                            }
                            if let Some(peer) = webrtc_peer.as_ref() {
                                let json = build_cursor_json(x, y, w, h, &pixels, &mask);
                                let rt = webrtc_stream::runtime();
                                let peer = Arc::clone(peer);
                                rt.spawn(async move {
                                    let _ = peer.push_cursor_update(&json).await;
                                });
                            }
                        }
                        ENCODING_DESKTOP_SIZE => {
                            // DesktopSize: (w, h) carry the new framebuffer size.
                            if w != 0
                                && h != 0
                                && (w != server_width || h != server_height)
                            {
                                server_width = w;
                                server_height = h;
                                image = vec![0u8; w as usize * h as usize * 4];
                                dirty.clear();
                                if let Some(peer) = webrtc_peer.as_ref() {
                                    let rt = webrtc_stream::runtime();
                                    let peer = Arc::clone(peer);
                                    let (nw, nh) = (w as u32, h as u32);
                                    rt.block_on(async move {
                                        let _ = peer.resize(nw, nh).await;
                                    });
                                }
                                send_fb_update_request(
                                    &mut writer,
                                    0,
                                    0,
                                    0,
                                    server_width,
                                    server_height,
                                )?;
                            }
                        }
                        _ => {}
                    }
                }

                if !dirty.is_empty() {
                    let emit_interval = if focused {
                        Duration::from_millis(16)
                    } else {
                        Duration::from_millis(110)
                    };
                    if last_emit.elapsed() >= emit_interval {
                        if let Some(peer) = webrtc_peer.as_ref() {
                            let rt = webrtc_stream::runtime();
                            let peer = Arc::clone(peer);
                            let snapshot = image.clone();
                            rt.spawn(async move {
                                let _ = peer.push_bgra_frame(&snapshot).await;
                            });
                        } else {
                            let pts_us = stream_started
                                .elapsed()
                                .as_micros()
                                .min(u128::from(u64::MAX))
                                as u64;
                            let chunks: Result<Vec<_>> = dirty
                                .iter()
                                .take(48)
                                .map(|(x, y, w, h)| {
                                    build_video_rect_chunk(&image, server_width, *x, *y, *w, *h)
                                })
                                .collect();
                            let chunks = chunks?;
                            let packet = build_video_rects_packet(
                                frame_id,
                                server_width,
                                server_height,
                                pts_us,
                                &chunks,
                            )?;
                            if video_rects_channel
                                .send(InvokeResponseBody::Raw(packet))
                                .is_err()
                            {
                                break 'worker;
                            }
                        }
                        frame_id = frame_id.wrapping_add(1);
                        last_emit = Instant::now();
                    }
                    send_fb_update_request(&mut writer, 1, 0, 0, server_width, server_height)?;
                }
            }
            1 => {
                let _padding = read_u8(&mut reader)?;
                let _first_colour = read_u16_be(&mut reader)?;
                let num_colours = read_u16_be(&mut reader)?;
                for _ in 0..num_colours {
                    let mut rgb = [0u8; 6];
                    reader.read_exact(&mut rgb).context("read colour entry")?;
                }
            }
            2 => {}
            3 => {
                let mut padding = [0u8; 3];
                reader
                    .read_exact(&mut padding)
                    .context("read cut text padding")?;
                let len = read_u32_be(&mut reader)?;
                let mut buf = vec![0u8; len.min(65536) as usize];
                reader
                    .read_exact(&mut buf)
                    .context("read server cut text")?;
            }
            _ => {
                bail!("vnc_unknown_server_message");
            }
        }
    }

    Ok(())
}

fn build_cursor_json(
    hotspot_x: u16,
    hotspot_y: u16,
    width: u16,
    height: u16,
    bgra: &[u8],
    mask: &[u8],
) -> String {
    if width == 0 || height == 0 {
        return r#"{"visible":false}"#.to_string();
    }
    let (w, h) = (width as usize, height as usize);
    let mask_stride = (w + 7) / 8;
    let mut rgba = vec![0u8; w * h * 4];
    for row in 0..h {
        for col in 0..w {
            let px = (row * w + col) * 4;
            if px + 3 >= bgra.len() {
                continue;
            }
            let mask_byte = row * mask_stride + col / 8;
            let opaque = mask
                .get(mask_byte)
                .map(|b| (b >> (7 - (col % 8))) & 1 == 1)
                .unwrap_or(false);
            rgba[px] = bgra[px + 2];
            rgba[px + 1] = bgra[px + 1];
            rgba[px + 2] = bgra[px];
            rgba[px + 3] = if opaque { 255 } else { 0 };
        }
    }
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &rgba);
    format!(
        r#"{{"visible":true,"hotspotX":{hotspot_x},"hotspotY":{hotspot_y},"width":{width},"height":{height},"rgbaBase64":"{encoded}"}}"#
    )
}

fn apply_rect(
    image: &mut Vec<u8>,
    image_width: u16,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    data: &[u8],
) {
    let iw = image_width as usize;
    for row in 0..h as usize {
        let dst_start = ((y as usize + row) * iw + x as usize) * 4;
        let src_start = row * w as usize * 4;
        let len = w as usize * 4;
        if dst_start + len <= image.len() && src_start + len <= data.len() {
            for i in 0..w as usize {
                let d = dst_start + i * 4;
                let s = src_start + i * 4;
                image[d] = data[s];
                image[d + 1] = data[s + 1];
                image[d + 2] = data[s + 2];
                image[d + 3] = 255;
            }
        }
    }
}

struct VideoRectChunk {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    raw_size: u32,
    compression: u8,
    payload: Vec<u8>,
}

fn build_video_rect_chunk(
    image: &[u8],
    image_width: u16,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
) -> Result<VideoRectChunk> {
    let iw = image_width as usize;
    let mut raw = Vec::with_capacity(w as usize * h as usize * 4);
    for row in 0..h as usize {
        let src_start = ((y as usize + row) * iw + x as usize) * 4;
        let len = w as usize * 4;
        if src_start + len <= image.len() {
            raw.extend_from_slice(&image[src_start..src_start + len]);
        } else {
            raw.extend(std::iter::repeat(0u8).take(len));
        }
    }
    let raw_size = u32::try_from(raw.len()).context("raw rect overflow")?;
    let (compression, payload) = compress_payload(&raw)?;
    Ok(VideoRectChunk {
        x,
        y,
        width: w,
        height: h,
        raw_size,
        compression,
        payload,
    })
}

fn compress_payload(raw: &[u8]) -> Result<(u8, Vec<u8>)> {
    let mut compressed = Vec::new();
    {
        let mut encoder = FrameEncoder::new(&mut compressed);
        encoder.write_all(raw).context("lz4 write")?;
        let _ = encoder.finish().context("lz4 finish")?;
    }
    if compressed.len() + 16 < raw.len() {
        Ok((COMPRESSION_LZ4, compressed))
    } else {
        Ok((COMPRESSION_NONE, raw.to_vec()))
    }
}

fn build_video_rects_packet(
    frame_id: u64,
    width: u16,
    height: u16,
    pts_us: u64,
    rects: &[VideoRectChunk],
) -> Result<Vec<u8>> {
    let flags = VIDEO_FRAME_BEGIN | VIDEO_FRAME_END;
    let rect_count = u16::try_from(rects.len()).context("too many rects")?;
    let total_payload_len = rects
        .iter()
        .map(|r| VIDEO_RECT_HEADER_LEN + r.payload.len())
        .sum::<usize>();

    let mut packet = Vec::with_capacity(VIDEO_PACKET_HEADER_LEN + total_payload_len);
    packet.extend_from_slice(&VIDEO_PACKET_MAGIC);
    packet.push(PACKET_VERSION);
    packet.push(flags);
    packet.extend_from_slice(&0u16.to_le_bytes());
    packet.extend_from_slice(&frame_id.to_le_bytes());
    packet.extend_from_slice(&width.to_le_bytes());
    packet.extend_from_slice(&height.to_le_bytes());
    packet.extend_from_slice(&rect_count.to_le_bytes());
    packet.extend_from_slice(&0u16.to_le_bytes());
    packet.extend_from_slice(&pts_us.to_le_bytes());

    for rect in rects {
        let payload_len = u32::try_from(rect.payload.len()).context("rect payload overflow")?;
        packet.extend_from_slice(&rect.x.to_le_bytes());
        packet.extend_from_slice(&rect.y.to_le_bytes());
        packet.extend_from_slice(&rect.width.to_le_bytes());
        packet.extend_from_slice(&rect.height.to_le_bytes());
        packet.push(rect.compression);
        packet.extend_from_slice(&[0u8; 3]);
        packet.extend_from_slice(&rect.raw_size.to_le_bytes());
        packet.extend_from_slice(&payload_len.to_le_bytes());
        packet.extend_from_slice(rect.payload.as_slice());
    }

    Ok(packet)
}

fn send_set_pixel_format(writer: &mut impl Write) -> Result<()> {
    // SetPixelFormat: 32bpp BGRA little-endian (red_shift=16, green_shift=8, blue_shift=0)
    let msg: [u8; 20] = [
        0,    // message type
        0, 0, 0, // padding
        32,   // bits_per_pixel
        24,   // depth
        0,    // big_endian_flag (little-endian)
        1,    // true_colour_flag
        0, 255, // red_max (big-endian u16)
        0, 255, // green_max
        0, 255, // blue_max
        16,   // red_shift
        8,    // green_shift
        0,    // blue_shift
        0, 0, 0, // padding
    ];
    writer.write_all(&msg).context("send SetPixelFormat")
}

fn send_set_encodings(writer: &mut impl Write, encodings: &[i32]) -> Result<()> {
    let count = encodings.len() as u16;
    let mut msg = Vec::with_capacity(4 + encodings.len() * 4);
    msg.push(2u8);
    msg.push(0);
    msg.extend_from_slice(&count.to_be_bytes());
    for &enc in encodings {
        msg.extend_from_slice(&enc.to_be_bytes());
    }
    writer.write_all(&msg).context("send SetEncodings")
}

fn send_fb_update_request(
    writer: &mut impl Write,
    incremental: u8,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
) -> Result<()> {
    let msg: [u8; 10] = [
        3,
        incremental,
        (x >> 8) as u8,
        x as u8,
        (y >> 8) as u8,
        y as u8,
        (w >> 8) as u8,
        w as u8,
        (h >> 8) as u8,
        h as u8,
    ];
    writer
        .write_all(&msg)
        .context("send FramebufferUpdateRequest")
}

fn send_pointer_event(writer: &mut impl Write, button_mask: u8, x: u16, y: u16) -> Result<()> {
    let msg: [u8; 6] = [
        5,
        button_mask,
        (x >> 8) as u8,
        x as u8,
        (y >> 8) as u8,
        y as u8,
    ];
    writer.write_all(&msg).context("send PointerEvent")
}

fn send_key_event(writer: &mut impl Write, down: bool, keysym: u32) -> Result<()> {
    let msg: [u8; 8] = [
        4,
        if down { 1 } else { 0 },
        0,
        0,
        ((keysym >> 24) & 0xff) as u8,
        ((keysym >> 16) & 0xff) as u8,
        ((keysym >> 8) & 0xff) as u8,
        (keysym & 0xff) as u8,
    ];
    writer.write_all(&msg).context("send KeyEvent")
}

fn send_input_event(
    writer: &mut impl Write,
    event: &VncInputEvent,
    pointer: &mut PointerState,
) -> Result<()> {
    match event {
        RdpInputEvent::MouseMove { x, y, .. } => {
            pointer.last_x = *x;
            pointer.last_y = *y;
            send_pointer_event(writer, pointer.button_mask, *x, *y)?;
        }
        RdpInputEvent::MouseButtonDown { x, y, button } => {
            pointer.last_x = *x;
            pointer.last_y = *y;
            pointer.button_mask |= rdp_button_to_vnc_mask(button);
            send_pointer_event(writer, pointer.button_mask, *x, *y)?;
        }
        RdpInputEvent::MouseButtonUp { x, y, button } => {
            pointer.last_x = *x;
            pointer.last_y = *y;
            pointer.button_mask &= !rdp_button_to_vnc_mask(button);
            send_pointer_event(writer, pointer.button_mask, *x, *y)?;
        }
        RdpInputEvent::MouseClick {
            x,
            y,
            button,
            double_click,
        } => {
            let mask = rdp_button_to_vnc_mask(button);
            pointer.last_x = *x;
            pointer.last_y = *y;
            send_pointer_event(writer, pointer.button_mask | mask, *x, *y)?;
            send_pointer_event(writer, pointer.button_mask, *x, *y)?;
            if *double_click {
                send_pointer_event(writer, pointer.button_mask | mask, *x, *y)?;
                send_pointer_event(writer, pointer.button_mask, *x, *y)?;
            }
        }
        RdpInputEvent::MouseScroll { x, y, delta_y, .. } => {
            pointer.last_x = *x;
            pointer.last_y = *y;
            let scroll_mask = if *delta_y >= 0 { 0x08u8 } else { 0x10u8 };
            let steps = (delta_y.unsigned_abs() as u32).max(1).min(5);
            for _ in 0..steps {
                send_pointer_event(writer, pointer.button_mask | scroll_mask, *x, *y)?;
                send_pointer_event(writer, pointer.button_mask, *x, *y)?;
            }
        }
        RdpInputEvent::MousePath { points } => {
            for pt in points {
                pointer.last_x = pt.x;
                pointer.last_y = pt.y;
                send_pointer_event(writer, pointer.button_mask, pt.x, pt.y)?;
            }
        }
        RdpInputEvent::KeyPress {
            code,
            text,
            ctrl,
            alt,
            shift,
            meta,
        } => {
            let keysym = web_code_to_keysym(code, text.as_deref());

            if *ctrl {
                send_key_event(writer, true, 0xffe3)?;
            }
            if *alt {
                send_key_event(writer, true, 0xffe9)?;
            }
            if *shift {
                send_key_event(writer, true, 0xffe1)?;
            }
            if *meta {
                send_key_event(writer, true, 0xffeb)?;
            }

            if let Some(sym) = keysym {
                send_key_event(writer, true, sym)?;
                send_key_event(writer, false, sym)?;
            }

            if *meta {
                send_key_event(writer, false, 0xffeb)?;
            }
            if *shift {
                send_key_event(writer, false, 0xffe1)?;
            }
            if *alt {
                send_key_event(writer, false, 0xffe9)?;
            }
            if *ctrl {
                send_key_event(writer, false, 0xffe3)?;
            }
        }
    }
    Ok(())
}

fn rdp_button_to_vnc_mask(button: &RdpMouseButton) -> u8 {
    match button {
        RdpMouseButton::Left => 0x01,
        RdpMouseButton::Middle => 0x02,
        RdpMouseButton::Right => 0x04,
    }
}

fn web_code_to_keysym(code: &str, text: Option<&str>) -> Option<u32> {
    if let Some(text) = text {
        if let Some(ch) = text.chars().next() {
            let cp = ch as u32;
            if cp >= 0x20 && cp <= 0x7e {
                return Some(cp);
            }
        }
    }
    match code {
        "KeyA" => Some(0x0061),
        "KeyB" => Some(0x0062),
        "KeyC" => Some(0x0063),
        "KeyD" => Some(0x0064),
        "KeyE" => Some(0x0065),
        "KeyF" => Some(0x0066),
        "KeyG" => Some(0x0067),
        "KeyH" => Some(0x0068),
        "KeyI" => Some(0x0069),
        "KeyJ" => Some(0x006a),
        "KeyK" => Some(0x006b),
        "KeyL" => Some(0x006c),
        "KeyM" => Some(0x006d),
        "KeyN" => Some(0x006e),
        "KeyO" => Some(0x006f),
        "KeyP" => Some(0x0070),
        "KeyQ" => Some(0x0071),
        "KeyR" => Some(0x0072),
        "KeyS" => Some(0x0073),
        "KeyT" => Some(0x0074),
        "KeyU" => Some(0x0075),
        "KeyV" => Some(0x0076),
        "KeyW" => Some(0x0077),
        "KeyX" => Some(0x0078),
        "KeyY" => Some(0x0079),
        "KeyZ" => Some(0x007a),
        "Digit0" => Some(0x0030),
        "Digit1" => Some(0x0031),
        "Digit2" => Some(0x0032),
        "Digit3" => Some(0x0033),
        "Digit4" => Some(0x0034),
        "Digit5" => Some(0x0035),
        "Digit6" => Some(0x0036),
        "Digit7" => Some(0x0037),
        "Digit8" => Some(0x0038),
        "Digit9" => Some(0x0039),
        "Enter" | "NumpadEnter" => Some(0xff0d),
        "Tab" => Some(0xff09),
        "Backspace" => Some(0xff08),
        "Escape" => Some(0xff1b),
        "Space" => Some(0x0020),
        "Delete" => Some(0xffff),
        "Insert" => Some(0xff63),
        "Home" => Some(0xff50),
        "End" => Some(0xff57),
        "PageUp" => Some(0xff55),
        "PageDown" => Some(0xff56),
        "ArrowLeft" => Some(0xff51),
        "ArrowUp" => Some(0xff52),
        "ArrowRight" => Some(0xff53),
        "ArrowDown" => Some(0xff54),
        "F1" => Some(0xffbe),
        "F2" => Some(0xffbf),
        "F3" => Some(0xffc0),
        "F4" => Some(0xffc1),
        "F5" => Some(0xffc2),
        "F6" => Some(0xffc3),
        "F7" => Some(0xffc4),
        "F8" => Some(0xffc5),
        "F9" => Some(0xffc6),
        "F10" => Some(0xffc7),
        "F11" => Some(0xffc8),
        "F12" => Some(0xffc9),
        "ControlLeft" => Some(0xffe3),
        "ControlRight" => Some(0xffe4),
        "ShiftLeft" => Some(0xffe1),
        "ShiftRight" => Some(0xffe2),
        "AltLeft" => Some(0xffe9),
        "AltRight" => Some(0xffea),
        "MetaLeft" => Some(0xffeb),
        "MetaRight" => Some(0xffec),
        _ => None,
    }
}

fn read_u8(reader: &mut impl Read) -> Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf).context("read u8")?;
    Ok(buf[0])
}

fn read_u16_be(reader: &mut impl Read) -> Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf).context("read u16")?;
    Ok(u16::from_be_bytes(buf))
}

fn read_u32_be(reader: &mut impl Read) -> Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf).context("read u32")?;
    Ok(u32::from_be_bytes(buf))
}

fn read_i32_be(reader: &mut impl Read) -> Result<i32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf).context("read i32")?;
    Ok(i32::from_be_bytes(buf))
}

fn reverse_bits(byte: u8) -> u8 {
    let mut b = byte;
    let mut result = 0u8;
    for _ in 0..8 {
        result = (result << 1) | (b & 1);
        b >>= 1;
    }
    result
}

fn vnc_des_key(password: &str) -> [u8; 8] {
    let mut key = [0u8; 8];
    let bytes = password.as_bytes();
    for (i, slot) in key.iter_mut().enumerate() {
        if i < bytes.len() {
            *slot = reverse_bits(bytes[i]);
        }
    }
    key
}

fn vnc_auth_response(password: &str, challenge: &[u8; 16]) -> [u8; 16] {
    use des::cipher::generic_array::GenericArray;
    use des::cipher::{BlockEncrypt, KeyInit};

    let key_bytes = vnc_des_key(password);
    let cipher = des::Des::new(GenericArray::from_slice(&key_bytes));
    let mut response = *challenge;
    let b1 = GenericArray::from_mut_slice(&mut response[0..8]);
    cipher.encrypt_block(b1);
    let b2 = GenericArray::from_mut_slice(&mut response[8..16]);
    cipher.encrypt_block(b2);
    response
}
