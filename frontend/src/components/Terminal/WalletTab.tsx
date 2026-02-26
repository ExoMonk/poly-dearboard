import { useState, useEffect } from "react";
import {
  useWallets,
  useGenerateWallet,
  useImportWallet,
  useDeriveCredentials,
  useDeleteWallet,
  useWalletBalance,
  useEnableTrading,
  useApproveExchanges,
  useDepositAddress,
  useDepositStatus,
} from "../../hooks/useWallet";
import { useTerminalDispatch } from "./TerminalProvider";
import { useAuth } from "../../context/AuthContext";
import type { TradingWalletInfo, WalletGenerateResponse } from "../../types";

function truncateAddress(addr: string): string {
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <button
      onClick={handleCopy}
      className="ml-2 px-1.5 py-0.5 text-[10px] rounded-lg bg-white/5 hover:bg-white/10 text-[var(--text-muted)] transition-colors"
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

function InfoNote({ children }: { children: React.ReactNode }) {
  return (
    <p className="mt-3 text-[11px] text-[var(--text-muted)] leading-relaxed">
      <span className="text-[var(--accent-blue)] mr-1">i</span>
      {children}
    </p>
  );
}

function StatusBadge({ status }: { status: string }) {
  const isCredentialed = status === "credentialed";
  return (
    <span className={`px-1.5 py-0.5 text-[10px] rounded ${
      isCredentialed ? "bg-green-500/15 text-green-400" : "bg-yellow-500/15 text-yellow-400"
    }`}>
      {status.charAt(0).toUpperCase() + status.slice(1)}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Balance + Funding section (shown for credentialed wallets)
// ---------------------------------------------------------------------------

function BalanceSection({
  walletId,
  walletAddress,
  proxyAddress,
  addLog,
}: {
  walletId: string;
  walletAddress: string;
  proxyAddress: string | null;
  addLog: (level: "info" | "warn" | "error" | "success", msg: string) => void;
}) {
  const { data: balance } = useWalletBalance(walletId);
  const { data: depositAddr } = useDepositAddress(walletId);
  const { data: depositStatus } = useDepositStatus(walletId);
  const approveMutation = useApproveExchanges();
  const enableTradingMutation = useEnableTrading();
  const [showFunding, setShowFunding] = useState(false);
  const [deploying, setDeploying] = useState(false);
  const [deployFailed, setDeployFailed] = useState(false);

  const handleApprove = async () => {
    try {
      const result = await approveMutation.mutateAsync(walletId);
      if (result.already_approved) {
        addLog("info", "Exchanges already approved");
      } else {
        addLog("success", `Exchanges approved${result.tx_hash ? `: ${result.tx_hash}` : ""}`);
      }
    } catch (e) {
      addLog("error", `Approval failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleEnableTrading = async () => {
    try {
      setDeploying(true);
      setDeployFailed(false);
      const result = await enableTradingMutation.mutateAsync(walletId);
      if (result.status === "already_deployed" || result.status === "deployed") {
        addLog("success", "Trading enabled — Safe proxy deployed");
      } else {
        addLog("info", "Safe deployment submitted to relayer");
      }
    } catch (e) {
      setDeployFailed(true);
      addLog("error", `Enable trading failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setDeploying(false);
    }
  };

  if (!balance) return null;

  const allApproved = balance.ctf_exchange_approved && balance.neg_risk_exchange_approved;
  const staleSecs = balance.last_checked_secs_ago ?? 0;
  const staleWarning = staleSecs > 120;

  return (
    <div className="mt-2 pt-2 border-t border-[var(--border-glow)]">
      {/* Balance row */}
      <div className="flex items-center justify-between text-[11px]">
        <div className="flex items-center gap-2">
          <span className="text-[var(--text-muted)]">USDC</span>
          <span className="text-[var(--text-primary)] font-mono font-medium">
            ${balance.usdc_balance}
          </span>
        </div>
        <div className="flex items-center gap-2">
          {staleWarning && (
            <span className="text-yellow-400 text-[10px]" title={`Last checked ${staleSecs}s ago`}>
              stale
            </span>
          )}
        </div>
      </div>

      {/* Safe proxy recovery — only shown if auto-deploy during CLOB derivation failed */}
      {!balance.safe_deployed && (
        <div className="flex items-center gap-3 mt-1.5 text-[11px]">
          <span className="text-[var(--text-muted)]">Safe</span>
          {deploying ? (
            <span className="text-yellow-400">Deploying...</span>
          ) : deployFailed ? (
            <>
              <span className="text-red-400">Deploy failed</span>
              <button
                onClick={handleEnableTrading}
                className="px-2 py-0.5 text-[10px] font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors"
              >
                Retry
              </button>
            </>
          ) : (
            <>
              <span className="text-yellow-400">Not deployed</span>
              <button
                onClick={handleEnableTrading}
                disabled={enableTradingMutation.isPending}
                className="px-2 py-0.5 text-[10px] font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {enableTradingMutation.isPending ? "Deploying..." : "Deploy Safe"}
              </button>
            </>
          )}
        </div>
      )}

      {/* Step 2: Approvals */}
      <div className="flex items-center gap-3 mt-1.5 text-[11px]">
        <span className="text-[var(--text-muted)]">Step 2</span>
        <span className={balance.ctf_exchange_approved ? "text-green-400" : "text-red-400"}>
          CTF {balance.ctf_exchange_approved ? "\u2713" : "\u2717"}
        </span>
        <span className={balance.neg_risk_exchange_approved ? "text-green-400" : "text-red-400"}>
          NegRisk {balance.neg_risk_exchange_approved ? "\u2713" : "\u2717"}
        </span>
        {!allApproved && (
          <button
            onClick={handleApprove}
            disabled={approveMutation.isPending || !balance.safe_deployed}
            className="px-2 py-0.5 text-[10px] font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {approveMutation.isPending ? "Approving..." : "Approve All"}
          </button>
        )}
      </div>

      {/* Step 4: Fund wallet */}
      <button
        onClick={() => setShowFunding(!showFunding)}
        className="mt-2 text-[11px] text-[var(--accent-blue)] hover:underline"
      >
        {showFunding ? "Hide funding" : `${allApproved ? "Step 3 — " : ""}Fund wallet`}
      </button>

      {showFunding && (
        <div className="mt-2 space-y-2 text-[11px]">
          {/* Direct transfer */}
          <div className="px-2 py-1.5 rounded bg-white/[0.03] border border-[var(--border-glow)]">
            <p className="text-[var(--text-muted)] mb-1">Direct Transfer (USDC.e on Polygon)</p>
            <div className="flex items-center gap-1 font-mono text-[var(--text-primary)]">
              <span>{truncateAddress(proxyAddress ?? walletAddress)}</span>
              <CopyButton text={proxyAddress ?? walletAddress} />
            </div>
          </div>

          {/* Bridge addresses */}
          {depositAddr && (
            <div className="px-2 py-1.5 rounded bg-white/[0.03] border border-[var(--border-glow)]">
              <p className="text-[var(--text-muted)] mb-1">Bridge (Cross-Chain)</p>
              {depositAddr.evm && (
                <div className="flex items-center gap-1 font-mono text-[10px]">
                  <span className="text-[var(--text-muted)] w-8">EVM</span>
                  <span className="text-[var(--text-primary)]">{truncateAddress(depositAddr.evm)}</span>
                  <CopyButton text={depositAddr.evm} />
                </div>
              )}
              {depositAddr.svm && (
                <div className="flex items-center gap-1 font-mono text-[10px]">
                  <span className="text-[var(--text-muted)] w-8">SOL</span>
                  <span className="text-[var(--text-primary)]">{truncateAddress(depositAddr.svm)}</span>
                  <CopyButton text={depositAddr.svm} />
                </div>
              )}
              {depositAddr.btc && (
                <div className="flex items-center gap-1 font-mono text-[10px]">
                  <span className="text-[var(--text-muted)] w-8">BTC</span>
                  <span className="text-[var(--text-primary)]">{truncateAddress(depositAddr.btc)}</span>
                  <CopyButton text={depositAddr.btc} />
                </div>
              )}
            </div>
          )}

          {/* Pending deposits */}
          {depositStatus && depositStatus.pending.length > 0 && (
            <div className="px-2 py-1.5 rounded bg-yellow-500/5 border border-yellow-500/20">
              <p className="text-yellow-400 mb-1">Pending Deposits</p>
              {depositStatus.pending.map((d, i) => (
                <div key={i} className="text-[10px] text-[var(--text-secondary)]">
                  {d.amount} {d.token} from {d.from_chain} — {d.status}
                </div>
              ))}
            </div>
          )}

          <InfoNote>
            To withdraw, import your trading wallet key into MetaMask.
          </InfoNote>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// No wallets view
// ---------------------------------------------------------------------------

function NoWalletsView({
  onGenerate,
  onImport,
  isGenerating,
}: {
  onGenerate: () => void;
  onImport: () => void;
  isGenerating: boolean;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 px-4">
      <p className="text-sm text-[var(--text-secondary)]">Trading Wallets</p>
      <p className="text-xs text-[var(--text-muted)]">No wallets configured.</p>
      <div className="flex gap-2 mt-2">
        <button
          onClick={onGenerate}
          disabled={isGenerating}
          className="px-3 py-1.5 text-xs font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {isGenerating ? "Generating..." : "Generate New Wallet"}
        </button>
        <button
          onClick={onImport}
          className="px-3 py-1.5 text-xs font-semibold rounded-xl border border-[var(--border-glow)] text-[var(--text-secondary)] hover:bg-white/5 transition-colors"
        >
          Import Existing
        </button>
      </div>
      <InfoNote>
        Create a dedicated wallet, then follow the steps on the card: Derive CLOB &rarr; Approve &rarr; Fund.
      </InfoNote>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Generate result (one-time key display)
// ---------------------------------------------------------------------------

function GenerateResultView({
  result,
  onContinue,
}: {
  result: WalletGenerateResponse;
  onContinue: () => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 px-4">
      <p className="text-sm text-yellow-400 font-medium">SAVE YOUR PRIVATE KEY</p>

      <div className="w-full max-w-md space-y-2 text-xs">
        <div className="flex items-center justify-between px-3 py-2 rounded bg-white/5">
          <span className="text-[var(--text-muted)]">Address</span>
          <span className="text-[var(--text-primary)] font-mono">
            {truncateAddress(result.address)}
            <CopyButton text={result.address} />
          </span>
        </div>

        <div className="flex items-center justify-between px-3 py-2 rounded bg-yellow-500/10 border border-yellow-500/20">
          <span className="text-yellow-400">Private Key</span>
          <span className="text-[var(--text-primary)] font-mono">
            {result.private_key.slice(0, 10)}...{result.private_key.slice(-6)}
            <CopyButton text={result.private_key} />
          </span>
        </div>
      </div>

      <p className="text-[11px] text-yellow-400/80 mt-1">
        This key will NOT be shown again. Back it up securely before proceeding.
      </p>

      <button
        onClick={onContinue}
        className="mt-2 px-4 py-1.5 text-xs font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors"
      >
        I've saved my key — Continue
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Import flow
// ---------------------------------------------------------------------------

function ImportView({
  onSubmit,
  onCancel,
  isImporting,
  error,
}: {
  onSubmit: (key: string) => void;
  onCancel: () => void;
  isImporting: boolean;
  error: string | null;
}) {
  const [key, setKey] = useState("");

  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 px-4">
      <p className="text-sm text-[var(--text-secondary)]">Import Private Key</p>

      <input
        type="password"
        value={key}
        onChange={(e) => setKey(e.target.value)}
        placeholder="0x..."
        className="w-full max-w-md px-3 py-2 text-xs font-mono rounded bg-white/5 border border-[var(--border-glow)] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[var(--accent-blue)]"
      />

      {error && (
        <p className="text-[11px] text-red-400">{error}</p>
      )}

      <div className="flex gap-2">
        <button
          onClick={() => onSubmit(key)}
          disabled={isImporting || !key}
          className="px-3 py-1.5 text-xs font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {isImporting ? "Importing..." : "Import"}
        </button>
        <button
          onClick={onCancel}
          className="px-3 py-1.5 text-xs font-semibold rounded-xl border border-[var(--border-glow)] text-[var(--text-secondary)] hover:bg-white/5 transition-colors"
        >
          Cancel
        </button>
      </div>

      <InfoNote>
        Paste your private key (0x + 64 hex characters). The key is encrypted and stored on the server.
      </InfoNote>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Single wallet card (used in list view)
// ---------------------------------------------------------------------------

function WalletCard({
  wallet,
  onDerive,
  onDelete,
  isDeriving,
  isDeleting,
  addLog,
}: {
  wallet: TradingWalletInfo;
  onDerive: (id: string) => void;
  onDelete: (id: string) => void;
  isDeriving: boolean;
  isDeleting: boolean;
  addLog: (level: "info" | "warn" | "error" | "success", msg: string) => void;
}) {
  return (
    <div className="px-3 py-2.5 rounded border border-[var(--border-glow)] bg-white/[0.02]">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <span className="font-mono text-xs text-[var(--text-primary)]">
            {truncateAddress(wallet.address)}
          </span>
          <CopyButton text={wallet.address} />
        </div>
        <StatusBadge status={wallet.status} />
      </div>

      <div className="space-y-1 text-[11px]">
        {wallet.proxy_address && (
          <div className="flex items-center gap-2">
            <span className="text-[var(--text-muted)] w-10">Proxy</span>
            <span className="font-mono text-[var(--text-secondary)]">
              {truncateAddress(wallet.proxy_address)}
            </span>
            <CopyButton text={wallet.proxy_address} />
          </div>
        )}

        {wallet.has_clob_credentials ? (
          <div className="flex items-center gap-2">
            <span className="text-[var(--text-muted)] w-10">CLOB</span>
            <span className="text-green-400">API key derived</span>
          </div>
        ) : (
          <div className="mt-1 space-y-1">
            <p className="text-[10px] text-[var(--text-muted)]">
              Step 1 — Sign a message to generate your CLOB API key (no gas needed)
            </p>
            <button
              onClick={() => onDerive(wallet.id)}
              disabled={isDeriving}
              className="px-2.5 py-1 text-[11px] font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {isDeriving ? "Deriving..." : "Derive CLOB Credentials"}
            </button>
          </div>
        )}
      </div>

      {/* Balance + funding section for credentialed wallets */}
      {wallet.has_clob_credentials && (
        <BalanceSection
          walletId={wallet.id}
          walletAddress={wallet.address}
          proxyAddress={wallet.proxy_address}
          addLog={addLog}
        />
      )}

      <div className="flex justify-end mt-2">
        <button
          onClick={() => onDelete(wallet.id)}
          disabled={isDeleting}
          className="px-2 py-0.5 text-[10px] font-semibold rounded-xl border border-red-500/20 text-red-400 hover:bg-red-500/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {isDeleting ? "Deleting..." : "Delete"}
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Wallet list view (1-3 wallets)
// ---------------------------------------------------------------------------

const MAX_WALLETS = 3;

function WalletListView({
  wallets,
  onAdd,
  onDerive,
  onDelete,
  derivingId,
  deletingId,
  addLog,
}: {
  wallets: TradingWalletInfo[];
  onAdd: () => void;
  onDerive: (id: string) => void;
  onDelete: (id: string) => void;
  derivingId: string | null;
  deletingId: string | null;
  addLog: (level: "info" | "warn" | "error" | "success", msg: string) => void;
}) {
  return (
    <div className="flex flex-col h-full px-4 py-3 overflow-y-auto">
      <div className="flex items-center justify-between mb-3">
        <p className="text-sm text-[var(--text-secondary)]">
          Trading Wallets ({wallets.length}/{MAX_WALLETS})
        </p>
        {wallets.length < MAX_WALLETS && (
          <button
            onClick={onAdd}
            className="px-2.5 py-1 text-[11px] font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors"
          >
            + Add Wallet
          </button>
        )}
      </div>

      <div className="space-y-2">
        {wallets.map((w) => (
          <WalletCard
            key={w.id}
            wallet={w}
            onDerive={onDerive}
            onDelete={onDelete}
            isDeriving={derivingId === w.id}
            isDeleting={deletingId === w.id}
            addLog={addLog}
          />
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Add wallet choice (shown when clicking "+ Add Wallet")
// ---------------------------------------------------------------------------

function AddWalletView({
  onGenerate,
  onImport,
  onCancel,
  isGenerating,
}: {
  onGenerate: () => void;
  onImport: () => void;
  onCancel: () => void;
  isGenerating: boolean;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 px-4">
      <p className="text-sm text-[var(--text-secondary)]">Add Trading Wallet</p>
      <div className="flex gap-2 mt-2">
        <button
          onClick={onGenerate}
          disabled={isGenerating}
          className="px-3 py-1.5 text-xs font-semibold rounded-xl bg-[var(--accent-blue)]/15 text-[var(--accent-blue)] border border-[var(--accent-blue)]/20 hover:bg-[var(--accent-blue)]/25 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {isGenerating ? "Generating..." : "Generate New"}
        </button>
        <button
          onClick={onImport}
          className="px-3 py-1.5 text-xs font-semibold rounded-xl border border-[var(--border-glow)] text-[var(--text-secondary)] hover:bg-white/5 transition-colors"
        >
          Import Existing
        </button>
        <button
          onClick={onCancel}
          className="px-3 py-1.5 text-xs font-semibold rounded-xl border border-[var(--border-glow)] text-[var(--text-muted)] hover:bg-white/5 transition-colors"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main WalletTab
// ---------------------------------------------------------------------------

type UIState = "loading" | "list" | "add" | "import" | "generate-result";

export function WalletTab() {
  const { isAuthenticated } = useAuth();
  const { data: wallets, isLoading } = useWallets();
  const generateMutation = useGenerateWallet();
  const importMutation = useImportWallet();
  const deriveMutation = useDeriveCredentials();
  const deleteMutation = useDeleteWallet();
  const { setWalletStatus, setWalletBalance, addLog } = useTerminalDispatch();

  const [uiState, setUIState] = useState<UIState>("loading");
  const [generateResult, setGenerateResult] = useState<WalletGenerateResponse | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [derivingId, setDerivingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  // Find the first credentialed wallet to track balance in status bar
  const primaryWallet = wallets?.find((w) => w.has_clob_credentials);
  const { data: primaryBalance } = useWalletBalance(primaryWallet?.id ?? null);

  // Sync query state -> UI state + terminal status
  useEffect(() => {
    if (isLoading) {
      setUIState("loading");
      return;
    }

    // Don't override user-initiated flows
    if (generateResult && uiState === "generate-result") return;
    if (uiState === "import" || uiState === "add") return;

    setUIState("list");

    // Update terminal wallet status
    if (!wallets || wallets.length === 0) {
      setWalletStatus("none");
      setWalletBalance(null);
    } else if (primaryBalance) {
      const allApproved = primaryBalance.ctf_exchange_approved && primaryBalance.neg_risk_exchange_approved;
      const hasFunds = parseFloat(primaryBalance.usdc_balance) > 0;
      setWalletBalance(primaryBalance.usdc_balance);
      if (hasFunds && allApproved) {
        setWalletStatus("funded");
      } else {
        setWalletStatus("setup");
      }
    } else if (wallets.some((w) => w.has_clob_credentials)) {
      setWalletStatus("setup");
    } else {
      setWalletStatus("setup");
    }
  }, [wallets, isLoading, generateResult, uiState, setWalletStatus, setWalletBalance, primaryBalance]);

  if (!isAuthenticated) {
    return (
      <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
        Sign in to configure your trading wallets.
      </div>
    );
  }

  const handleGenerate = async () => {
    try {
      const result = await generateMutation.mutateAsync();
      setGenerateResult(result);
      setUIState("generate-result");
      addLog("success", `Trading wallet generated: ${truncateAddress(result.address)}`);
    } catch (e) {
      addLog("error", `Wallet generation failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const handleImport = async (privateKey: string) => {
    try {
      setImportError(null);
      const result = await importMutation.mutateAsync(privateKey);
      setUIState("list");
      addLog("success", `Wallet imported: ${truncateAddress(result.address)}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setImportError(msg);
      addLog("error", `Wallet import failed: ${msg}`);
    }
  };

  const handleDerive = async (walletId: string) => {
    try {
      setDerivingId(walletId);
      await deriveMutation.mutateAsync(walletId);
      addLog("success", "CLOB credentials derived successfully");
    } catch (e) {
      addLog("error", `Credential derivation failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setDerivingId(null);
    }
  };

  const handleDelete = async (walletId: string) => {
    try {
      setDeletingId(walletId);
      await deleteMutation.mutateAsync(walletId);
      setGenerateResult(null);
      addLog("info", "Trading wallet deleted");
    } catch (e) {
      addLog("error", `Wallet deletion failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setDeletingId(null);
    }
  };

  switch (uiState) {
    case "loading":
      return (
        <div className="flex items-center justify-center h-full text-xs text-[var(--text-muted)]">
          Loading wallets...
        </div>
      );

    case "list":
      if (!wallets || wallets.length === 0) {
        return (
          <NoWalletsView
            onGenerate={handleGenerate}
            onImport={() => setUIState("import")}
            isGenerating={generateMutation.isPending}
          />
        );
      }
      return (
        <WalletListView
          wallets={wallets}
          onAdd={() => setUIState("add")}
          onDerive={handleDerive}
          onDelete={handleDelete}
          derivingId={derivingId}
          deletingId={deletingId}
          addLog={addLog}
        />
      );

    case "add":
      return (
        <AddWalletView
          onGenerate={handleGenerate}
          onImport={() => setUIState("import")}
          onCancel={() => setUIState("list")}
          isGenerating={generateMutation.isPending}
        />
      );

    case "import":
      return (
        <ImportView
          onSubmit={handleImport}
          onCancel={() => { setUIState("list"); setImportError(null); }}
          isImporting={importMutation.isPending}
          error={importError}
        />
      );

    case "generate-result":
      return (
        <GenerateResultView
          result={generateResult!}
          onContinue={() => { setGenerateResult(null); setUIState("list"); }}
        />
      );
  }
}
