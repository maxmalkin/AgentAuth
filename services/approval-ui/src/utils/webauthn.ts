// WebAuthn/Passkey utilities for signing approval assertions

import type { ApprovalAssertion } from '../types';

/** Check if WebAuthn is supported */
export function isWebAuthnSupported(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.PublicKeyCredential !== 'undefined'
  );
}

/** Check if platform authenticator (passkey) is available */
export async function isPlatformAuthenticatorAvailable(): Promise<boolean> {
  if (!isWebAuthnSupported()) {
    return false;
  }
  try {
    return await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable();
  } catch {
    return false;
  }
}

/** Convert ArrayBuffer to base64url string */
function bufferToBase64url(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]!);
  }
  return btoa(binary)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=/g, '');
}

/** Convert base64url string to ArrayBuffer */
function base64urlToBuffer(base64url: string): ArrayBuffer {
  const base64 = base64url.replace(/-/g, '+').replace(/_/g, '/');
  const padding = '='.repeat((4 - (base64.length % 4)) % 4);
  const binary = atob(base64 + padding);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}

/** Generate a challenge for WebAuthn */
function generateChallenge(): ArrayBuffer {
  const challenge = new Uint8Array(32);
  crypto.getRandomValues(challenge);
  return challenge.buffer as ArrayBuffer;
}

/** Sign an approval assertion using WebAuthn */
export async function signApprovalAssertion(
  assertion: ApprovalAssertion,
  credentialId?: string
): Promise<string> {
  if (!isWebAuthnSupported()) {
    throw new Error('WebAuthn is not supported in this browser');
  }

  // Create the data to sign - JSON serialization of the assertion
  const assertionJson = JSON.stringify(assertion);
  const encoder = new TextEncoder();
  const assertionBytes = encoder.encode(assertionJson);

  // Hash the assertion data to use as challenge
  const hashBuffer = await crypto.subtle.digest('SHA-256', assertionBytes);
  const challenge = new Uint8Array(hashBuffer);

  // Build the WebAuthn request options
  const options: PublicKeyCredentialRequestOptions = {
    challenge,
    timeout: 60000, // 1 minute
    userVerification: 'required',
    rpId: window.location.hostname,
  };

  // If we have a specific credential ID, use it
  if (credentialId) {
    options.allowCredentials = [
      {
        type: 'public-key',
        id: base64urlToBuffer(credentialId),
      },
    ];
  }

  // Request the credential
  const credential = (await navigator.credentials.get({
    publicKey: options,
  })) as PublicKeyCredential | null;

  if (!credential) {
    throw new Error('No credential returned from WebAuthn');
  }

  const response = credential.response as AuthenticatorAssertionResponse;

  // Combine authenticator data, client data, and signature into a single payload
  const signaturePayload = {
    credentialId: bufferToBase64url(credential.rawId),
    authenticatorData: bufferToBase64url(response.authenticatorData),
    clientDataJSON: bufferToBase64url(response.clientDataJSON),
    signature: bufferToBase64url(response.signature),
    assertionHash: bufferToBase64url(hashBuffer),
  };

  return JSON.stringify(signaturePayload);
}

/** Register a new passkey for the current user */
export async function registerPasskey(
  userId: string,
  userName: string,
  userDisplayName: string
): Promise<{ credentialId: string; publicKey: string }> {
  if (!isWebAuthnSupported()) {
    throw new Error('WebAuthn is not supported in this browser');
  }

  const challenge = generateChallenge();

  const options: PublicKeyCredentialCreationOptions = {
    challenge,
    rp: {
      name: 'AgentAuth',
      id: window.location.hostname,
    },
    user: {
      id: new TextEncoder().encode(userId),
      name: userName,
      displayName: userDisplayName,
    },
    pubKeyCredParams: [
      { alg: -7, type: 'public-key' }, // ES256
      { alg: -257, type: 'public-key' }, // RS256
    ],
    authenticatorSelection: {
      authenticatorAttachment: 'platform',
      userVerification: 'required',
      residentKey: 'required',
    },
    timeout: 60000,
    attestation: 'none',
  };

  const credential = (await navigator.credentials.create({
    publicKey: options,
  })) as PublicKeyCredential | null;

  if (!credential) {
    throw new Error('No credential returned from WebAuthn registration');
  }

  const response = credential.response as AuthenticatorAttestationResponse;

  return {
    credentialId: bufferToBase64url(credential.rawId),
    publicKey: bufferToBase64url(response.getPublicKey()!),
  };
}
