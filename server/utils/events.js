// Signed event envelope — the on-wire format for federated Hey data.
//
// Every message that will eventually cross the Carrier gossip mesh is wrapped
// in an envelope: a fixed shape (type, payload, sender_did, ts) signed with
// the sender's Ed25519 key. Recipients verify the signature against the
// sender's did:key before trusting the event.
//
// Today (Phase 2) we use this for chat messages stored locally — even on a
// single Hey instance, every chat line gets signed and verified. That's
// future-proofing: when Phase 3 swaps the local transport for Carrier, the
// wire format doesn't change. And it lets us show the "✓ signed" UI
// affordance from day one without lying about provenance.
//
// Canonical serialization: we JSON.stringify the envelope with sorted keys so
// the signing-time bytes are identical to the verifying-time bytes regardless
// of how the JSON was reconstructed from the wire.

const {
  publicKeyToDidKey,
  didKeyToPublicKey,
  sign,
  verify,
} = require("./identity");

// Sort-keys JSON serializer. Required so sign-then-roundtrip-then-verify
// works even if intermediate hops re-encode the JSON with different key
// ordering. Recursive across nested objects/arrays.
const canonicalize = (value) => {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) {
    return "[" + value.map(canonicalize).join(",") + "]";
  }
  const keys = Object.keys(value).sort();
  return (
    "{" +
    keys.map((k) => JSON.stringify(k) + ":" + canonicalize(value[k])).join(",") +
    "}"
  );
};

// The bytes that get signed: everything in the envelope except the signature
// field itself. (Otherwise we'd be signing the signature too — circular.)
const bytesToSign = ({ type, payload, sender_did, ts }) =>
  canonicalize({ type, payload, sender_did, ts });

// Construct a signed envelope. `keypair` is the output of
// identity.keypairFromAuthKey(). Returns the on-wire object.
const createSignedEvent = ({ type, payload }, keypair) => {
  if (typeof type !== "string" || !type) {
    throw new Error("event.type is required");
  }
  if (payload === undefined) {
    throw new Error("event.payload is required");
  }
  const sender_did = publicKeyToDidKey(keypair.publicKey);
  const ts = Date.now();
  const signature = sign(bytesToSign({ type, payload, sender_did, ts }), keypair.privateKey);
  return { type, payload, sender_did, ts, signature };
};

// Verify a received event. Returns { valid: bool, reason?: string }. We never
// throw on tampering — callers always get back a boolean, so the verify path
// can't be turned into a DoS by sending malformed events.
const verifySignedEvent = (event) => {
  if (!event || typeof event !== "object") {
    return { valid: false, reason: "not-an-object" };
  }
  const { type, payload, sender_did, ts, signature } = event;
  if (typeof type !== "string" || !type) return { valid: false, reason: "bad-type" };
  if (payload === undefined) return { valid: false, reason: "no-payload" };
  if (typeof sender_did !== "string" || !sender_did.startsWith("did:key:z")) {
    return { valid: false, reason: "bad-sender_did" };
  }
  if (!Number.isInteger(ts) || ts <= 0) return { valid: false, reason: "bad-ts" };
  if (typeof signature !== "string" || signature.length !== 128) {
    return { valid: false, reason: "bad-signature-shape" };
  }

  let pubKey;
  try {
    pubKey = didKeyToPublicKey(sender_did);
  } catch {
    return { valid: false, reason: "unresolvable-did" };
  }

  const ok = verify(bytesToSign({ type, payload, sender_did, ts }), signature, pubKey);
  return ok ? { valid: true } : { valid: false, reason: "signature-mismatch" };
};

module.exports = {
  createSignedEvent,
  verifySignedEvent,
  canonicalize,
};
