use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
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
        pc.add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .context("add video track")?;

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
        let mut state = self.encoder.lock().await;
        let (w, h) = (state.width as usize, state.height as usize);
        if bgra.len() != w * h * 4 {
            return Err(anyhow!(
                "bgra frame size mismatch: got {}, expected {}",
                bgra.len(),
                w * h * 4
            ));
        }
        let yuv = bgra_to_i420(bgra, w, h);
        let yuv_buffer = YUVBuffer::from_vec(yuv, w, h);
        state.pts_ms += 33;
        if state.pts_ms - state.last_keyframe_ms >= 5000 {
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
