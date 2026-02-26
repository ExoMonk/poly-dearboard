import { useState, useRef, useEffect } from "react";
import { motion, AnimatePresence } from "motion/react";
import { useTraderLists, useAddMembers } from "../hooks/useTraderLists";
import { useAuth } from "../context/AuthContext";
import { tapScale } from "../lib/motion";

interface Props {
  address: string;
}

export default function AddToListButton({ address }: Props) {
  const { isAuthenticated } = useAuth();
  const { data: lists } = useTraderLists();
  const addMembers = useAddMembers();
  const [open, setOpen] = useState(false);
  const [addedTo, setAddedTo] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    if (open) document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  if (!isAuthenticated) return null;

  function handleAdd(listId: string) {
    addMembers.mutate(
      { id: listId, addresses: [address] },
      {
        onSuccess: () => {
          setAddedTo(listId);
          setTimeout(() => {
            setAddedTo(null);
            setOpen(false);
          }, 1200);
        },
      },
    );
  }

  return (
    <div ref={ref} className="relative">
      <motion.button
        whileTap={tapScale}
        onClick={(e) => {
          e.stopPropagation();
          setOpen(!open);
        }}
        className="inline-flex items-center gap-1 px-2 py-1 rounded-md text-[var(--text-secondary)] hover:text-[var(--accent-blue)] hover:bg-[var(--accent-blue)]/10 transition-all cursor-pointer text-[10px] font-medium whitespace-nowrap"
        title="Add to list"
      >
        <span className="text-sm font-bold leading-none">+</span>
        <span className="hidden sm:inline">Add trader</span>
      </motion.button>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: -4 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: -4 }}
            transition={{ duration: 0.15 }}
            className="absolute right-0 top-full mt-1 z-50 min-w-[180px] py-1.5 rounded-lg bg-[var(--bg-card)] border border-[var(--border-glow)] shadow-xl"
          >
            {!lists?.length ? (
              <div className="px-3 py-2 text-xs text-[var(--text-secondary)]">
                No lists yet. Create one in PolyLab.
              </div>
            ) : (
              lists.map((list) => (
                <button
                  key={list.id}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleAdd(list.id);
                  }}
                  disabled={addMembers.isPending}
                  className="w-full text-left px-3 py-2 text-sm text-[var(--text-primary)] hover:bg-[var(--accent-blue)]/10 transition-colors cursor-pointer flex items-center justify-between"
                >
                  <span className="truncate">{list.name}</span>
                  {addedTo === list.id && (
                    <span className="text-[var(--neon-green)] text-xs font-semibold ml-2">Added</span>
                  )}
                </button>
              ))
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
