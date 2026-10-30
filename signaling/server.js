/**
 * USB Master Pro — P2P Signaling Broker & Rendezvous Server
 * Provides AnyDesk-style 9-digit ID discovery, WebRTC SDP negotiation,
 * and ICE candidate exchange over standard WSS/HTTPS port 443 (or dev 8443).
 */

const http = require('http');
const crypto = require('crypto');
const WebSocket = require('ws');

const PORT = process.env.PORT || 8443;

// In-Memory Peer Registry (Mapped to 9-Digit ID)
const peersById = new Map();              // idString -> { ws, publicKey, verified, info, registeredAt, lastPing }
const peersByWs = new Map();              // ws -> idString
const verifiedPeers = new Map();          // idString -> { ws, publicKey, info, registeredAt, lastPing }
const pendingChallenges = new Map();      // idString -> { nonce, publicKey, timestamp, timer }
const pendingConnectRequests = new Map(); // reqKey ("fromId:targetId") -> { timestamp, fromId, targetId }
const connectionsByIp = new Map();        // ip -> Set<ws>
const activeChannels = new Map();         // channelId -> { senderId, receiverId, deviceId }

const server = http.createServer((req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');
  res.setHeader('Content-Type', 'application/json');

  if (req.method === 'OPTIONS') {
    res.writeHead(204);
    res.end();
    return;
  }

  if (req.url === '/health') {
    res.writeHead(200);
    res.end(JSON.stringify({ status: 'ok', service: 'USB Master Pro Signaling Broker', version: '1.0.0' }));
    return;
  }

  if (req.url === '/stats') {
    res.writeHead(200);
    res.end(JSON.stringify({
      activeNodes: peersById.size,
      verifiedNodes: verifiedPeers.size,
      activeChannels: activeChannels.size,
      uptime: process.uptime()
    }));
    return;
  }

  // Section 9.1: GET /v1/installations/:id -> returns verified public metadata
  const installMatch = req.url.match(/^\/v1\/installations\/([a-zA-Z0-9\s_-]+)$/);
  if (installMatch && req.method === 'GET') {
    const rawId = installMatch[1];
    const cleanId = rawId.replace(/\s+/g, '');
    const peer = peersById.get(cleanId);
    if (!peer) {
      res.writeHead(404);
      res.end(JSON.stringify({ error: 'Installation not found or offline', id: cleanId }));
      return;
    }
    const isVerified = verifiedPeers.has(cleanId);
    const formattedId = cleanId.length === 9
      ? `${cleanId.slice(0, 3)} ${cleanId.slice(3, 6)} ${cleanId.slice(6, 9)}`
      : cleanId;

    res.writeHead(200);
    res.end(JSON.stringify({
      id: cleanId,
      formattedId,
      verified: isVerified,
      publicKey: peer.publicKey || null,
      version: peer.info?.version || '1.0.0',
      online: peer.ws?.readyState === WebSocket.OPEN,
      registeredAt: peer.registeredAt,
      lastPing: peer.lastPing
    }));
    return;
  }

  // Section 9.1: GET /v1/presence -> returns aggregate online and verified peer counts
  if ((req.url === '/v1/presence' || req.url === '/v1/presence/') && req.method === 'GET') {
    let totalOnline = 0;
    let verifiedCount = 0;
    for (const [id, peer] of peersById.entries()) {
      if (peer.ws && peer.ws.readyState === WebSocket.OPEN) {
        totalOnline++;
        if (verifiedPeers.has(id)) {
          verifiedCount++;
        }
      }
    }
    res.writeHead(200);
    res.end(JSON.stringify({
      totalOnline,
      verifiedCount,
      timestamp: Date.now()
    }));
    return;
  }

  res.writeHead(404);
  res.end(JSON.stringify({ error: 'Endpoint not found' }));
});

const wss = new WebSocket.Server({ server, maxPayload: 1024 * 1024 }); // 1MB frame limit for DoS prevention

console.log(`[USB Master Pro Broker] Initializing P2P Rendezvous Fabric on port ${PORT}...`);

wss.on('connection', (ws, req) => {
  const remoteIp = req.socket.remoteAddress || 'unknown';

  // DoS Hardening: Max 10 concurrent WebSocket connections per remote IP
  const ipConns = connectionsByIp.get(remoteIp) || new Set();
  if (ipConns.size >= 10) {
    console.warn(`[Broker] Connection limit exceeded for IP: ${remoteIp}`);
    ws.close(1008, 'Connection limit exceeded (max 10 connections per IP)');
    return;
  }
  ipConns.add(ws);
  connectionsByIp.set(remoteIp, ipConns);

  console.log(`[Broker] New peer connection from ${remoteIp}`);

  // Rate Limiting: Max 60 messages per 10s per socket
  const rateLimit = {
    count: 0,
    resetTime: Date.now() + 10000
  };

  ws.on('message', (data) => {
    const now = Date.now();
    if (now > rateLimit.resetTime) {
      rateLimit.count = 0;
      rateLimit.resetTime = now + 10000;
    }
    rateLimit.count++;
    if (rateLimit.count > 60) {
      console.warn(`[Broker] Rate limit exceeded for socket from ${remoteIp}`);
      ws.send(JSON.stringify({ type: 'ERROR', error: 'Rate limit exceeded: max 60 messages per 10s' }));
      return;
    }

    try {
      const msg = JSON.parse(data.toString());
      handleMessage(ws, msg);
    } catch (err) {
      console.error('[Broker] Malformed message received:', err.message);
      ws.send(JSON.stringify({ type: 'ERROR', error: 'Malformed JSON payload' }));
    }
  });

  ws.on('close', () => {
    // Release IP connection tracking
    const conns = connectionsByIp.get(remoteIp);
    if (conns) {
      conns.delete(ws);
      if (conns.size === 0) {
        connectionsByIp.delete(remoteIp);
      }
    }

    const peerId = peersByWs.get(ws);
    // Always clean up the ws->peerId mapping, regardless of ownership
    peersByWs.delete(ws);

    if (peerId) {
      const currentRecord = peersById.get(peerId);
      // Only clean up global registry if THIS socket is still the active registration
      if (currentRecord && currentRecord.ws === ws) {
        console.log(`[Broker] Active peer disconnected: ${peerId}`);
        peersById.delete(peerId);
        verifiedPeers.delete(peerId);
        if (pendingChallenges.has(peerId)) {
          const c = pendingChallenges.get(peerId);
          if (c.timer) clearTimeout(c.timer);
          pendingChallenges.delete(peerId);
        }
        // Clean up any pending connect requests involving this peer
        for (const [key, reqObj] of pendingConnectRequests.entries()) {
          if (reqObj.fromId === peerId || reqObj.targetId === peerId) {
            pendingConnectRequests.delete(key);
          }
        }

        // Clean up active channels and notify peers
        for (const [chanId, chan] of activeChannels.entries()) {
          if (chan.senderId === peerId || chan.receiverId === peerId) {
            const otherId = chan.senderId === peerId ? chan.receiverId : chan.senderId;
            const otherPeer = peersById.get(otherId);
            if (otherPeer && otherPeer.ws.readyState === WebSocket.OPEN) {
              otherPeer.ws.send(JSON.stringify({
                type: 'PEER_DISCONNECTED',
                peerId,
                channelId: chanId,
                reason: 'Remote node disconnected abruptly'
              }));
            }
            activeChannels.delete(chanId);
          }
        }
      } else {
        console.log(`[Broker] Stale socket closed for peer [${peerId}]; active registration preserved.`);
      }
    }
  });

  ws.on('error', (err) => {
    console.error(`[Broker] Socket error from ${remoteIp}:`, err.message);
  });
});

/**
 * Cryptographic Ed25519 signature verifier.
 * Strictly verifies RFC 8032 Ed25519 signatures using standard Node.js crypto.verify.
 * The custom SHA-256 fallback bypass is deleted.
 */
function verifyEd25519Proof(publicKeyHex, messageStr, signatureHex) {
  try {
    if (!publicKeyHex || !signatureHex) return false;
    if (signatureHex.length !== 128 || publicKeyHex.length !== 64) return false;

    const rawPubKey = Buffer.from(publicKeyHex, 'hex');
    const spkiKey = crypto.createPublicKey({
      key: Buffer.concat([
        Buffer.from('302a300506032b6570032100', 'hex'),
        rawPubKey
      ]),
      format: 'der',
      type: 'spki'
    });
    return crypto.verify(null, Buffer.from(messageStr, 'utf8'), spkiKey, Buffer.from(signatureHex, 'hex'));
  } catch (_) {
    return false;
  }
}

function handleMessage(ws, msg) {
  switch (msg.type) {
    case 'REGISTER_INIT': {
      // msg: { type: 'REGISTER_INIT', id: '482190334', publicKey: '<64-hex>' }
      const rawId = msg.id != null ? String(msg.id) : '';
      const peerId = rawId.replace(/\s+/g, '');
      if (!peerId || peerId.length !== 9 || !/^\d{9}$/.test(peerId)) {
        ws.send(JSON.stringify({ type: 'REGISTER_RESP', success: false, error: 'Invalid 9-digit ID format' }));
        return;
      }
      if (!msg.publicKey || typeof msg.publicKey !== 'string' || msg.publicKey.length !== 64) {
        ws.send(JSON.stringify({ type: 'REGISTER_RESP', success: false, error: 'Invalid 32-byte Ed25519 public key hex' }));
        return;
      }

      // Hijacking prevention: if already verified by another active socket with different key
      if (verifiedPeers.has(peerId)) {
        const existing = verifiedPeers.get(peerId);
        if (existing.ws !== ws && existing.ws.readyState === WebSocket.OPEN && existing.publicKey !== msg.publicKey.toLowerCase()) {
          console.warn(`[Broker] REGISTER_INIT rejected: ID [${peerId}] is cryptographically owned by another active node.`);
          ws.send(JSON.stringify({
            type: 'REGISTER_RESP',
            success: false,
            error: 'Peer ID is already claimed and verified by another installation'
          }));
          return;
        }
      }

      if (pendingChallenges.has(peerId)) {
        const prior = pendingChallenges.get(peerId);
        if (prior.timer) clearTimeout(prior.timer);
      }

      const nonce = crypto.randomBytes(32).toString('hex');
      const timestamp = Date.now();

      const timer = setTimeout(() => {
        if (pendingChallenges.get(peerId)?.nonce === nonce) {
          pendingChallenges.delete(peerId);
        }
      }, 10000); // 10s TTL

      pendingChallenges.set(peerId, {
        nonce,
        publicKey: msg.publicKey.toLowerCase(),
        timestamp,
        timer
      });

      console.log(`[Broker] Challenge issued for node [${peerId}]`);
      ws.send(JSON.stringify({
        type: 'REGISTER_CHALLENGE',
        id: peerId,
        nonce,
        timestamp
      }));
      break;
    }

    case 'REGISTER_PROOF': {
      // msg: { type: 'REGISTER_PROOF', id: '482190334', signature: '<128-hex>', info: { ... } }
      const rawId = msg.id != null ? String(msg.id) : '';
      const peerId = rawId.replace(/\s+/g, '');
      if (!peerId || peerId.length !== 9 || !/^\d{9}$/.test(peerId)) {
        ws.send(JSON.stringify({ type: 'REGISTER_RESP', success: false, error: 'Invalid 9-digit ID format' }));
        return;
      }

      const challenge = pendingChallenges.get(peerId);
      if (!challenge) {
        ws.send(JSON.stringify({
          type: 'REGISTER_RESP',
          success: false,
          error: 'No active registration challenge found or challenge expired (10s TTL)'
        }));
        return;
      }

      // Single-use challenge deletion: immediately clear timer and delete challenge
      if (challenge.timer) clearTimeout(challenge.timer);
      pendingChallenges.delete(peerId);

      if (Date.now() - challenge.timestamp > 10000) {
        ws.send(JSON.stringify({
          type: 'REGISTER_RESP',
          success: false,
          error: 'Registration challenge expired'
        }));
        return;
      }

      // Enforce exact challenge-key binding
      if (msg.publicKey && msg.publicKey.toLowerCase() !== challenge.publicKey) {
        ws.send(JSON.stringify({
          type: 'REGISTER_RESP',
          success: false,
          error: 'Public key mismatch: does not match challenge key'
        }));
        return;
      }

      const publicKey = challenge.publicKey;
      const signatureHex = (msg.signature || '').toLowerCase();
      const formattedId = `${peerId.slice(0, 3)} ${peerId.slice(3, 6)} ${peerId.slice(6, 9)}`;

      const valid = verifyEd25519Proof(publicKey, `USB_MASTER_REGISTER:${peerId}:${challenge.nonce}`, signatureHex)
        || verifyEd25519Proof(publicKey, `USB_MASTER_REGISTER:${formattedId}:${challenge.nonce}`, signatureHex)
        || (rawId !== peerId && verifyEd25519Proof(publicKey, `USB_MASTER_REGISTER:${rawId}:${challenge.nonce}`, signatureHex));

      if (!valid) {
        console.warn(`[Broker] Cryptographic proof FAILED for node [${peerId}]`);
        ws.send(JSON.stringify({
          type: 'REGISTER_RESP',
          success: false,
          error: 'Cryptographic proof verification failed. Signature mismatch or unauthorized key.'
        }));
        return;
      }

      const peerRecord = {
        ws,
        publicKey,
        verified: true,
        info: msg.info || {},
        registeredAt: Date.now(),
        lastPing: Date.now()
      };

      verifiedPeers.set(peerId, peerRecord);
      peersById.set(peerId, peerRecord);
      peersByWs.set(ws, peerId);

      console.log(`[Broker] Node cryptographically authenticated: ID [${peerId}] (${msg.info?.hostname || 'Unknown'})`);
      ws.send(JSON.stringify({
        type: 'REGISTER_RESP',
        success: true,
        id: peerId,
        verified: true,
        serverTime: Date.now()
      }));
      break;
    }

    case 'REGISTER': {
      // Legacy unauthenticated registration fallback (Gated by ALLOW_LEGACY_REGISTER environment flag)
      if (process.env.ALLOW_LEGACY_REGISTER !== 'true') {
        console.warn(`[Broker] Rejected unauthenticated legacy REGISTER attempt. Unauthenticated registration is disabled for security.`);
        ws.send(JSON.stringify({
          type: 'REGISTER_RESP',
          success: false,
          error: 'Unauthenticated legacy registration is disabled. Upgrade client to REGISTER_INIT cryptographic challenge-response.'
        }));
        return;
      }

      // msg: { type: 'REGISTER', id: '482190334', info: { hostname, os, devices, version } }
      const peerId = String(msg.id).replace(/\s+/g, '');
      if (!peerId || peerId.length !== 9) {
        ws.send(JSON.stringify({ type: 'REGISTER_RESP', success: false, error: 'Invalid 9-digit ID format' }));
        return;
      }

      // Hijack prevention: reject unauthenticated claim if peerId is currently held by active verified session
      if (verifiedPeers.has(peerId)) {
        const active = verifiedPeers.get(peerId);
        if (active.ws !== ws && active.ws.readyState === WebSocket.OPEN) {
          console.warn(`[Broker] Unauthenticated REGISTER rejected for verified node [${peerId}].`);
          ws.send(JSON.stringify({
            type: 'REGISTER_RESP',
            success: false,
            error: 'Peer ID is cryptographically secured by an active verified session'
          }));
          return;
        }
      }

      console.warn(`[Broker] WARNING: Node [${peerId}] registered via unauthenticated legacy REGISTER protocol. Upgrade to REGISTER_INIT challenge-response.`);

      peersById.set(peerId, {
        ws,
        info: msg.info || {},
        verified: false,
        registeredAt: Date.now(),
        lastPing: Date.now()
      });
      peersByWs.set(ws, peerId);

      console.log(`[Broker] Node registered (legacy): ID [${peerId}] (${msg.info?.hostname || 'Unknown Host'})`);
      ws.send(JSON.stringify({
        type: 'REGISTER_RESP',
        success: true,
        id: peerId,
        warning: 'Legacy registration deprecated; use challenge-response protocol',
        serverTime: Date.now()
      }));
      break;
    }

    case 'DEVICE_CATALOG_UPDATE': {
      // msg: { type: 'DEVICE_CATALOG_UPDATE', devices: [...] }
      const peerId = peersByWs.get(ws);
      if (!peerId) {
        ws.send(JSON.stringify({ type: 'ERROR', error: 'Unregistered peer cannot update device catalog' }));
        return;
      }
      if (!Array.isArray(msg.devices) || msg.devices.length > 32) {
        ws.send(JSON.stringify({
          type: 'ERROR',
          error: 'Invalid device catalog: must be an array with at most 32 devices'
        }));
        return;
      }
      const peer = peersById.get(peerId);
      if (peer) {
        const sanitized = msg.devices.map(dev => {
          if (!dev || typeof dev !== 'object') return null;
          const clean = {};
          for (const [k, v] of Object.entries(dev)) {
            if (typeof v === 'string') {
              clean[k] = v.slice(0, 256).replace(/[\x00-\x1f\x7f]/g, '');
            } else if (typeof v === 'number' || typeof v === 'boolean') {
              clean[k] = v;
            } else if (v === null) {
              clean[k] = null;
            }
          }
          return clean;
        }).filter(Boolean);

        peer.info = peer.info || {};
        peer.info.devices = sanitized;
        console.log(`[Broker] Node [${peerId}] updated device catalog (${sanitized.length} devices)`);
      }
      break;
    }

    case 'QUERY_PEER': {
      // msg: { type: 'QUERY_PEER', targetId: '712883910' }
      const targetId = String(msg.targetId).replace(/\s+/g, '');
      const target = peersById.get(targetId);
      if (!target || target.ws.readyState !== WebSocket.OPEN) {
        ws.send(JSON.stringify({
          type: 'QUERY_PEER_RESP',
          found: false,
          targetId,
          error: 'Remote computer is offline or ID does not exist'
        }));
      } else {
        ws.send(JSON.stringify({
          type: 'QUERY_PEER_RESP',
          found: true,
          targetId,
          version: target.info?.version || '1.0.0'
        }));
      }
      break;
    }

    case 'CONNECT_REQUEST': {
      // Machine A requests to mount a device from Machine B
      // msg: { type: 'CONNECT_REQUEST', fromId, targetId, deviceId, deviceName, pin, sdpOffer }
      const fromId = peersByWs.get(ws);
      if (!fromId) {
        ws.send(JSON.stringify({ type: 'ERROR', error: 'Unregistered peer cannot initiate connections' }));
        return;
      }
      const targetId = String(msg.targetId || '').replace(/\s+/g, '');
      const target = peersById.get(targetId);

      if (!target || target.ws.readyState !== WebSocket.OPEN) {
        ws.send(JSON.stringify({
          type: 'CONNECT_RESPONSE',
          success: false,
          error: `Remote computer ${targetId} is offline`
        }));
        return;
      }

      // Track pending connect request (30s TTL)
      const reqKey = `${fromId}:${targetId}`;
      pendingConnectRequests.set(reqKey, {
        timestamp: Date.now(),
        fromId,
        targetId
      });

      console.log(`[Broker] Forwarding connect request: [${fromId}] -> [${targetId}] for device ${msg.deviceName}`);
      target.ws.send(JSON.stringify({
        type: 'INCOMING_CONNECTION',
        fromId,
        fromHostname: peersById.get(fromId)?.info?.hostname || 'Remote Host',
        deviceId: msg.deviceId,
        deviceName: msg.deviceName,
        pin: msg.pin,
        sdpOffer: msg.sdpOffer
      }));
      break;
    }

    case 'CONNECT_RESPONSE': {
      // Machine B accepts or rejects Machine A's request
      // msg: { type: 'CONNECT_RESPONSE', toId, accepted: true/false, reason, sdpAnswer }
      const fromId = peersByWs.get(ws);
      if (!fromId) {
        ws.send(JSON.stringify({ type: 'ERROR', error: 'Unregistered peer cannot respond to connections' }));
        return;
      }
      const toId = String(msg.toId || '').replace(/\s+/g, '');

      // Validate matching unexpired pending request exists from toId to fromId
      const reqKey = `${toId}:${fromId}`;
      const pending = pendingConnectRequests.get(reqKey);

      if (!pending || (Date.now() - pending.timestamp > 30000)) {
        if (pending) pendingConnectRequests.delete(reqKey);
        console.warn(`[Broker] Unsolicited or expired CONNECT_RESPONSE rejected from [${fromId}] to [${toId}]`);
        ws.send(JSON.stringify({
          type: 'CONNECT_RESPONSE_ERROR',
          error: 'Unsolicited, replayed, or expired connection response'
        }));
        return;
      }

      // Single-use: delete pending request immediately once handled
      pendingConnectRequests.delete(reqKey);

      const client = peersById.get(toId);
      if (!client || client.ws.readyState !== WebSocket.OPEN) {
        console.warn(`[Broker] Cannot route answer; requesting peer [${toId}] disconnected`);
        ws.send(JSON.stringify({
          type: 'CONNECT_RESPONSE_ERROR',
          error: `Requesting peer [${toId}] disconnected`
        }));
        return;
      }

      console.log(`[Broker] Routing connection decision: [${fromId}] -> [${toId}]: Accepted=${msg.accepted}`);
      client.ws.send(JSON.stringify({
        type: 'CONNECT_DECISION',
        fromId,
        accepted: msg.accepted,
        reason: msg.reason,
        sdpAnswer: msg.sdpAnswer
      }));
      break;
    }

    case 'ICE_CANDIDATE': {
      // Relay STUN/TURN ICE candidate
      // msg: { type: 'ICE_CANDIDATE', targetId, candidate }
      const fromId = peersByWs.get(ws);
      if (!fromId || !verifiedPeers.has(fromId)) {
        ws.send(JSON.stringify({ type: 'ERROR', error: 'Unregistered socket cannot relay ICE candidates' }));
        return;
      }

      // Candidate size guard
      const candidateStr = typeof msg.candidate === 'string' ? msg.candidate : JSON.stringify(msg.candidate || '');
      if (candidateStr.length > 4096) {
        ws.send(JSON.stringify({ type: 'ERROR', error: 'ICE candidate payload too large' }));
        return;
      }

      const targetId = String(msg.targetId).replace(/\s+/g, '');
      const target = peersById.get(targetId);

      if (target && target.ws.readyState === WebSocket.OPEN) {
        target.ws.send(JSON.stringify({
          type: 'ICE_CANDIDATE',
          fromId,
          candidate: msg.candidate
        }));
      }
      break;
    }

    case 'PING': {
      const peerId = peersByWs.get(ws);
      if (peerId) {
        const peer = peersById.get(peerId);
        if (peer) peer.lastPing = Date.now();
      }
      ws.send(JSON.stringify({ type: 'PONG', timestamp: Date.now() }));
      break;
    }

    default:
      console.warn(`[Broker] Unhandled message type: ${msg.type}`);
  }
}

if (require.main === module) {
  server.listen(PORT, '0.0.0.0', () => {
    console.log(`[USB Master Pro Broker] Live on http://0.0.0.0:${PORT}`);
    console.log(`[USB Master Pro Broker] Health check: http://0.0.0.0:${PORT}/health`);
  });
}

module.exports = {
  server,
  wss,
  verifyEd25519Proof,
  peersById,
  verifiedPeers,
  pendingChallenges,
  pendingConnectRequests,
  connectionsByIp
};
