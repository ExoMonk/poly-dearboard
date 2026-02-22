import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  fetchWallets,
  generateWallet,
  importWallet,
  deriveCredentials,
  deleteWallet,
} from "../api";

const WALLETS_KEY = ["trading-wallets"] as const;

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
