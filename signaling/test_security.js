/**
 * USB Master Pro — Signaling Broker Security Regression Test Suite
 *
 * Verifies Step 2 Security Fixes:
 * 1. Forged SHA-256 signature rejection (elimination of SHA-256 bypass backdoor).
 * 2. Valid Ed25519 RFC 8032 signature acceptance, challenge-key binding, and single-use challenge deletion.
 * 3. Unsolicited, expired, or replayed CONNECT_RESPONSE rejection via pendingConnectRequests tracking.
 * 4. Public endpoint sanitization (/v1/installations/:id, /v1/presence, QUERY_PEER).
 * 5. Device catalog array boundary validation (<=32 items, string sanitization).
 */

const http = require('http');
const crypto = require('crypto');
const assert = require('assert');
const WebSocket = require('ws');

// Import server module
const {
  server,
  verifyEd25519Proof,
  peersById,
  verifiedPeers,
  pendingChallenges,
  pendingConnectRequests,
  connectionsByIp
} = require('./server');

const TEST_PORT = 18443;
const BROKER_URL = `ws://127.0.0.1:${TEST_PORT}`;
const HTTP_BASE = `http://127.0.0.1:${TEST_PORT}`;

function createEd25519Key() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');
  const rawPub = publicKey.export({ type: 'spki', format: 'der' }).subarray(12);
  return {
    publicKeyHex: rawPub.toString('hex'),
    privateKey
  };
}

function createForgedSha256Signature(publicKeyHex, messageStr) {
  // Reconstruct old backdoor: s = SHA256('USB_MASTER_ED25519_SIG_RESPONSE_S' + r + pubKey + msg)
  const rPart = crypto.randomBytes(32);
  const pubKeyBuf = Buffer.from(publicKeyHex, 'hex');
  const msgBuf = Buffer.from(messageStr, 'utf8');

  const hash = crypto.createHash('sha256');
  hash.update('USB_MASTER_ED25519_SIG_RESPONSE_S');
  hash.update(rPart);
  hash.update(pubKeyBuf);
  hash.update(msgBuf);
  const sPart = hash.digest();

  return Buffer.concat([rPart, sPart]).toString('hex');
}

function httpGet(path) {
  return new Promise((resolve, reject) => {
    http.get(`${HTTP_BASE}${path}`, (res) => {
      let data = '';
      res.on('data', chunk => { data += chunk; });
      res.on('end', () => {
        try {
          resolve({ status: res.statusCode, body: JSON.parse(data) });
        } catch (e) {
          resolve({ status: res.statusCode, raw: data });
        }
      });
    }).on('error', reject);
  });
}

function connectWs() {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(BROKER_URL);
    ws.on('open', () => resolve(ws));
    ws.on('error', reject);
  });
}

function waitForMessage(ws, filterFn, timeoutMs = 3000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      ws.off('message', handler);
      reject(new Error(`Timeout waiting for message matching filter after ${timeoutMs}ms`));
    }, timeoutMs);

    function handler(data) {
      try {
        const msg = JSON.parse(data.toString());
        if (!filterFn || filterFn(msg)) {
          clearTimeout(timer);
          ws.off('message', handler);
          resolve(msg);
        }
      } catch (err) {}
    }

    ws.on('message', handler);
  });
}

async function runTests() {
  console.log('================================================================================');
  console.log('  🧪 USB Master Pro — Signaling Security Regression Test Suite');
  console.log('================================================================================\n');

  // Start HTTP / WS Server on TEST_PORT
  await new Promise((resolve) => {
    server.listen(TEST_PORT, '127.0.0.1', () => {
      console.log(`[Test Server] Live on ${BROKER_URL}`);
      resolve();
    });
  });

  try {
    // -------------------------------------------------------------------------
    // TEST 1: Forged SHA-256 signature rejection
    // -------------------------------------------------------------------------
    console.log('👉 [Test 1] Forged SHA-256 signature rejection...');
    {
      const keypair = createEd25519Key();
      const testMsg = 'USB_MASTER_REGISTER:123456789:randomnonce1234567890abcdef';
      const forgedSig = createForgedSha256Signature(keypair.publicKeyHex, testMsg);

      // 1. Direct function verification must return false
      const result = verifyEd25519Proof(keypair.publicKeyHex, testMsg, forgedSig);
      assert.strictEqual(result, false, 'verifyEd25519Proof MUST reject forged SHA-256 signature!');

      // 2. Over-the-wire challenge-response flow with forged signature must return failure
      const ws = await connectWs();
      const peerId = '123456789';

      ws.send(JSON.stringify({
        type: 'REGISTER_INIT',
        id: peerId,
        publicKey: keypair.publicKeyHex
      }));

      const challenge = await waitForMessage(ws, m => m.type === 'REGISTER_CHALLENGE');
      assert.strictEqual(challenge.id, peerId);

      const forgedWireSig = createForgedSha256Signature(
        keypair.publicKeyHex,
        `USB_MASTER_REGISTER:${peerId}:${challenge.nonce}`
      );

      ws.send(JSON.stringify({
        type: 'REGISTER_PROOF',
        id: peerId,
        signature: forgedWireSig,
        info: { hostname: 'attacker-host', os: 'Linux', version: '1.0.0' }
      }));

      const resp = await waitForMessage(ws, m => m.type === 'REGISTER_RESP');
      assert.strictEqual(resp.success, false, 'REGISTER_PROOF with forged SHA-256 signature must be rejected!');
      assert(resp.error.includes('Cryptographic proof verification failed'), `Expected proof error, got: ${resp.error}`);

      ws.close();
      console.log('   ✅ Test 1 Passed: Forged SHA-256 signatures are strictly rejected.');
    }

    // -------------------------------------------------------------------------
    // TEST 2: Valid Ed25519 signature acceptance, challenge-key binding, & single-use deletion
    // -------------------------------------------------------------------------
    console.log('\n👉 [Test 2] Valid Ed25519 signature acceptance & challenge-key binding...');
    let registeredWsA;
    let peerA_Id = '111222333';
    let peerA_Key;
    {
      peerA_Key = createEd25519Key();
      const ws = await connectWs();

      // Step A: REGISTER_INIT
      ws.send(JSON.stringify({
        type: 'REGISTER_INIT',
        id: peerA_Id,
        publicKey: peerA_Key.publicKeyHex
      }));

      const challenge = await waitForMessage(ws, m => m.type === 'REGISTER_CHALLENGE');
      assert.strictEqual(challenge.id, peerA_Id);
      assert.ok(challenge.nonce && challenge.nonce.length === 64);

      // Step B: Sign with valid Ed25519 private key
      const messageToSign = `USB_MASTER_REGISTER:${peerA_Id}:${challenge.nonce}`;
      const validSig = crypto.sign(null, Buffer.from(messageToSign, 'utf8'), peerA_Key.privateKey);
      const validSigHex = validSig.toString('hex');

      // Test verifyEd25519Proof directly
      const directVerify = verifyEd25519Proof(peerA_Key.publicKeyHex, messageToSign, validSigHex);
      assert.strictEqual(directVerify, true, 'verifyEd25519Proof must accept valid RFC 8032 Ed25519 signature!');

      // Step C: Send valid REGISTER_PROOF
      ws.send(JSON.stringify({
        type: 'REGISTER_PROOF',
        id: peerA_Id,
        signature: validSigHex,
        publicKey: peerA_Key.publicKeyHex,
        info: {
          hostname: 'node-alpha.internal',
          os: 'Windows 11 x64',
          version: '1.0.0',
          devices: [{ deviceId: 'dev-alpha-01', name: 'Mantra MFS100' }]
        }
      }));

      const resp = await waitForMessage(ws, m => m.type === 'REGISTER_RESP');
      assert.strictEqual(resp.success, true, 'Valid Ed25519 registration must succeed!');
      assert.strictEqual(resp.verified, true);
      assert.strictEqual(resp.id, peerA_Id);

      // Step D: Verify single-use challenge deletion: replaying same proof must fail
      ws.send(JSON.stringify({
        type: 'REGISTER_PROOF',
        id: peerA_Id,
        signature: validSigHex,
        publicKey: peerA_Key.publicKeyHex
      }));

      const replayResp = await waitForMessage(ws, m => m.type === 'REGISTER_RESP');
      assert.strictEqual(replayResp.success, false, 'Challenge replay must fail (single-use deletion)!');
      assert(replayResp.error.includes('No active registration challenge found'), 'Replay must report missing challenge');

      registeredWsA = ws;
      console.log('   ✅ Test 2 Passed: Valid Ed25519 signatures accepted; challenge-key binding & single-use verified.');
    }

    // -------------------------------------------------------------------------
    // TEST 3: Unsolicited & replayed CONNECT_RESPONSE rejection
    // -------------------------------------------------------------------------
    console.log('\n👉 [Test 3] Unsolicited and replayed CONNECT_RESPONSE rejection...');
    let registeredWsB;
    let peerB_Id = '444555666';
    let peerB_Key = createEd25519Key();
    {
      const wsB = await connectWs();

      // Register Peer B
      wsB.send(JSON.stringify({
        type: 'REGISTER_INIT',
        id: peerB_Id,
        publicKey: peerB_Key.publicKeyHex
      }));
      const challengeB = await waitForMessage(wsB, m => m.type === 'REGISTER_CHALLENGE');
      const sigB = crypto.sign(
        null,
        Buffer.from(`USB_MASTER_REGISTER:${peerB_Id}:${challengeB.nonce}`, 'utf8'),
        peerB_Key.privateKey
      ).toString('hex');

      wsB.send(JSON.stringify({
        type: 'REGISTER_PROOF',
        id: peerB_Id,
        signature: sigB,
        publicKey: peerB_Key.publicKeyHex,
        info: { hostname: 'node-beta.internal', os: 'Linux', version: '1.0.0' }
      }));
      const regRespB = await waitForMessage(wsB, m => m.type === 'REGISTER_RESP');
      assert.strictEqual(regRespB.success, true);
      registeredWsB = wsB;

      // 1. Peer B sends UNSOLICITED CONNECT_RESPONSE to Peer A (no CONNECT_REQUEST was issued)
      wsB.send(JSON.stringify({
        type: 'CONNECT_RESPONSE',
        toId: peerA_Id,
        accepted: true,
        reason: 'Unsolicited acceptance attempt'
      }));

      const unsolicitedError = await waitForMessage(wsB, m => m.type === 'CONNECT_RESPONSE_ERROR');
      assert(
        unsolicitedError.error.includes('Unsolicited, replayed, or expired'),
        `Expected unsolicited error, got: ${unsolicitedError.error}`
      );

      // 2. Legitimate CONNECT_REQUEST from Peer A to Peer B
      registeredWsA.send(JSON.stringify({
        type: 'CONNECT_REQUEST',
        targetId: peerB_Id,
        deviceId: 'dev-001',
        deviceName: 'Biometric Scanner',
        sdpOffer: { type: 'offer', sdp: 'v=0...' }
      }));

      const incoming = await waitForMessage(wsB, m => m.type === 'INCOMING_CONNECTION');
      assert.strictEqual(incoming.fromId, peerA_Id);

      // 3. Legitimate CONNECT_RESPONSE from Peer B to Peer A
      wsB.send(JSON.stringify({
        type: 'CONNECT_RESPONSE',
        toId: peerA_Id,
        accepted: true,
        reason: 'Authorized session',
        sdpAnswer: { type: 'answer', sdp: 'v=0...' }
      }));

      const decision = await waitForMessage(registeredWsA, m => m.type === 'CONNECT_DECISION');
      assert.strictEqual(decision.accepted, true);
      assert.strictEqual(decision.fromId, peerB_Id);

      // 4. REPLAY attempt: Peer B sends identical CONNECT_RESPONSE again -> must be rejected
      wsB.send(JSON.stringify({
        type: 'CONNECT_RESPONSE',
        toId: peerA_Id,
        accepted: true,
        reason: 'Replayed answer attempt'
      }));

      const replayedError = await waitForMessage(wsB, m => m.type === 'CONNECT_RESPONSE_ERROR');
      assert(
        replayedError.error.includes('Unsolicited, replayed, or expired'),
        `Replayed CONNECT_RESPONSE must be rejected, got: ${replayedError.error}`
      );

      console.log('   ✅ Test 3 Passed: Unsolicited and replayed CONNECT_RESPONSE strictly rejected.');
    }

    // -------------------------------------------------------------------------
    // TEST 4: Public endpoint sanitization
    // -------------------------------------------------------------------------
    console.log('\n👉 [Test 4] Public endpoint sanitization (/v1/installations, /v1/presence, QUERY_PEER)...');
    {
      // 1. GET /v1/installations/:id
      const installResp = await httpGet(`/v1/installations/${peerA_Id}`);
      assert.strictEqual(installResp.status, 200);
      assert.strictEqual(installResp.body.id, peerA_Id);
      assert.strictEqual(installResp.body.verified, true);
      assert.strictEqual(installResp.body.publicKey, peerA_Key.publicKeyHex);
      assert.strictEqual(installResp.body.version, '1.0.0');

      // Crucial security assertions: hostname, os, devicesCount, devices must NOT exist
      assert.strictEqual(installResp.body.hostname, undefined, 'hostname MUST NOT be leaked in /v1/installations/:id');
      assert.strictEqual(installResp.body.os, undefined, 'os MUST NOT be leaked in /v1/installations/:id');
      assert.strictEqual(installResp.body.devicesCount, undefined, 'devicesCount MUST NOT be leaked in /v1/installations/:id');
      assert.strictEqual(installResp.body.devices, undefined, 'devices MUST NOT be leaked in /v1/installations/:id');

      // 2. GET /v1/presence
      const presenceResp = await httpGet('/v1/presence');
      assert.strictEqual(presenceResp.status, 200);
      assert(typeof presenceResp.body.totalOnline === 'number');
      assert(typeof presenceResp.body.verifiedCount === 'number');
      assert(typeof presenceResp.body.timestamp === 'number');
      assert.strictEqual(presenceResp.body.peers, undefined, 'Individual peers array MUST NOT be returned in /v1/presence');

      // 3. QUERY_PEER WebSocket query
      registeredWsB.send(JSON.stringify({
        type: 'QUERY_PEER',
        targetId: peerA_Id
      }));

      const queryResp = await waitForMessage(registeredWsB, m => m.type === 'QUERY_PEER_RESP');
      assert.strictEqual(queryResp.found, true);
      assert.strictEqual(queryResp.targetId, peerA_Id);
      assert.strictEqual(queryResp.version, '1.0.0');
      assert.strictEqual(queryResp.info, undefined, 'info object MUST NOT be returned in QUERY_PEER_RESP');
      assert.strictEqual(queryResp.devices, undefined, 'devices array MUST NOT be returned in QUERY_PEER_RESP');
      assert.strictEqual(queryResp.hostname, undefined, 'hostname MUST NOT be returned in QUERY_PEER_RESP');
      assert.strictEqual(queryResp.os, undefined, 'os MUST NOT be returned in QUERY_PEER_RESP');

      console.log('   ✅ Test 4 Passed: Public endpoints are cleanly sanitized; hostnames, OS, and devices stripped.');
    }

    // -------------------------------------------------------------------------
    // TEST 5: Device catalog array validation & sanitization
    // -------------------------------------------------------------------------
    console.log('\n👉 [Test 5] Device catalog array validation and sanitization...');
    {
      // 1. Non-array devices -> ERROR
      registeredWsA.send(JSON.stringify({
        type: 'DEVICE_CATALOG_UPDATE',
        devices: 'malicious_string_not_array'
      }));
      const errNonArray = await waitForMessage(registeredWsA, m => m.type === 'ERROR');
      assert(errNonArray.error.includes('must be an array'), `Expected array error, got: ${errNonArray.error}`);

      // 2. Devices array exceeding limit (> 32) -> ERROR
      const oversizedDevices = Array.from({ length: 33 }, (_, i) => ({
        deviceId: `dev-${i}`,
        name: `Test Device ${i}`
      }));
      registeredWsA.send(JSON.stringify({
        type: 'DEVICE_CATALOG_UPDATE',
        devices: oversizedDevices
      }));
      const errLimit = await waitForMessage(registeredWsA, m => m.type === 'ERROR');
      assert(errLimit.error.includes('at most 32 devices'), `Expected limit error, got: ${errLimit.error}`);

      // 3. Valid array with control characters -> strings properly sanitized
      registeredWsA.send(JSON.stringify({
        type: 'DEVICE_CATALOG_UPDATE',
        devices: [
          {
            deviceId: 'dev-001\x00\x1f',
            name: 'Morpho MSO1300\x07\x08',
            type: 'biometric\x1b',
            status: 'ready'
          }
        ]
      }));

      // Give broker a tick to process
      await new Promise(r => setTimeout(r, 100));

      const storedPeer = peersById.get(peerA_Id);
      assert.ok(storedPeer && storedPeer.info && Array.isArray(storedPeer.info.devices));
      assert.strictEqual(storedPeer.info.devices.length, 1);
      assert.strictEqual(storedPeer.info.devices[0].deviceId, 'dev-001');
      assert.strictEqual(storedPeer.info.devices[0].name, 'Morpho MSO1300');
      assert.strictEqual(storedPeer.info.devices[0].type, 'biometric');
      assert.strictEqual(storedPeer.info.devices[0].status, 'ready');

      console.log('   ✅ Test 5 Passed: Device catalog array bounds enforced and strings sanitized.');
    }

    // -------------------------------------------------------------------------
    // Test 6: Zero-copy binary Turbo-URB frame relay between connected active peer pair
    // -------------------------------------------------------------------------
    console.log('👉 [Test 6] Binary Turbo-URB frame relay between active peer pair...');
    {
      const binaryPromise = new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error('Timeout waiting for binary relay')), 3000);
        registeredWsA.once('message', (data, isBinary) => {
          clearTimeout(timeout);
          resolve({ data, isBinary });
        });
      });

      // Construct a 40-byte Turbo-URB binary packet starting with TURB magic bytes
      const sampleTurbFrame = Buffer.concat([
        Buffer.from('TURB'), // 0x54555242
        Buffer.alloc(32, 0xAA),
        Buffer.from('DESCRIPTORS_PAYLOAD_TEST')
      ]);

      // B sends binary URB completion to A
      registeredWsB.send(sampleTurbFrame, { binary: true });

      const received = await binaryPromise;
      assert.ok(Buffer.isBuffer(received.data), 'Relayed payload must be a Buffer');
      assert.strictEqual(received.data.length, sampleTurbFrame.length, 'Length must match exactly');
      assert.ok(received.data.equals(sampleTurbFrame), 'Binary payload contents must match byte-for-byte');

      console.log('   ✅ Test 6 Passed: Binary Turbo-URB zero-copy frame relay verified.');
    }

    // Cleanup sockets
    if (registeredWsA) registeredWsA.close();
    if (registeredWsB) registeredWsB.close();

    console.log('\n================================================================================');
    console.log('  🎉 ALL 6 SECURITY REGRESSION TESTS PASSED CLEANLY (100% PASS)');
    console.log('================================================================================\n');

  } finally {
    await new Promise((resolve) => {
      server.close(() => resolve());
    });
  }
}

runTests().catch(err => {
  console.error('\n❌ Security Regression Test Failed:\n', err);
  process.exit(1);
});
