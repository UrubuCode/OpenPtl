use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use openh264::encoder::{Encoder, EncoderConfig};
use openh264::formats::YUVBuffer;
use openh264::{OpenH264API, Timestamp};
use tokio::sync::{mpsc, Mutex};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

pub struct StreamPeer {
    pc: Arc<RTCPeerConnection>,
    video_track: Arc<TrackLocalStaticSample>,
    encoder: Arc<Mutex<EncoderState>>,
    // Server-initiated cursor DataChannel; driven by push_cursor_update.
    cursor_channel: Arc<RTCDataChannel>,
    // Set when the viewer requests a keyframe (RTCP PLI/FIR). Without honoring
    // this the browser never gets an intra frame after a loss and stays black.
    force_keyframe: Arc<AtomicBool>,
}

struct EncoderState {
    encoder: Encoder,
    width: u32,
    height: u32,
    pts_ms: i64,
    last_keyframe_ms: i64,
}

unsafe impl Send for EncoderState {}
unsafe impl Sync for EncoderState {}

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// webrtc-rs DTLS uses rustls, which needs a process-level CryptoProvider. Install
/// it once before building any peer (idempotent; safe if RDP already installed it).
fn ensure_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}

fn build_encoder(width: u32, height: u32) -> Result<Encoder> {
    let cfg = EncoderConfig::new()
        .bitrate(openh264::encoder::BitRate::from_bps(
            estimate_bitrate_bps(width, height),
        ))
        .max_frame_rate(openh264::encoder::FrameRate::from_hz(30.0));
    Encoder::with_api_config(OpenH264API::from_source(), cfg)
        .map_err(|e| anyhow!("openh264 encoder new: {e:?}"))
}

impl StreamPeer {
    pub async fn new<F>(
        width: u32,
        height: u32,
        on_ice: F,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Vec<u8>>)>
    where
        F: Fn(serde_json::Value) + Send + Sync + 'static,
    {
        ensure_crypto_provider();
        let mut media = MediaEngine::default();
        media.register_default_codecs().context("register codecs")?;
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media)
            .context("register interceptors")?;
        let api = APIBuilder::new()
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .build();

        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pc = Arc::new(api.new_peer_connection(config).await.context("new pc")?);

        let on_ice = Arc::new(on_ice);
        pc.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let on_ice = Arc::clone(&on_ice);
            Box::pin(async move {
                if let Some(c) = candidate {
                    if let Ok(init) = c.to_json() {
                        if let Ok(val) = serde_json::to_value(init) {
                            on_ice(val);
                        }
                    }
                }
            })
        }));

        let video_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                ..Default::default()
            },
            "video".to_owned(),
            "openptl-stream".to_owned(),
        ));
        let rtp_sender = pc
            .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .context("add video track")?;

        let force_keyframe = Arc::new(AtomicBool::new(false));
        {
            let force = Arc::clone(&force_keyframe);
            tokio::spawn(async move {
                use webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
                use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
                while let Ok((packets, _)) = rtp_sender.read_rtcp().await {
                    for packet in packets {
                        let any = packet.as_any();
                        if any.downcast_ref::<PictureLossIndication>().is_some()
                            || any.downcast_ref::<FullIntraRequest>().is_some()
                        {
                            force.store(true, Ordering::Relaxed);
                        }
                    }
                }
            });
        }

        let cursor_channel = pc
            .create_data_channel(
                "cursor",
                Some(RTCDataChannelInit {
                    ordered: Some(true),
                    ..Default::default()
                }),
            )
            .await
            .context("create cursor dc")?;

        let (input_tx, input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let input_tx = Arc::new(input_tx);
        pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let input_tx = Arc::clone(&input_tx);
            Box::pin(async move {
                if dc.label() == "input" {
                    let tx = Arc::clone(&input_tx);
                    dc.on_message(Box::new(move |msg: DataChannelMessage| {
                        let tx = Arc::clone(&tx);
                        Box::pin(async move {
                            let _ = tx.send(msg.data.to_vec());
                        })
                    }));
                }
            })
        }));

        let encoder = build_encoder(width, height)?;
        let state = EncoderState {
            encoder,
            width,
            height,
            pts_ms: 0,
            last_keyframe_ms: 0,
        };

        Ok((
            Self {
                pc,
                video_track,
                encoder: Arc::new(Mutex::new(state)),
                cursor_channel,
                force_keyframe,
            },
            input_rx,
        ))
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription> {
        let offer = self.pc.create_offer(None).await.context("create offer")?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .context("set local")?;
        Ok(offer)
    }

    pub async fn accept_answer(&self, answer: RTCSessionDescription) -> Result<()> {
        self.pc
            .set_remote_description(answer)
            .await
            .context("set remote")?;
        Ok(())
    }

    pub async fn add_remote_ice(&self, candidate: RTCIceCandidateInit) -> Result<()> {
        self.pc
            .add_ice_candidate(candidate)
            .await
            .context("add ice")?;
        Ok(())
    }

    /// True when the viewer asked for a keyframe (RTCP PLI/FIR) but no frame has
    /// been pushed since. The worker uses this to push the current image even
    /// when the desktop is static, so a late-joining viewer isn't stuck black.
    pub fn keyframe_requested(&self) -> bool {
        self.force_keyframe.load(Ordering::Relaxed)
    }

    pub async fn resize(&self, width: u32, height: u32) -> Result<()> {
        let mut state = self.encoder.lock().await;
        if state.width == width && state.height == height {
            return Ok(());
        }
        state.encoder = build_encoder(width, height)?;
        state.width = width;
        state.height = height;
        state.last_keyframe_ms = state.pts_ms;
        Ok(())
    }

    pub async fn push_bgra_frame(&self, bgra: &[u8]) -> Result<()> {
        self.push_frame(bgra, FramePixelFormat::Bgra).await
    }

    pub async fn push_rgba_frame(&self, rgba: &[u8]) -> Result<()> {
        self.push_frame(rgba, FramePixelFormat::Rgba).await
    }

    async fn push_frame(&self, pixels: &[u8], format: FramePixelFormat) -> Result<()> {
        let mut state = self.encoder.lock().await;
        let (w, h) = (state.width as usize, state.height as usize);
        if pixels.len() != w * h * 4 {
            return Err(anyhow!(
                "frame size mismatch: got {}, expected {}",
                pixels.len(),
                w * h * 4
            ));
        }
        let yuv = match format {
            FramePixelFormat::Bgra => bgra_to_i420(pixels, w, h),
            FramePixelFormat::Rgba => rgba_to_i420(pixels, w, h),
        };
        let yuv_buffer = YUVBuffer::from_vec(yuv, w, h);
        state.pts_ms += 33;
        let requested = self.force_keyframe.swap(false, Ordering::Relaxed);
        if requested || state.pts_ms - state.last_keyframe_ms >= 2000 {
            state.encoder.force_intra_frame();
            state.last_keyframe_ms = state.pts_ms;
        }
        let ts = Timestamp::from_millis(state.pts_ms as u64);
        let data = {
            let bitstream = state
                .encoder
                .encode_at(&yuv_buffer, ts)
                .map_err(|e| anyhow!("openh264 encode: {e:?}"))?;
            bitstream.to_vec()
        };
        if !data.is_empty() {
            self.video_track
                .write_sample(&Sample {
                    data: bytes::Bytes::from(data),
                    duration: Duration::from_millis(33),
                    ..Default::default()
                })
                .await
                .context("write sample")?;
        }
        Ok(())
    }

    pub async fn push_cursor_update(&self, json: &str) -> Result<()> {
        if self.cursor_channel.ready_state()
            != webrtc::data_channel::data_channel_state::RTCDataChannelState::Open
        {
            return Ok(());
        }
        self.cursor_channel
            .send_text(json.to_string())
            .await
            .map_err(|e| anyhow!("cursor dc send: {e}"))?;
        Ok(())
    }

    pub async fn close(&self) {
        let _ = self.pc.close().await;
    }
}

type PeerRegistry = Arc<Mutex<HashMap<String, Arc<StreamPeer>>>>;

pub fn registry() -> PeerRegistry {
    static REG: OnceLock<PeerRegistry> = OnceLock::new();
    REG.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

/// Data-only WebRTC peer: no video track, no encoder. A single bidirectional
/// "terminal" DataChannel carries PTY bytes both ways. Used by SSH terminals.
pub struct DataPeer {
    pc: Arc<RTCPeerConnection>,
    term_channel: Arc<RTCDataChannel>,
    // Buffers PTY bytes produced before the channel opens (e.g. the first shell
    // prompt) so nothing is lost; flushed in order once the channel is open.
    pending: Arc<Mutex<Vec<u8>>>,
}

async fn flush_pending(channel: &Arc<RTCDataChannel>, pending: &Arc<Mutex<Vec<u8>>>) {
    if channel.ready_state()
        != webrtc::data_channel::data_channel_state::RTCDataChannelState::Open
    {
        return;
    }
    let data = {
        let mut guard = pending.lock().await;
        if guard.is_empty() {
            return;
        }
        std::mem::take(&mut *guard)
    };
    let _ = channel.send(&bytes::Bytes::from(data)).await;
}

impl DataPeer {
    pub async fn new<F>(on_ice: F) -> Result<(Self, mpsc::UnboundedReceiver<Vec<u8>>)>
    where
        F: Fn(serde_json::Value) + Send + Sync + 'static,
    {
        ensure_crypto_provider();
        let mut media = MediaEngine::default();
        media.register_default_codecs().context("register codecs")?;
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media)
            .context("register interceptors")?;
        let api = APIBuilder::new()
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .build();

        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pc = Arc::new(api.new_peer_connection(config).await.context("new pc")?);

        let on_ice = Arc::new(on_ice);
        pc.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let on_ice = Arc::clone(&on_ice);
            Box::pin(async move {
                if let Some(c) = candidate {
                    if let Ok(init) = c.to_json() {
                        if let Ok(val) = serde_json::to_value(init) {
                            on_ice(val);
                        }
                    }
                }
            })
        }));

        let term_channel = pc
            .create_data_channel(
                "terminal",
                Some(RTCDataChannelInit {
                    ordered: Some(true),
                    ..Default::default()
                }),
            )
            .await
            .context("create terminal dc")?;

        let (input_tx, input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let input_tx = Arc::new(input_tx);
        term_channel.on_message(Box::new(move |msg: DataChannelMessage| {
            let tx = Arc::clone(&input_tx);
            Box::pin(async move {
                let _ = tx.send(msg.data.to_vec());
            })
        }));

        let pending = Arc::new(Mutex::new(Vec::<u8>::new()));
        let flush_channel = Arc::clone(&term_channel);
        let flush_pending_buf = Arc::clone(&pending);
        term_channel.on_open(Box::new(move || {
            let channel = Arc::clone(&flush_channel);
            let pending = Arc::clone(&flush_pending_buf);
            Box::pin(async move {
                flush_pending(&channel, &pending).await;
            })
        }));

        Ok((
            Self {
                pc,
                term_channel,
                pending,
            },
            input_rx,
        ))
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription> {
        let offer = self.pc.create_offer(None).await.context("create offer")?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .context("set local")?;
        Ok(offer)
    }

    pub async fn accept_answer(&self, answer: RTCSessionDescription) -> Result<()> {
        self.pc
            .set_remote_description(answer)
            .await
            .context("set remote")?;
        Ok(())
    }

    pub async fn add_remote_ice(&self, candidate: RTCIceCandidateInit) -> Result<()> {
        self.pc
            .add_ice_candidate(candidate)
            .await
            .context("add ice")?;
        Ok(())
    }

    pub async fn send_terminal(&self, bytes: &[u8]) -> Result<()> {
        {
            self.pending.lock().await.extend_from_slice(bytes);
        }
        flush_pending(&self.term_channel, &self.pending).await;
        Ok(())
    }

    pub async fn close(&self) {
        let _ = self.pc.close().await;
    }
}

type DataPeerRegistry = Arc<Mutex<HashMap<String, Arc<DataPeer>>>>;

pub fn data_registry() -> DataPeerRegistry {
    static REG: OnceLock<DataPeerRegistry> = OnceLock::new();
    REG.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("webrtc-rt")
            .build()
            .expect("build webrtc runtime")
    })
}

fn estimate_bitrate_bps(width: u32, height: u32) -> u32 {
    let pixels = width * height;
    ((pixels as f32 * 0.1) as u32).clamp(500_000, 8_000_000)
}

enum FramePixelFormat {
    Bgra,
    Rgba,
}

fn bgra_to_i420(bgra: &[u8], width: usize, height: usize) -> Vec<u8> {
    use yuv::{
        bgra_to_yuv420, YuvChromaSubsampling, YuvConversionMode, YuvPlanarImageMut, YuvRange,
        YuvStandardMatrix,
    };
    let mut planar = YuvPlanarImageMut::<u8>::alloc(
        width as u32,
        height as u32,
        YuvChromaSubsampling::Yuv420,
    );
    let _ = bgra_to_yuv420(
        &mut planar,
        bgra,
        (width * 4) as u32,
        YuvRange::Limited,
        YuvStandardMatrix::Bt709,
        YuvConversionMode::Balanced,
    );
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut out = Vec::with_capacity(y_size + 2 * uv_size);
    out.extend_from_slice(planar.y_plane.borrow());
    out.extend_from_slice(planar.u_plane.borrow());
    out.extend_from_slice(planar.v_plane.borrow());
    out
}

fn rgba_to_i420(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    use yuv::{
        rgba_to_yuv420, YuvChromaSubsampling, YuvConversionMode, YuvPlanarImageMut, YuvRange,
        YuvStandardMatrix,
    };
    let mut planar = YuvPlanarImageMut::<u8>::alloc(
        width as u32,
        height as u32,
        YuvChromaSubsampling::Yuv420,
    );
    let _ = rgba_to_yuv420(
        &mut planar,
        rgba,
        (width * 4) as u32,
        YuvRange::Limited,
        YuvStandardMatrix::Bt709,
        YuvConversionMode::Balanced,
    );
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut out = Vec::with_capacity(y_size + 2 * uv_size);
    out.extend_from_slice(planar.y_plane.borrow());
    out.extend_from_slice(planar.u_plane.borrow());
    out.extend_from_slice(planar.v_plane.borrow());
    out
}
