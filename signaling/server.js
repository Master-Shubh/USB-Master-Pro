/**
 * USB Master Pro — P2P Signaling Broker & Rendezvous Server
 * Provides AnyDesk-style 9-digit ID discovery, WebRTC SDP negotiation,
 * and ICE candidate exchange over standard WSS/HTTPS port 443 (or dev 8443).
 */

const http = require('http');
const WebSocket = require('ws');

const PORT = process.env.PORT || 8443;
const server = http.createServer((req, res) => {
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, OPTIONS');
  res.setHeader('Content-Type', 'application/json');

  if (req.url === '/health') {
    res.writeHead(200);
    res.end(JSON.stringify({ status: 'ok', service: 'USB Master Pro Signaling Broker', version: '1.0.0' }));
    return;
  }

  if (req.url === '/stats') {
    res.writeHead(200);
    res.end(JSON.stringify({
      activeNodes: peersById.size,
      activeChannels: activeChannels.size,
      uptime: process.uptime()
    }));
    return;
  }

  res.writeHead(404);
  res.end(JSON.stringify({ error: 'Endpoint not found' }));
});

const wss = new WebSocket.Server({ server });

// In-Memory Peer Registry (Mapped to 9-Digit ID)
const peersById = new Map();       // idString -> { ws, info, registeredAt, lastPing }
const peersByWs = new Map();       // ws -> idString
const activeChannels = new Map();  // channelId -> { senderId, receiverId, deviceId }

console.log(`[USB Master Pro Broker] Initializing P2P Rendezvous Fabric on port ${PORT}...`);

wss.on('connection', (ws, req) => {
  const remoteIp = req.socket.remoteAddress;
  console.log(`[Broker] New peer connection from ${remoteIp}`);

  ws.on('message', (data) => {
    try {
      const msg = JSON.parse(data.toString());
      handleMessage(ws, msg);
    } catch (err) {
      console.error('[Broker] Malformed message received:', err.message);
      ws.send(JSON.stringify({ type: 'ERROR', error: 'Malformed JSON payload' }));
    }
  });

  ws.on('close', () => {
    const peerId = peersByWs.get(ws);
    if (peerId) {
      console.log(`[Broker] Peer disconnected: ${peerId}`);
      peersById.delete(peerId);
      peersByWs.delete(ws);

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
    }
  });

  ws.on('error', (err) => {
    console.error(`[Broker] Socket error from ${remoteIp}:`, err.message);
  });
});

function handleMessage(ws, msg) {
  switch (msg.type) {
    case 'REGISTER': {
      // msg: { type: 'REGISTER', id: '482190334', info: { hostname, os, devices, version } }
      const peerId = String(msg.id).replace(/\s+/g, '');
      if (!peerId || peerId.length !== 9) {
        ws.send(JSON.stringify({ type: 'REGISTER_RESP', success: false, error: 'Invalid 9-digit ID format' }));
        return;
      }

      peersById.set(peerId, {
        ws,
        info: msg.info || {},
        registeredAt: Date.now(),
        lastPing: Date.now()
      });
      peersByWs.set(ws, peerId);

      console.log(`[Broker] Node registered: ID [${peerId}] (${msg.info?.hostname || 'Unknown Host'})`);
      ws.send(JSON.stringify({
        type: 'REGISTER_RESP',
        success: true,
        id: peerId,
        serverTime: Date.now()
      }));
      break;
    }

    case 'DEVICE_CATALOG_UPDATE': {
      // msg: { type: 'DEVICE_CATALOG_UPDATE', devices: [...] }
      const peerId = peersByWs.get(ws);
      if (!peerId) return;
      const peer = peersById.get(peerId);
      if (peer) {
        peer.info.devices = msg.devices || [];
        console.log(`[Broker] Node [${peerId}] updated device catalog (${peer.info.devices.length} devices)`);
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
          info: {
            hostname: target.info.hostname,
            os: target.info.os,
            devices: target.info.devices || [],
            version: target.info.version
          }
        }));
      }
      break;
    }

    case 'CONNECT_REQUEST': {
      // Machine A requests to mount a device from Machine B
      // msg: { type: 'CONNECT_REQUEST', fromId, targetId, deviceId, deviceName, pin, sdpOffer }
      const fromId = peersByWs.get(ws);
      const targetId = String(msg.targetId).replace(/\s+/g, '');
      const target = peersById.get(targetId);

      if (!target || target.ws.readyState !== WebSocket.OPEN) {
        ws.send(JSON.stringify({
          type: 'CONNECT_RESPONSE',
          success: false,
          error: `Remote computer ${targetId} is offline`
        }));
        return;
      }

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
      const toId = String(msg.toId).replace(/\s+/g, '');
      const client = peersById.get(toId);

      if (!client || client.ws.readyState !== WebSocket.OPEN) {
        console.warn(`[Broker] Cannot route answer; requesting peer [${toId}] disconnected`);
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

server.listen(PORT, '0.0.0.0', () => {
  console.log(`[USB Master Pro Broker] Live on http://0.0.0.0:${PORT}`);
  console.log(`[USB Master Pro Broker] Health check: http://0.0.0.0:${PORT}/health`);
});
