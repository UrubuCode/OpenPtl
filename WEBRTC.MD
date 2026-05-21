# WebRTC — Substituição do pipeline RDP/VNC

## Objetivo

Trocar o pipeline atual de streaming de tela do RDP e VNC, que hoje envia frames
brutos comprimidos em LZ4 por um protocolo binário próprio (`TVNV`/`TVNC`/`TRDV`)
através de Tauri `Channel<InvokeResponseBody>` e renderiza com **pixi.js** num
`<canvas>`, por uma stack baseada em **WebRTC**:

- **Vídeo**: H.264 codificado no backend, transportado por uma `MediaStreamTrack`
  WebRTC, exibido por um simples `<video>` no React.
- **Inputs (mouse/teclado)**: enviados pelo frontend por um `RTCDataChannel`.
- **Cursor remoto**: enviado pelo backend por outro `RTCDataChannel` separado.
- **Sinalização**: SDP via comandos Tauri (IPC). ICE candidates do frontend via
  comando Tauri; ICE candidates do backend via eventos Tauri (`emit`).

Tudo continua **100% Rust no backend**. Nenhum processo externo (LiveKit, GStreamer,
ffmpeg).

A motivação é qualidade adaptativa em rede real (jitter buffer, congestion control,
keyframe forçado em pacote perdido — tudo que vem de graça com WebRTC), reduzir
banda em conexões remotas e simplificar drasticamente o frontend (some pixi.js,
some parsing manual de protocolo binário, some decodificação de retângulos).

A fase atual é **POC apenas no VNC**, com pipeline antigo preservado atrás de uma
flag `webrtc_enabled`. RDP será portado depois da validação.

---

## Status atual

### ✅ Pronto

| Camada | Arquivo | Estado |
|---|---|---|
| Cargo deps | `src-tauri/Cargo.toml` | `webrtc 0.13`, `openh264 0.9`, `yuv 0.8`, `bytes 1` |
| Módulo Rust | `src-tauri/src/protocols/webrtc_stream.rs` | 298 linhas — peer, encoder, registry, runtime |
| Integração VNC | `src-tauri/src/protocols/vnc.rs` | flag `webrtc_enabled` no `VncSessionOptions`; worker cria peer após Ready |
| Comandos Tauri | `src-tauri/src/lib.rs` | `vnc_webrtc_offer/answer/ice` + `webrtc_enabled` em `vnc_session_start` |
| Bindings TS | `src/lib/tauri.ts` | `vncWebrtcOffer/Answer/Ice`, opção `webrtcEnabled` |
| Componente React | `src/pages/tabs/workspace/vnc-webrtc.tsx` | 288 linhas — `VncWebrtcBlockView` standalone |

### ⏳ Pendente

1. **Integrar `VncWebrtcBlockView` em `workspace-tab-page.tsx`** — escolher entre
   `VncBlockView` (legacy pixi) e `VncWebrtcBlockView` (novo) com base num
   toggle/setting, e passar `webrtcEnabled: true` em `api.vncSessionStart`.
2. **Testar end-to-end no Windows** — primeira build vai demorar 3-5 min compilando
   openh264 do source.
3. **Portar a mesma estrutura pro RDP** — depois de validar VNC.
4. **Parsear cursor pseudo-encoding -239 no `vnc.rs`** — pra cursor remoto custom
   funcionar de fato (gancho do DataChannel já está pronto, mas o VNC atual não
   decodifica cursor).

---

## Como a comunicação funciona

```
┌────────────── Backend Rust ──────────────┐    ┌───── Frontend (Tauri WebView) ─────┐
│                                          │    │                                    │
│  VNC worker thread                       │    │  VncWebrtcBlockView                │
│  ├─ TCP → servidor VNC                   │    │                                    │
│  ├─ Decodifica RFB → image: Vec<u8>      │    │  RTCPeerConnection                 │
│  │                                       │    │  ├─ ontrack → <video srcObject>   │
│  └─ A cada 16ms, se webrtc_enabled:      │    │  ├─ ondatachannel "cursor"        │
│       peer.push_bgra_frame(&image)       │    │  │    → <img> overlay              │
│                                          │    │  └─ createDataChannel "input"     │
│  StreamPeer (webrtc_stream.rs)           │    │       → JSON {events: [...]}       │
│  ├─ openh264 encoder (BGRA → I420 → H264)│    │                                    │
│  ├─ Webrtc-rs RTCPeerConnection          │    │  Inputs do pointer/teclado:        │
│  │   ├─ video_track (H264)               │    │  - pointermove/down/up             │
│  │   ├─ cursor DataChannel (server-init) │    │  - mapeia coord pra servidor       │
│  │   └─ input DataChannel (client-init)  │    │  - DataChannel.send(JSON)          │
│  └─ Registry global: HashMap<id, peer>   │    │                                    │
│                                          │    │  ICE backend→frontend:             │
│  Tauri commands:                         │    │  - listen(`vnc-ice-${id}`)         │
│  - vnc_webrtc_offer(id) → SDP           │    │  - pc.addIceCandidate(payload)    │
│  - vnc_webrtc_answer(id, sdp)            │◄──►│  ICE frontend→backend:             │
│  - vnc_webrtc_ice(id, candidate)         │    │  - api.vncWebrtcIce(id, cand)      │
│                                          │    │                                    │
│  Tauri event emit:                       │    │  Sinalização SDP:                  │
│  - app.emit("vnc-ice-${id}", candidate)  │    │  - api.vncWebrtcOffer(id)          │
│                                          │    │  - pc.setRemoteDescription(offer)  │
│                                          │    │  - createAnswer → setLocal         │
│                                          │    │  - api.vncWebrtcAnswer(id, ans)    │
└──────────────────────────────────────────┘    └────────────────────────────────────┘
```

### Sequência de uma sessão (com `webrtc_enabled: true`)

1. **Frontend** chama `api.vncSessionStart(profileId, ..., { webrtcEnabled: true })`.
2. **Backend** spawna o worker thread VNC. Worker faz handshake RFB.
3. Worker recebe `server_width/server_height`. Cria `StreamPeer::new(w, h, on_ice)`
   via runtime tokio dedicada. Registra `Arc<StreamPeer>` em `registry()` global.
4. Worker emite `VncSessionControlEvent::Ready` pelo control channel.
5. **Frontend** vê `connectStage === "ready"`, monta `RTCPeerConnection`, cria
   DataChannel "input", chama `api.vncWebrtcOffer(sessionId)`.
6. **Backend** retira peer do registry, retorna `peer.create_offer()` (que já chama
   `setLocalDescription`).
7. **Frontend** `setRemoteDescription(offer)` → `createAnswer` → `setLocalDescription`
   → `api.vncWebrtcAnswer(id, answer)`.
8. **Backend** `peer.accept_answer(answer)` (chama `setRemoteDescription`).
9. ICE candidates trocados em paralelo (trickle).
10. Conexão sobe. Backend começa a chamar `peer.push_bgra_frame(&image)` a cada
    emit interval (16ms focused / 110ms unfocused). Encoder H264 produz NALs.
    Webrtc-rs packetiza e envia.
11. Frontend recebe o track via `ontrack`, joga em `<video>`. Renderiza.
12. Inputs do usuário: `<video>.onPointerMove` → mapeia coord para resolução do
    servidor → `inputChannel.send(JSON.stringify({events:[...]}))`.
13. Backend, dentro do worker, tem uma task tokio escutando `input_rx` do peer.
    Parseia JSON → `RdpInputBatch` (VNC reusa o tipo do RDP) → reinjeta no canal
    interno do worker como `VncSessionWorkerMessage::InputBatch`, que roda no
    mesmo path do input vindo de `api.vncInputBatch`.

### O que está preservado / o que muda no `vnc.rs`

- Tudo do handshake RFB, auth, decodificação Raw, input handling → **inalterado**.
- Apenas o **emit** muda: se `webrtc_peer.is_some()`, em vez de chamar
  `build_video_rects_packet` + `video_rects_channel.send(...)`, faz
  `peer.push_bgra_frame(&image)` via `runtime().spawn`.
- Se `webrtc_enabled: false`, comportamento é idêntico ao anterior (pixi via
  Channel binary). Zero regressão na stack legacy.

---

## Decisões técnicas

### Por que H.264 (openh264) e não VP8/VP9?

**Não foi a primeira escolha.** A intenção inicial era VP8 puro Rust, mas:

- `vpx-encode` + `env-libvpx-sys`: precisa libvpx do sistema. Linux/WSL: apt
  install. **Windows MSVC: precisa vcpkg.** Bola de neve.
- `shiguredo_libvpx` (feature `source-build`): compila libvpx do source, mas no
  Windows usa MSYS2 + MinGW + sh + make. Outra bola de neve.
- `openh264` (feature `source` default ativa): **compila openh264 do source via
  `cc` + `nasm-rs`. Zero dependência externa em qualquer plataforma.**

Trade-offs:
- H.264 tem licença Cisco para o binário Cisco oficial, mas openh264 OSS
  source-build não distribui binário Cisco, então a questão de royalties é
  diferente — consulte advogado pra distribuição comercial se for o caso.
- Browsers (incluindo o WebView do Tauri / Edge WebView2) **têm H.264 nativo**.
  Nenhuma mudança no frontend para receber.
- VP8 daria bitrate ~10-15% menor pra mesma qualidade visual em telas. Não vale o
  custo de setup pra POC.

### Por que `openh264 0.9` com feature `source`

A feature `source` (default) faz `openh264-sys2` compilar openh264 do source
junto, usando `cc` (toolchain do sistema) + `nasm-rs` (que baixa nasm
automaticamente se não tiver). **Funciona out-of-the-box em Linux/macOS/Windows
MSVC sem instalar nada manualmente.**

Custo: ~3-5 min na primeira build. Builds incrementais usam cache.

### YUV via crate `yuv` (com SIMD)

A conversão BGRA→I420 inicialmente era um loop manual. Trocado por `yuv` crate
que tem AVX2/SSE2/NEON ativos por default. Esperado 5-10x speedup em 1080p.

### `registry()` e `runtime()` globais

`StreamPeer` é criado dentro do worker thread VNC (que é síncrono), mas precisa
ser acessível pelos comandos Tauri assíncronos (`vnc_webrtc_offer/answer/ice`).
Solução: `OnceLock<Arc<Mutex<HashMap<session_id, Arc<StreamPeer>>>>>` global.

O `runtime()` é um `tokio::runtime::Runtime` multi-thread compartilhado, criado
preguiçosamente. Worker síncrono usa `runtime().block_on(...)` pra criar o peer e
fazer encoding; spawna tasks tokio pra processar inputs vindos do DataChannel.

### `unsafe impl Send/Sync for EncoderState`

`openh264::encoder::Encoder` contém ponteiros raw para a struct C que não são
`Send/Sync` por default. Como o `Encoder` fica permanentemente atrás de um
`tokio::sync::Mutex` (acesso serializado), é seguro marcar manualmente. Mesma
técnica usada com `vpx-encode` antes.

### Trickle ICE bidirecional

- **Frontend → backend**: comando Tauri `vnc_webrtc_ice(id, candidate)`.
- **Backend → frontend**: backend registra `pc.on_ice_candidate(...)` que recebe
  uma closure injetada via `StreamPeer::new(_, _, on_ice)`. A closure chama
  `app_handle.emit(format!("vnc-ice-{id}"), candidate)`. Frontend faz
  `listen("vnc-ice-{id}", ...)` e adiciona via `pc.addIceCandidate(payload)`.

Necessário para que funcione em redes reais (não-localhost).

### DataChannels separados

- `"input"`: criado pelo **frontend** (negociado in-band). Mouse/teclado → JSON.
- `"cursor"`: criado pelo **backend** já em `StreamPeer::new()`. Server-initiated.
  Mensagens JSON `{x, y, hotspotX, hotspotY, dataUrl, visible}` → `<img>` overlay
  absoluto sobre o `<video>`.

Frontend esconde cursor nativo (`cursor: none`) quando o cursor remoto está
visível.

### Resize dinâmico

`StreamPeer::resize(w, h)` recria o encoder H.264 (libopenh264 não suporta resize
in-stream sem reinit) e força próximo keyframe. **Não precisa renegociar SDP** —
H.264 codifica dimensions dentro do SPS de cada keyframe, e o browser refaz
layout do `<video>` automaticamente quando a track muda de resolução.

Hoje o VNC com handshake RFB padrão não muda resolução durante a sessão (a menos
que use pseudo-encoding `DesktopSize`). A API está pronta para quando portarmos
pro RDP, que muda mais.

---

## Arquivos novos / modificados

### Novos

- `src-tauri/src/protocols/webrtc_stream.rs` (298 linhas)
- `src/pages/tabs/workspace/vnc-webrtc.tsx` (288 linhas)
- `WEBRTC.MD` (este arquivo)

### Modificados

- `src-tauri/Cargo.toml` — 4 deps novas (`webrtc`, `openh264`, `yuv`, `bytes`)
- `src-tauri/src/protocols/mod.rs` — `pub mod webrtc_stream;`
- `src-tauri/src/protocols/vnc.rs`:
  - `VncSessionOptions.webrtc_enabled: bool`
  - Worker recebe `worker_tx_for_input: mpsc::Sender<...>` e `app_handle: AppHandle`
  - Após Ready, se `webrtc_enabled`: cria StreamPeer, registra no registry, spawna
    task que consome `input_rx` JSON e reinjeta como `InputBatch`
  - No emit, se peer ativo: `runtime().spawn(peer.push_bgra_frame(image.clone()))`;
    caso contrário, caminho legacy intacto
  - Cleanup: remove peer do registry e fecha quando worker termina
- `src-tauri/src/lib.rs`:
  - `vnc_session_start` aceita `webrtc_enabled: Option<bool>` e `app: AppHandle`
  - Novos comandos: `vnc_webrtc_offer`, `vnc_webrtc_answer`, `vnc_webrtc_ice`
  - Registrados no `invoke_handler`
- `src/lib/tauri.ts`:
  - `vncSessionStart` aceita `webrtcEnabled?: boolean`
  - `vncWebrtcOffer / vncWebrtcAnswer / vncWebrtcIce`

---

## Como testar (quando integrar)

1. Build no Windows:
   ```
   cd C:\Users\danie\Documents\GitHub\TermOpen
   npm run tauri dev
   ```
   Primeira build: ~3-5 min compilando openh264. Subsequentes: cache.

2. Integrar `VncWebrtcBlockView` em `workspace-tab-page.tsx`. Opções:
   - Setting global "WebRTC streaming (beta)" em `pages/sections/settings`.
   - Toggle por bloco no header do `VncBlockView`.
   - Variável de env só pra dev.

   Onde houver `<VncBlockView .../>`, condicional:
   ```tsx
   {block.useWebrtc ? <VncWebrtcBlockView .../> : <VncBlockView .../>}
   ```

3. Passar `webrtcEnabled: true` em `api.vncSessionStart(...)` quando o bloco
   estiver em modo WebRTC.

4. Conectar em um servidor VNC real. Validar:
   - Sessão sobe (control channel emite `Ready`).
   - `<video>` exibe a tela.
   - Mouse/teclado funcionam.
   - Latência subjetiva aceitável (<150ms em rede local).

---

## Limitações conhecidas (pós-mitigação)

- **Cursor custom no VNC**: gancho do DataChannel pronto, mas `vnc.rs` ainda não
  decodifica cursor pseudo-encoding -239. Cursor padrão do SO funciona normal.
- **Resize de tela no VNC**: API `resize()` existe, mas RFB do projeto não
  parseia `DesktopSize`. Pra RDP vai ser usado.
- **Encoding em CPU**: openh264 + yuv crate com SIMD são razoáveis a 1080p30,
  mas 4K vai sofrer. Hardware encoding (Intel QSV / NVENC) seria próximo passo.
- **Audio**: não implementado. RDP/VNC do projeto não têm audio hoje, então
  não-issue.
- **Reconexão WebRTC**: se a conexão peer cair, hoje não tenta restabelecer
  automaticamente. Seria preciso watchdog no `connectionstatechange`.
- **Build no Windows nunca foi validado pelo Claude** — só o módulo isolado no
  Linux. Esperar 1-2 ajustes pequenos na primeira `cargo build` real.

---

## Por que tudo isso vale a pena

Quando estiver completo e validado, vai:

- Sumir `src/pages/tabs/workspace/natives/vnc-stream.ts` (parser binário) e
  `natives/rdp-stream.ts`.
- Reduzir `vnc.tsx` e `rdp.tsx` em ~70% (sem pixi, sem decodificação manual).
- Trocar protocolo binário próprio + LZ4 por padrão da indústria (WebRTC).
- Ganhar congestion control, jitter buffer, FEC e keyframe-on-loss de graça.
- Banda significativamente menor em redes ruins (H.264 vs raw+LZ4).
- Base para multi-viewer no futuro (LiveKit SFU, se um dia fizer sentido).
