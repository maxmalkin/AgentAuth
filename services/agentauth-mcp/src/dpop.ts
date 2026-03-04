import * as ed from "@noble/ed25519";
import { sha256 } from "@noble/hashes/sha2.js";
import { uuidv7 } from "uuidv7";

function b64url(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("base64url");
}

function b64urlStr(str: string): string {
  return b64url(new TextEncoder().encode(str));
}

/** JWK thumbprint for a given Ed25519 public key (RFC 7638). */
export function jwkThumbprint(pubKey: Uint8Array): string {
  // Keys sorted alphabetically per RFC 7638
  const canonical = JSON.stringify({ crv: "Ed25519", kty: "OKP", x: b64url(pubKey) });
  return b64url(sha256(new TextEncoder().encode(canonical)));
}

/**
 * Generate a DPoP proof JWT for a request.
 *
 * @param privKey - 32-byte Ed25519 private key seed
 * @param pubKey  - 32-byte Ed25519 public key
 * @param method  - HTTP method (GET, POST, DELETE, …)
 * @param url     - Full request URL (query string stripped)
 * @param token   - Access token string, if binding an existing token (adds `ath`)
 */
export async function makeDpopProof(
  privKey: Uint8Array,
  pubKey: Uint8Array,
  method: string,
  url: string,
  token?: string,
): Promise<string> {
  const jwk = { kty: "OKP", crv: "Ed25519", x: b64url(pubKey) };

  const header = { alg: "EdDSA", typ: "dpop+jwt", jwk };
  const headerB64 = b64urlStr(JSON.stringify(header));

  // Strip query string from URL
  const htu = url.split("?")[0];

  const payload: Record<string, unknown> = {
    jti: uuidv7(),
    htm: method.toUpperCase(),
    htu,
    iat: Math.floor(Date.now() / 1000),
  };

  if (token) {
    // ath = base64url(SHA-256(ASCII(token)))
    payload.ath = b64url(sha256(new TextEncoder().encode(token)));
  }

  const payloadB64 = b64urlStr(JSON.stringify(payload));
  const signingInput = new TextEncoder().encode(`${headerB64}.${payloadB64}`);
  const signature = await ed.signAsync(signingInput, privKey);

  return `${headerB64}.${payloadB64}.${b64url(signature)}`;
}
