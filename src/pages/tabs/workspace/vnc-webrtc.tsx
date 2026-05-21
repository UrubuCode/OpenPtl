import { Monitor, RefreshCw } from "lucide-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { resolveBackendMessage } from "@/functions/backend-message";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type {
  ConnectionProfile,
  RdpInputEvent,
  RdpSessionFocusInput,
} from "@/types/openptl";

import type { VncBlock } from "./types";

export interface VncWebrtcBlockViewProps {
  block: VncBlock;
  active: boolean;
  profiles: ConnectionProfile[];
  onFocus: () => void;
  onProfileChange: (profileId: string) => void;
  onFocusChange: (focus: RdpSessionFocusInput) => void;
  onRetry: () => void;
  onToggleWebrtc: () => void;
}

function cursorMessageToDataUrl(
  rgbaBase64: string,
  width: number,
  height: number,
): string | null {
  if (width <= 0 || height <= 0) return null;
  try {
    const binary = atob(rgbaBase64);
    const bytes = new Uint8ClampedArray(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    if (bytes.length < width * height * 4) return null;
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.putImageData(new ImageData(bytes, width, height), 0, 0);
    return canvas.toDataURL("image/png");
  } catch {
    return null;
  }
}

function cursorOverlayLeft(
  cursor: { x: number; hotspotX: number },
  video: HTMLVideoElement,
): number {
  if (!video.videoWidth) return 0;
  const rect = video.getBoundingClientRect();
  const scale = rect.width / video.videoWidth;
  return (cursor.x - cursor.hotspotX) * scale;
}

function cursorOverlayTop(
  cursor: { y: number; hotspotY: number },
  video: HTMLVideoElement,
): number {
  if (!video.videoHeight) return 0;
  const rect = video.getBoundingClientRect();
  const scale = rect.height / video.videoHeight;
  return (cursor.y - cursor.hotspotY) * scale;
}

export function VncWebrtcBlockView({
  block,
  active,
  profiles,
  onFocus,
  onProfileChange,
  onFocusChange,
  onRetry,
  onToggleWebrtc,
}: VncWebrtcBlockViewProps) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const pcRef = useRef<RTCPeerConnection | null>(null);
  const inputChannelRef = useRef<RTCDataChannel | null>(null);
  const negotiatedRef = useRef<string | null>(null);
  const [cursor, setCursor] = useState<{
    x: number;
    y: number;
    hotspotX: number;
    hotspotY: number;
    dataUrl: string | null;
    visible: boolean;
  }>({ x: 0, y: 0, hotspotX: 0, hotspotY: 0, dataUrl: null, visible: false });

  useEffect(() => {
    const sessionId = block.sessionId;
    if (!sessionId || block.connectStage !== "ready") return;
    if (negotiatedRef.current === sessionId) return;
    negotiatedRef.current = sessionId;

    let cancelled = false;

    const pc = new RTCPeerConnection({
      iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
    });
    pcRef.current = pc;

    pc.addTransceiver("video", { direction: "recvonly" });

    const inputChannel = pc.createDataChannel("input", { ordered: true });
    inputChannelRef.current = inputChannel;

    pc.ondatachannel = (ev) => {
      if (ev.channel.label === "cursor") {
        ev.channel.onmessage = (m) => {
          try {
            const data = JSON.parse(m.data as string) as {
              visible: boolean;
              hotspotX?: number;
              hotspotY?: number;
              width?: number;
              height?: number;
              rgbaBase64?: string;
            };
            if (!data.visible) {
              setCursor((prev) => ({ ...prev, visible: false, dataUrl: null }));
              return;
            }
            const dataUrl =
              data.rgbaBase64 && data.width && data.height
                ? cursorMessageToDataUrl(data.rgbaBase64, data.width, data.height)
                : null;
            setCursor((prev) => ({
              ...prev,
              visible: dataUrl != null,
              hotspotX: data.hotspotX ?? 0,
              hotspotY: data.hotspotY ?? 0,
              dataUrl,
            }));
          } catch {}
        };
      }
    };

    pc.ontrack = (ev) => {
      const [stream] = ev.streams;
      if (videoRef.current && stream) {
        videoRef.current.srcObject = stream;
        void videoRef.current.play().catch(() => {});
      }
    };

    pc.onicecandidate = (ev) => {
      if (ev.candidate) {
        void api
          .vncWebrtcIce(sessionId, ev.candidate.toJSON())
          .catch(() => {});
      }
    };

    let unlistenIce: UnlistenFn | null = null;
    void listen<RTCIceCandidateInit>(`vnc-ice-${sessionId}`, (event) => {
      if (cancelled) return;
      pc.addIceCandidate(event.payload).catch(() => {});
    }).then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlistenIce = fn;
      }
    });

    (async () => {
      try {
        const offer = await api.vncWebrtcOffer(sessionId);
        if (cancelled) return;
        await pc.setRemoteDescription(offer);
        const answer = await pc.createAnswer();
        await pc.setLocalDescription(answer);
        await api.vncWebrtcAnswer(sessionId, {
          sdp: answer.sdp,
          type: answer.type,
        });
      } catch (err) {
        console.error("vnc webrtc negotiation failed", err);
      }
    })();

    return () => {
      cancelled = true;
      if (unlistenIce) unlistenIce();
      try {
        inputChannel.close();
      } catch {}
      try {
        pc.close();
      } catch {}
      pcRef.current = null;
      inputChannelRef.current = null;
      negotiatedRef.current = null;
    };
  }, [block.sessionId, block.connectStage]);

  const sendInputs = useCallback((events: RdpInputEvent[]) => {
    const ch = inputChannelRef.current;
    if (!ch || ch.readyState !== "open") return;
    try {
      ch.send(JSON.stringify({ events }));
    } catch {}
  }, []);

  const surfaceToServer = useCallback(
    (e: ReactPointerEvent<HTMLVideoElement>): { x: number; y: number } => {
      const video = videoRef.current;
      if (!video || !video.videoWidth) return { x: 0, y: 0 };
      const rect = video.getBoundingClientRect();
      const sx = ((e.clientX - rect.left) / rect.width) * video.videoWidth;
      const sy = ((e.clientY - rect.top) / rect.height) * video.videoHeight;
      return {
        x: Math.max(0, Math.min(video.videoWidth - 1, Math.round(sx))),
        y: Math.max(0, Math.min(video.videoHeight - 1, Math.round(sy))),
      };
    },
    [],
  );

  const handlePointerMove = useCallback(
    (e: ReactPointerEvent<HTMLVideoElement>) => {
      const p = surfaceToServer(e);
      setCursor((prev) => (prev.visible ? { ...prev, x: p.x, y: p.y } : prev));
      sendInputs([{ kind: "mouse_move", x: p.x, y: p.y }]);
    },
    [sendInputs, surfaceToServer],
  );

  const buttonOf = (n: number): "left" | "right" | "middle" =>
    n === 2 ? "right" : n === 1 ? "middle" : "left";

  const handlePointerDown = useCallback(
    (e: ReactPointerEvent<HTMLVideoElement>) => {
      onFocus();
      const p = surfaceToServer(e);
      sendInputs([
        { kind: "mouse_button_down", x: p.x, y: p.y, button: buttonOf(e.button) },
      ]);
    },
    [onFocus, sendInputs, surfaceToServer],
  );

  const handlePointerUp = useCallback(
    (e: ReactPointerEvent<HTMLVideoElement>) => {
      const p = surfaceToServer(e);
      sendInputs([
        { kind: "mouse_button_up", x: p.x, y: p.y, button: buttonOf(e.button) },
      ]);
    },
    [sendInputs, surfaceToServer],
  );

  useEffect(() => {
    if (!active) {
      onFocusChange({ focused: false });
    }
  }, [active, onFocusChange]);

  return (
    <div className="flex h-full w-full flex-col bg-black">
      <div className="flex items-center justify-between gap-2 border-b border-border bg-muted/30 px-2 py-1 text-xs">
        <div className="flex items-center gap-2">
          <Monitor className="h-3.5 w-3.5" />
          <select
            className="h-6 max-w-[180px] rounded border border-border/50 bg-background px-1 text-xs text-foreground"
            value={block.profileId}
            onChange={(e) => onProfileChange(e.target.value)}
          >
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} ({p.host}:{p.port})
              </option>
            ))}
          </select>
          <span className="text-muted-foreground">[WebRTC POC]</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onToggleWebrtc}
            className="rounded px-2 py-0.5 text-muted-foreground hover:bg-muted"
          >
            {"legacy"}
          </button>
          {block.connectStage === "error" && (
            <button
              type="button"
              onClick={onRetry}
              className="flex items-center gap-1 rounded px-2 py-0.5 text-muted-foreground hover:bg-muted"
            >
              <RefreshCw className="h-3 w-3" />
              {"retry"}
            </button>
          )}
        </div>
      </div>
      <div
        className={cn(
          "relative flex-1 overflow-hidden",
          block.connectStage !== "ready" && "flex items-center justify-center",
        )}
      >
        {block.connectStage === "ready" ? (
          <>
            <video
              ref={videoRef}
              autoPlay
              muted
              playsInline
              className="h-full w-full"
              style={cursor.visible && cursor.dataUrl ? { cursor: "none" } : undefined}
              onPointerMove={handlePointerMove}
              onPointerDown={handlePointerDown}
              onPointerUp={handlePointerUp}
              onContextMenu={(e) => e.preventDefault()}
            />
            {cursor.visible && cursor.dataUrl && videoRef.current && (
              <img
                src={cursor.dataUrl}
                alt=""
                className="pointer-events-none absolute"
                style={{
                  left: cursorOverlayLeft(cursor, videoRef.current),
                  top: cursorOverlayTop(cursor, videoRef.current),
                }}
              />
            )}
          </>
        ) : (
          <div className="text-xs text-muted-foreground">
            {block.connectStage === "error" && block.connectError
              ? resolveBackendMessage(block.connectError)
              : block.connectStage}
          </div>
        )}
      </div>
    </div>
  );
}
