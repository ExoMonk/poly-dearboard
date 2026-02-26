import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  fetchTraderLists,
  fetchTraderListDetail,
  createTraderList,
  renameTraderList,
  deleteTraderList,
  addListMembers,
  removeListMembers,
} from "../api";

const LISTS_KEY = ["trader-lists"] as const;

const JWT_KEY = "pd_jwt";

export function useTraderLists() {
  const hasJwt = !!localStorage.getItem(JWT_KEY);
  return useQuery({
    queryKey: LISTS_KEY,
    queryFn: fetchTraderLists,
    enabled: hasJwt,
    staleTime: 30_000,
  });
}

export function useTraderListDetail(id: string | null) {
  return useQuery({
    queryKey: ["trader-list", id],
    queryFn: () => fetchTraderListDetail(id!),
    enabled: !!id,
    staleTime: 30_000,
  });
}

export function useCreateList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => createTraderList(name),
    onSuccess: () => qc.invalidateQueries({ queryKey: LISTS_KEY }),
  });
}

export function useDeleteList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteTraderList(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: LISTS_KEY }),
  });
}

export function useRenameList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => renameTraderList(id, name),
    onSuccess: () => qc.invalidateQueries({ queryKey: LISTS_KEY }),
  });
}

export function useAddMembers() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, addresses }: { id: string; addresses: string[] }) =>
      addListMembers(id, addresses),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: LISTS_KEY });
      qc.invalidateQueries({ queryKey: ["trader-list", vars.id] });
    },
  });
}

export function useRemoveMembers() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, addresses }: { id: string; addresses: string[] }) =>
      removeListMembers(id, addresses),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: LISTS_KEY });
      qc.invalidateQueries({ queryKey: ["trader-list", vars.id] });
    },
  });
}
