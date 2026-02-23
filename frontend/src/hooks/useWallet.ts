import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  fetchWallets,
  generateWallet,
  importWallet,
  deriveCredentials,
  deleteWallet,
  fetchWalletBalance,
  approveExchanges,
  fetchDepositAddress,
  fetchDepositStatus,
} from "../api";

const WALLETS_KEY = ["trading-wallets"] as const;
const BALANCE_KEY = (id: string) => ["wallet-balance", id] as const;
const DEPOSIT_ADDR_KEY = (id: string) => ["deposit-address", id] as const;
const DEPOSIT_STATUS_KEY = (id: string) => ["deposit-status", id] as const;

export function useWallets() {
  return useQuery({
    queryKey: WALLETS_KEY,
    queryFn: fetchWallets,
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

export function useApproveExchanges() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (walletId: string) => approveExchanges(walletId),
    onSuccess: (_data, walletId) => {
      qc.invalidateQueries({ queryKey: BALANCE_KEY(walletId) });
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
