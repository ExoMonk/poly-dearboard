import { useState, useCallback, createContext, useContext } from "react";
import { AnimatePresence, motion } from "motion/react";

type ToastLevel = "success" | "info" | "error" | "warn";

interface Toast {
  id: number;
  level: ToastLevel;
  message: string;
}

interface ToastContextType {
  toast: (level: ToastLevel, message: string) => void;
}

const ToastContext = createContext<ToastContextType>({ toast: () => {} });

export function useToast() {
  return useContext(ToastContext);
}

const LEVEL_STYLES: Record<ToastLevel, string> = {
  success: "border-green-500/40 bg-green-500/10 text-green-300",
  info: "border-blue-500/40 bg-blue-500/10 text-blue-300",
  error: "border-red-500/40 bg-red-500/10 text-red-300",
  warn: "border-yellow-500/40 bg-yellow-500/10 text-yellow-300",
};

const LEVEL_ICONS: Record<ToastLevel, string> = {
  success: "✓",
  info: "→",
  error: "✗",
  warn: "⚠",
};

let nextId = 0;

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback((level: ToastLevel, message: string) => {
    const id = ++nextId;
    setToasts((prev) => [...prev.slice(-4), { id, level, message }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 4000);
  }, []);

  return (
    <ToastContext.Provider value={{ toast: addToast }}>
      {children}
      <div className="fixed bottom-12 right-4 z-50 flex flex-col gap-2 pointer-events-none">
        <AnimatePresence>
          {toasts.map((t) => (
            <motion.div
              key={t.id}
              initial={{ opacity: 0, x: 80, scale: 0.95 }}
              animate={{ opacity: 1, x: 0, scale: 1 }}
              exit={{ opacity: 0, x: 80, scale: 0.95 }}
              transition={{ duration: 0.2 }}
              className={`pointer-events-auto border rounded-lg px-3 py-2 text-xs font-mono backdrop-blur-sm max-w-xs ${LEVEL_STYLES[t.level]}`}
            >
              <span className="mr-1.5 opacity-70">{LEVEL_ICONS[t.level]}</span>
              {t.message}
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ToastContext.Provider>
  );
}
