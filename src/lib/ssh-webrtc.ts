import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { api } from "./tauri";

interface SshWebrtcConn {
  pc: RTCPeerConnection;
  channel: RTCDataChannel | null;
  unlistenIce: UnlistenFn | null;
  decoder: TextDecoder;
  cancelled: boolean;
}

const connections = new Map<string, SshWebrtcConn>();
const encoder = new TextEncoder();

export interface SshWebrtcHandlers {
  onData: (text: string) => void;
  onOpen?: () => void;
  onClose?: () => void;
}

/**
 * Negotiate a data-only WebRTC peer for an SSH terminal session. PTY bytes flow
 * both ways over a single "terminal" DataChannel. Throws if the backend has no
 * peer registered for the session (local terminals / legacy poll sessions), so
 * callers can fall back to the command/poll transport.
 */
export async function connectSshWebrtc(
  sessionId: string,
  handlers: SshWebrtcHandlers,
): Promise<void> {
  if (connections.has(sessionId)) {
    return;
  }

  const pc = new RTCPeerConnection({
    iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
  });
  const conn: SshWebrtcConn = {
    pc,
    channel: null,
    unlistenIce: null,
    decoder: new TextDecoder(),
    cancelled: false,
  };
  connections.set(sessionId, conn);

  pc.ondatachannel = (ev) => {
    if (ev.channel.label !== "terminal") {
      return;
    }
    conn.channel = ev.channel;
    ev.channel.binaryType = "arraybuffer";
    ev.channel.onmessage = (m) => {
      if (conn.cancelled) return;
      if (m.data instanceof ArrayBuffer) {
        handlers.onData(conn.decoder.decode(new Uint8Array(m.data), { stream: true }));
      } else if (typeof m.data === "string") {
        handlers.onData(m.data);
      }
    };
    ev.channel.onopen = () => {
      if (!conn.cancelled) handlers.onOpen?.();
    };
    ev.channel.onclose = () => {
      if (!conn.cancelled) handlers.onClose?.();
    };
    if (ev.channel.readyState === "open" && !conn.cancelled) {
      handlers.onOpen?.();
    }
  };

  pc.onicecandidate = (ev) => {
    if (ev.candidate) {
      void api.sshWebrtcIce(sessionId, ev.candidate.toJSON()).catch(() => {});
    }
  };

  conn.unlistenIce = await listen<RTCIceCandidateInit>(`ssh-ice-${sessionId}`, (event) => {
    if (conn.cancelled) return;
    pc.addIceCandidate(event.payload).catch(() => {});
  });

  try {
    const offer = await api.sshWebrtcOffer(sessionId);
    if (conn.cancelled) return;
    await pc.setRemoteDescription(offer);
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    await api.sshWebrtcAnswer(sessionId, { sdp: answer.sdp, type: answer.type });
  } catch (err) {
    disconnectSshWebrtc(sessionId);
    handlers.onClose?.();
    throw err;
  }
}

export function sendSshWebrtc(sessionId: string, data: string): boolean {
  const conn = connections.get(sessionId);
  if (!conn || !conn.channel || conn.channel.readyState !== "open") {
    return false;
  }
  try {
    conn.channel.send(encoder.encode(data));
    return true;
  } catch {
    return false;
  }
}

export function disconnectSshWebrtc(sessionId: string): void {
  const conn = connections.get(sessionId);
  if (!conn) {
    return;
  }
  conn.cancelled = true;
  if (conn.unlistenIce) conn.unlistenIce();
  try {
    conn.channel?.close();
  } catch {}
  try {
    conn.pc.close();
  } catch {}
  connections.delete(sessionId);
}
