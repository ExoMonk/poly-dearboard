import { createHmac } from "crypto";

// ---------------------------------------------------------------------------
// Config — reads from env or uses defaults for local dev
// ---------------------------------------------------------------------------

export const API_BASE = "http://localhost:3001";
export const JWT_SECRET: string = process.env.JWT_SECRET || "";
// ---------------------------------------------------------------------------
// JWT helpers
// ---------------------------------------------------------------------------

function base64url(data: string): string {
  return Buffer.from(data).toString("base64url");
}

/** Mint a HS256 JWT matching the server's `issue_jwt` format. */
export function mintJwt(
  address: string,
  expiresInSec = 86400,
): string {
  const now = Math.floor(Date.now() / 1000);
  const header = base64url(JSON.stringify({ alg: "HS256", typ: "JWT" }));
  const payload = base64url(
    JSON.stringify({
      sub: address.toLowerCase(),
      iat: now,
      exp: now + expiresInSec,
    }),
  );
  const sigInput = `${header}.${payload}`;
  const sig = createHmac("sha256", JWT_SECRET)
    .update(sigInput)
    .digest("base64url");
  return `${header}.${payload}.${sig}`;
}

// ---------------------------------------------------------------------------
// API client
// ---------------------------------------------------------------------------

type Method = "GET" | "POST" | "DELETE";

interface ApiResponse<T = unknown> {
  status: number;
  ok: boolean;
  data: T;
  text: string;
}

/** Make an authenticated API request. */
export async function api<T = unknown>(
  method: Method,
  path: string,
  opts?: { token?: string; body?: unknown },
): Promise<ApiResponse<T>> {
  const headers: Record<string, string> = {};
  if (opts?.token) headers["Authorization"] = `Bearer ${opts.token}`;
  if (opts?.body) headers["Content-Type"] = "application/json";

  const res = await fetch(`${API_BASE}${path}`, {
    method,
    headers,
    body: opts?.body ? JSON.stringify(opts.body) : undefined,
  });

  const text = await res.text();
  let data: T | undefined;
  try {
    data = JSON.parse(text) as T;
  } catch {
    data = text as unknown as T;
  }

  return { status: res.status, ok: res.ok, data: data!, text };
}

// ---------------------------------------------------------------------------
// Server readiness
// ---------------------------------------------------------------------------

/** Wait for the API server to be reachable (up to timeoutMs). */
export async function waitForServer(timeoutMs = 5000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(`${API_BASE}/api/health`);
      if (res.ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`Server not reachable at ${API_BASE} after ${timeoutMs}ms`);
}

// ---------------------------------------------------------------------------
// Test user factory
// ---------------------------------------------------------------------------

let userCounter = 0;

/** Generate a unique fake Ethereum address + JWT for test isolation. */
export function testUser(): { address: string; token: string } {
  userCounter++;
  const hex = userCounter.toString(16).padStart(40, "0");
  const address = `0x${hex}`;
  return { address, token: mintJwt(address) };
}
