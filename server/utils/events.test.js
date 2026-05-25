import { describe, it, expect } from "vitest";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { keypairFromAuthKey } = require("./identity.js");
const { createSignedEvent, verifySignedEvent, canonicalize } = require("./events.js");

const KEY_A = "a".repeat(64);
const KEY_B = "b".repeat(64);

describe("canonicalize (sorted-keys JSON)", () => {
  it("emits keys in sorted order regardless of object insertion order", () => {
    expect(canonicalize({ b: 1, a: 2 })).toBe(canonicalize({ a: 2, b: 1 }));
  });

  it("handles nested objects", () => {
    const a = { x: { z: 1, y: 2 }, w: [3, 2, 1] };
    const b = { w: [3, 2, 1], x: { y: 2, z: 1 } };
    expect(canonicalize(a)).toBe(canonicalize(b));
  });

  it("preserves array order (arrays aren't sorted)", () => {
    expect(canonicalize([1, 2, 3])).toBe("[1,2,3]");
    expect(canonicalize([3, 2, 1])).toBe("[3,2,1]");
  });

  it("handles primitives", () => {
    expect(canonicalize(null)).toBe("null");
    expect(canonicalize(42)).toBe("42");
    expect(canonicalize("hello")).toBe('"hello"');
    expect(canonicalize(true)).toBe("true");
  });
});

describe("createSignedEvent + verifySignedEvent", () => {
  it("round-trips a basic event", () => {
    const kp = keypairFromAuthKey(KEY_A);
    const event = createSignedEvent({ type: "chat.msg", payload: { text: "hi" } }, kp);
    expect(event.type).toBe("chat.msg");
    expect(event.payload).toEqual({ text: "hi" });
    expect(event.sender_did.startsWith("did:key:z")).toBe(true);
    expect(typeof event.ts).toBe("number");
    expect(event.signature.length).toBe(128); // 64 bytes hex
    expect(verifySignedEvent(event).valid).toBe(true);
  });

  it("detects payload tampering", () => {
    const kp = keypairFromAuthKey(KEY_A);
    const event = createSignedEvent({ type: "chat.msg", payload: { text: "hi" } }, kp);
    event.payload.text = "tampered";
    expect(verifySignedEvent(event).valid).toBe(false);
  });

  it("detects sender_did tampering", () => {
    const a = keypairFromAuthKey(KEY_A);
    const b = keypairFromAuthKey(KEY_B);
    const event = createSignedEvent({ type: "chat.msg", payload: "x" }, a);
    // Lie about who sent it
    const fakeEvent = { ...event, sender_did: createSignedEvent({ type: "x", payload: "y" }, b).sender_did };
    expect(verifySignedEvent(fakeEvent).valid).toBe(false);
  });

  it("detects ts tampering", () => {
    const kp = keypairFromAuthKey(KEY_A);
    const event = createSignedEvent({ type: "chat.msg", payload: "x" }, kp);
    expect(verifySignedEvent({ ...event, ts: event.ts + 1 }).valid).toBe(false);
  });

  it("verifies after a full JSON round-trip (simulates wire transport)", () => {
    const kp = keypairFromAuthKey(KEY_A);
    const event = createSignedEvent({ type: "chat.msg", payload: { a: 1, b: 2 } }, kp);
    // The receiving side reconstructs from JSON — keys may come back in any order.
    const onWire = JSON.parse(JSON.stringify(event));
    expect(verifySignedEvent(onWire).valid).toBe(true);
  });

  it("verifies after deliberate key reordering (proves canonicalization works)", () => {
    const kp = keypairFromAuthKey(KEY_A);
    const event = createSignedEvent({ type: "x", payload: { b: 1, a: 2 } }, kp);
    // Reconstruct the payload with reversed key order
    const reordered = {
      signature: event.signature,
      ts: event.ts,
      sender_did: event.sender_did,
      payload: { a: 2, b: 1 },
      type: "x",
    };
    expect(verifySignedEvent(reordered).valid).toBe(true);
  });
});

describe("verifySignedEvent rejects malformed inputs without throwing", () => {
  it.each([
    [null, "not-an-object"],
    [undefined, "not-an-object"],
    ["string", "not-an-object"],
    [{}, "bad-type"],
    [{ type: "" }, "bad-type"],
    [{ type: "x" }, "no-payload"],
    [{ type: "x", payload: 1 }, "bad-sender_did"],
    [{ type: "x", payload: 1, sender_did: "did:web:foo" }, "bad-sender_did"],
    [{ type: "x", payload: 1, sender_did: "did:key:zABC", ts: -1 }, "bad-ts"],
    [{ type: "x", payload: 1, sender_did: "did:key:zABC", ts: 1, signature: "deadbeef" }, "bad-signature-shape"],
  ])("rejects %s with reason %s", (input, expectedReason) => {
    const result = verifySignedEvent(input);
    expect(result.valid).toBe(false);
    expect(result.reason).toBe(expectedReason);
  });
});

describe("rejects unresolvable did:key gracefully", () => {
  it("returns valid:false instead of throwing on garbage did:key", () => {
    // valid shape but garbage content
    const badEvent = {
      type: "x",
      payload: 1,
      sender_did: "did:key:zNotARealKey",
      ts: 1,
      signature: "00".repeat(64),
    };
    const result = verifySignedEvent(badEvent);
    expect(result.valid).toBe(false);
    // could be "unresolvable-did" or "signature-mismatch" depending on shape
    expect(["unresolvable-did", "signature-mismatch"]).toContain(result.reason);
  });
});
