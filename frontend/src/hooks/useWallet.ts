import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  fetchWallets,
  generateWallet,
  importWallet,
  deriveCredentials,
  deleteWallet,
  fetchWalletBalance,
  enableTrading,
  approveExchanges,
  fetchDepositAddress,
  fetchDepositStatus,
} from "../api";

const WALLETS_KEY = ["trading-wallets"] as const;
const BALANCE_KEY = (id: string) => ["wallet-balance", id] as const;
const DEPOSIT_ADDR_KEY = (id: string) => ["deposit-address", id] as const;
const DEPOSIT_STATUS_KEY = (id: string) => ["deposit-status", id] as const;

const JWT_KEY = "pd_jwt";

export function useWallets() {
  const hasJwt = !!localStorage.getItem(JWT_KEY);
  return useQuery({
    queryKey: WALLETS_KEY,
    queryFn: fetchWallets,
    enabled: hasJwt,
    staleTime: 30_000,
  });
}

export function useGenerateWallet() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: generateWallet,
    onSuccess: () => qc.invalidateQueries({ queryKey: WALLETS_KEY }),
  });
}

export function useImportWallet() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (privateKey: string) => importWallet(privateKey),
    onSuccess: () => qc.invalidateQueries({ queryKey: WALLETS_KEY }),
  });
}

export function useDeriveCredentials() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (walletId: string) => deriveCredentials(walletId),
    onSuccess: () => qc.invalidateQueries({ queryKey: WALLETS_KEY }),
  });
}

export function useDeleteWallet() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (walletId: string) => deleteWallet(walletId),
    onSuccess: () => qc.invalidateQueries({ queryKey: WALLETS_KEY }),
  });
}

// -- Wallet Funding (spec 14) --

export function useWalletBalance(walletId: string | null) {
  return useQuery({
    queryKey: BALANCE_KEY(walletId ?? ""),
    queryFn: () => fetchWalletBalance(walletId!),
    enabled: !!walletId,
    refetchInterval: 30_000,
    staleTime: 15_000,
  });
}

export function useEnableTrading() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (walletId: string) => enableTrading(walletId),
    onSuccess: (data, walletId) => {
      // Optimistic: hide the Enable Trading button immediately
      if (data.status === "deployed" || data.status === "already_deployed") {
        qc.setQueryData(BALANCE_KEY(walletId), (old: any) =>
          old ? { ...old, safe_deployed: true } : old
        );
      }
      qc.invalidateQueries({ queryKey: WALLETS_KEY });
      // Delayed refetch to confirm on-chain deployment
      setTimeout(() => qc.invalidateQueries({ queryKey: BALANCE_KEY(walletId) }), 8_000);
    },
  });
}

export function useApproveExchanges() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (walletId: string) => approveExchanges(walletId),
    onSuccess: (_data, walletId) => {
      // Optimistic: set approvals to true immediately (backend confirmed success)
      qc.setQueryData(BALANCE_KEY(walletId), (old: any) =>
        old ? { ...old, ctf_exchange_approved: true, neg_risk_exchange_approved: true } : old
      );
      // Delayed refetch to sync on-chain state after relayer tx settles
      // Don't invalidate immediately — it would overwrite the optimistic update
      setTimeout(() => qc.invalidateQueries({ queryKey: BALANCE_KEY(walletId) }), 10_000);
    },
  });
}

export function useDepositAddress(walletId: string | null) {
  return useQuery({
    queryKey: DEPOSIT_ADDR_KEY(walletId ?? ""),
    queryFn: () => fetchDepositAddress(walletId!),
    enabled: !!walletId,
    staleTime: 5 * 60_000,
  });
}

export function useDepositStatus(walletId: string | null) {
  return useQuery({
    queryKey: DEPOSIT_STATUS_KEY(walletId ?? ""),
    queryFn: () => fetchDepositStatus(walletId!),
    enabled: !!walletId,
    refetchInterval: 15_000,
    staleTime: 10_000,
  });
}
