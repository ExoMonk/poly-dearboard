import { describe, test, expect, beforeAll } from "bun:test";
import { api, waitForServer, testUser } from "./helpers";

// ---------------------------------------------------------------------------
// Types (mirrored from frontend/src/types.ts — kept minimal for tests)
// ---------------------------------------------------------------------------

interface WalletInfo {
  id: string;
  address: string;
  proxy_address: string | null;
  status: string;
  has_clob_credentials: boolean;
  created_at: string;
}

interface GenerateResponse {
  id: string;
  address: string;
  private_key: string;
  proxy_address: string;
}

interface ImportResponse {
  id: string;
  address: string;
  proxy_address: string;
}

interface DeriveResponse {
  success: boolean;
  wallet_id: string;
  api_key: string;
}

/** Helper: delete all wallets for a user */
async function cleanupWallets(token: string) {
  const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
  if (list.ok && Array.isArray(list.data)) {
    for (const w of list.data) {
      await api("DELETE", `/api/wallets/${w.id}`, { token });
    }
  }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeAll(async () => {
  await waitForServer();
});

// ---------------------------------------------------------------------------
// GET /api/wallets
// ---------------------------------------------------------------------------

describe("GET /api/wallets", () => {
  test("returns empty array when no wallets exist", async () => {
    const { token } = testUser();
    const res = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(res.status).toBe(200);
    expect(res.data).toEqual([]);
  });

  test("returns 401 without auth", async () => {
    const res = await api("GET", "/api/wallets");
    expect(res.status).toBe(401);
  });
});

// ---------------------------------------------------------------------------
// POST /api/wallets/generate
// ---------------------------------------------------------------------------

describe("POST /api/wallets/generate", () => {
  test("generates a new wallet and returns id + private key", async () => {
    const { token } = testUser();

    const res = await api<GenerateResponse>("POST", "/api/wallets/generate", {
      token,
    });
    expect(res.status).toBe(200);
    expect(res.data.id).toBeTruthy();
    expect(res.data.address).toMatch(/^0x[0-9a-f]{40}$/);
    expect(res.data.private_key).toMatch(/^0x[0-9a-f]{64}$/);
    expect(res.data.proxy_address).toMatch(/^0x[0-9a-f]{40}$/);

    // Verify GET returns wallet in list without private key
    const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(list.status).toBe(200);
    expect(list.data).toHaveLength(1);
    expect(list.data[0].id).toBe(res.data.id);
    expect(list.data[0].address).toBe(res.data.address);
    expect(list.data[0].status).toBe("created");
    expect(list.data[0].has_clob_credentials).toBe(false);
    expect(list.data[0]).not.toHaveProperty("private_key");

    await cleanupWallets(token);
  });
});

// ---------------------------------------------------------------------------
// POST /api/wallets/import
// ---------------------------------------------------------------------------

describe("POST /api/wallets/import", () => {
  test("imports a valid private key", async () => {
    const { token } = testUser();
    const testKey =
      "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    const res = await api<ImportResponse>("POST", "/api/wallets/import", {
      token,
      body: { private_key: testKey },
    });
    expect(res.status).toBe(200);
    expect(res.data.id).toBeTruthy();
    expect(res.data.address).toMatch(/^0x[0-9a-f]{40}$/);

    // Verify via GET
    const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(list.data).toHaveLength(1);
    expect(list.data[0].address).toBe(res.data.address);

    await cleanupWallets(token);
  });

  test("rejects invalid private key (too short)", async () => {
    const { token } = testUser();
    const res = await api("POST", "/api/wallets/import", {
      token,
      body: { private_key: "0xdead" },
    });
    expect(res.status).toBe(400);
  });

  test("rejects invalid hex characters", async () => {
    const { token } = testUser();
    const res = await api("POST", "/api/wallets/import", {
      token,
      body: {
        private_key:
          "0xzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
      },
    });
    expect(res.status).toBe(400);
  });
});

// ---------------------------------------------------------------------------
// POST /api/wallets/:id/derive-credentials
// ---------------------------------------------------------------------------

describe("POST /api/wallets/:id/derive-credentials", () => {
  test("derives CLOB credentials for a specific wallet", async () => {
    const { token } = testUser();

    // Generate wallet first
    const gen = await api<GenerateResponse>("POST", "/api/wallets/generate", {
      token,
    });

    // Derive credentials (calls live Polymarket CLOB API)
    const res = await api<DeriveResponse>(
      "POST",
      `/api/wallets/${gen.data.id}/derive-credentials`,
      { token },
    );
    expect(res.status).toBe(200);
    expect(res.data.success).toBe(true);
    expect(res.data.wallet_id).toBe(gen.data.id);
    expect(res.data.api_key).toBeTruthy();

    // Verify status changed to credentialed
    const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(list.data[0].status).toBe("credentialed");
    expect(list.data[0].has_clob_credentials).toBe(true);

    await cleanupWallets(token);
  });

  test("returns 404 for non-existent wallet id", async () => {
    const { token } = testUser();
    const res = await api(
      "POST",
      "/api/wallets/nonexistent-id/derive-credentials",
      { token },
    );
    expect(res.status).toBe(404);
  });
});

// ---------------------------------------------------------------------------
// DELETE /api/wallets/:id
// ---------------------------------------------------------------------------

describe("DELETE /api/wallets/:id", () => {
  test("deletes a specific wallet", async () => {
    const { token } = testUser();

    const gen = await api<GenerateResponse>("POST", "/api/wallets/generate", {
      token,
    });
    const del = await api("DELETE", `/api/wallets/${gen.data.id}`, { token });
    expect(del.status).toBe(204);

    // Verify gone
    const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(list.data).toHaveLength(0);
  });

  test("returns 404 for non-existent wallet id", async () => {
    const { token } = testUser();
    const res = await api("DELETE", "/api/wallets/nonexistent-id", { token });
    expect(res.status).toBe(404);
  });
});

// ---------------------------------------------------------------------------
// Multi-wallet: up to 3 per user
// ---------------------------------------------------------------------------

describe("multi-wallet support", () => {
  test("user can create up to 3 wallets", async () => {
    const { token } = testUser();

    const w1 = await api<GenerateResponse>("POST", "/api/wallets/generate", { token });
    const w2 = await api<GenerateResponse>("POST", "/api/wallets/generate", { token });
    const w3 = await api<GenerateResponse>("POST", "/api/wallets/generate", { token });
    expect(w1.status).toBe(200);
    expect(w2.status).toBe(200);
    expect(w3.status).toBe(200);

    // All 3 returned in list
    const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(list.data).toHaveLength(3);

    // All unique IDs
    const ids = new Set(list.data.map((w) => w.id));
    expect(ids.size).toBe(3);

    await cleanupWallets(token);
  });

  test("4th wallet returns 409 limit reached", async () => {
    const { token } = testUser();

    await api("POST", "/api/wallets/generate", { token });
    await api("POST", "/api/wallets/generate", { token });
    await api("POST", "/api/wallets/generate", { token });

    const fourth = await api("POST", "/api/wallets/generate", { token });
    expect(fourth.status).toBe(409);
    expect(fourth.text).toContain("limit");

    await cleanupWallets(token);
  });

  test("import also counts toward limit", async () => {
    const { token } = testUser();
    const testKey =
      "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    await api("POST", "/api/wallets/generate", { token });
    await api("POST", "/api/wallets/generate", { token });
    await api("POST", "/api/wallets/import", { token, body: { private_key: testKey } });

    // 4th (via generate) should fail
    const fourth = await api("POST", "/api/wallets/generate", { token });
    expect(fourth.status).toBe(409);

    await cleanupWallets(token);
  });

  test("deleting one wallet frees a slot", async () => {
    const { token } = testUser();

    const w1 = await api<GenerateResponse>("POST", "/api/wallets/generate", { token });
    await api("POST", "/api/wallets/generate", { token });
    await api("POST", "/api/wallets/generate", { token });

    // At limit
    const full = await api("POST", "/api/wallets/generate", { token });
    expect(full.status).toBe(409);

    // Delete one
    await api("DELETE", `/api/wallets/${w1.data.id}`, { token });

    // Now can add another
    const newWallet = await api<GenerateResponse>("POST", "/api/wallets/generate", { token });
    expect(newWallet.status).toBe(200);

    const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(list.data).toHaveLength(3);

    await cleanupWallets(token);
  });

  test("deleting one wallet does not affect others", async () => {
    const { token } = testUser();

    const w1 = await api<GenerateResponse>("POST", "/api/wallets/generate", { token });
    const w2 = await api<GenerateResponse>("POST", "/api/wallets/generate", { token });

    // Delete w1
    await api("DELETE", `/api/wallets/${w1.data.id}`, { token });

    // w2 still exists
    const list = await api<WalletInfo[]>("GET", "/api/wallets", { token });
    expect(list.data).toHaveLength(1);
    expect(list.data[0].id).toBe(w2.data.id);

    await cleanupWallets(token);
  });
});

// ---------------------------------------------------------------------------
// Cross-cutting: generate → import round-trip (address consistency)
// ---------------------------------------------------------------------------

describe("wallet round-trip", () => {
  test("importing the generated key produces the same address", async () => {
    const user1 = testUser();
    const user2 = testUser();

    // Generate with user1
    const gen = await api<GenerateResponse>("POST", "/api/wallets/generate", {
      token: user1.token,
    });
    expect(gen.status).toBe(200);

    // Import same key with user2
    const imp = await api<ImportResponse>("POST", "/api/wallets/import", {
      token: user2.token,
      body: { private_key: gen.data.private_key },
    });
    expect(imp.status).toBe(200);
    expect(imp.data.address).toBe(gen.data.address);
    expect(imp.data.proxy_address).toBe(gen.data.proxy_address);

    await cleanupWallets(user1.token);
    await cleanupWallets(user2.token);
  });
});
